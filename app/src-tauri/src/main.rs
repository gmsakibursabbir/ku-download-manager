#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod commands;
mod tray;

use ku_proto::{paths, AddRequest};
use kucore::{api, db::Db, Core, CoreEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    pub core: Arc<Core>,
    pub hidden_start: bool,
    /// Events that arrived before the UI finished loading.
    pub pending: Mutex<Vec<CoreEvent>>,
    pub ui_ready: AtomicBool,
    pub api_port: Mutex<Option<u16>>,
}

fn init_logging() {
    let dir = paths::data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let filter = tracing_subscriber::EnvFilter::try_from_env("KU_LOG").unwrap_or_else(|_| "kucore=info,kudownloader=info".into());
    let file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("kudownloader.log"));
    match file {
        Ok(f) if !cfg!(debug_assertions) => {
            let _ = tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).with_writer(Mutex::new(f)).try_init();
        }
        _ => {
            let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
        }
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main").and_then(|w| w.is_visible().ok()).unwrap_or(false)
}

/// Launch arguments: URLs, magnet links or .torrent paths (also forwarded
/// from a second instance).
fn handle_args(app: &AppHandle, args: &[String]) {
    let state = app.state::<AppState>();
    for a in args.iter().skip(1).filter(|a| !a.starts_with("--")) {
        let lower = a.to_ascii_lowercase();
        let req = if kucore::classify::scheme_allowed(a) {
            Some(AddRequest { url: a.clone(), source: Some("launch".into()), ..Default::default() })
        } else if lower.ends_with(".torrent") {
            std::fs::read(a).ok().filter(|b| b.len() < 20 << 20).map(|bytes| {
                use base64::Engine as _;
                let mut r = AddRequest { source: Some("launch".into()), ..Default::default() };
                r.options.torrent_data = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
                r.filename = std::path::Path::new(a).file_stem().map(|s| s.to_string_lossy().into_owned());
                r
            })
        } else {
            None
        };
        if let Some(r) = req {
            state.core.emit(CoreEvent::Show);
            state.core.emit(CoreEvent::PromptAdd { request: Box::new(r) });
        }
    }
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn forward_events(app: AppHandle, core: Arc<Core>) {
    let mut rx = core.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            };
            let s = core.settings();
            match &ev {
                CoreEvent::Progress { download_speed, upload_speed, items } => {
                    tray::update_tooltip(&app, *download_speed, *upload_speed, items.len());
                }
                CoreEvent::Completed { name, .. } if s.notify_complete => notify(&app, "Download complete", name),
                CoreEvent::Notice { level, title, message, .. } if level == "error" && s.notify_error && !window_visible(&app) => {
                    notify(&app, title, message)
                }
                CoreEvent::QueueDone { name, .. } if s.notify_queue_done => notify(&app, "Queue finished", &format!("All downloads in “{name}” are done.")),
                CoreEvent::PowerCountdown { action, seconds } => {
                    show_main(&app);
                    notify(&app, "Downloads finished", &format!("Your computer will {} in {seconds} seconds.", if action == "sleep" { "sleep" } else { "shut down" }));
                }
                CoreEvent::Show => show_main(&app),
                CoreEvent::ClipboardUrl { url } if !window_visible(&app) => notify(&app, "Link copied", &format!("{url}\nOpen KuDownloader to download it.")),
                _ => {}
            }
            let state = app.state::<AppState>();
            if !state.ui_ready.load(Ordering::SeqCst) {
                if ev.is_prompt() {
                    state.pending.lock().unwrap().push(ev);
                }
                continue;
            }
            let _ = app.emit("ku", &ev);
        }
    });
}

fn main() {
    init_logging();
    let args: Vec<String> = std::env::args().collect();
    let hidden = args.iter().any(|a| a == "--hidden" || a == "--minimized");

    let db = match Db::open(&paths::db_path()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("KuDownloader could not open its database: {e:#}");
            std::process::exit(1);
        }
    };
    let core = Core::open(db).expect("initializing KuCore");

    let setup_core = core.clone();
    let exit_core = core.clone();
    let launch_args = args.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main(app);
            handle_args(app, &argv);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            core: core.clone(),
            hidden_start: hidden,
            pending: Mutex::new(Vec::new()),
            ui_ready: AtomicBool::new(false),
            api_port: Mutex::new(None),
        })
        .invoke_handler(commands::handler())
        .setup(move |app| {
            let handle = app.handle().clone();
            let core = setup_core.clone();
            forward_events(handle.clone(), core.clone());
            let api_core = core.clone();
            let port = tauri::async_runtime::block_on(async move {
                api_core.start();
                api::serve(api_core.clone()).await
            });
            match port {
                Ok(info) => *app.state::<AppState>().api_port.lock().unwrap() = Some(info.port),
                Err(e) => tracing::error!("local API unavailable: {e:#}"),
            }
            let quit_handle = handle.clone();
            core.set_quit_hook(move || quit_handle.exit(0));
            tray::create(&handle)?;
            clipboard::start(core.clone());
            // Keep browser registration pointing at this installation.
            if let Err(e) = kucore::nativehost::register(&core.settings().extra_extension_ids) {
                tracing::warn!("native messaging registration: {e:#}");
            }
            commands::sync_autostart(&handle, core.settings().start_with_os);
            handle_args(&handle, &launch_args);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.core.settings().minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("building KuDownloader");

    app.run(move |_app, event| {
        if let RunEvent::Exit = event {
            let core = exit_core.clone();
            tauri::async_runtime::block_on(async move { core.shutdown().await });
            api::remove_api_file();
        }
    });
}
