#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod commands;
mod i18n;
mod platform;
mod tray;

use ku_proto::{paths, AddRequest};
use kucore::{api, db::Db, Core, CoreEvent};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    pub core: Arc<Core>,
    pub hidden_start: bool,
    /// Events that arrived before the UI finished loading.
    pub pending: Mutex<Vec<CoreEvent>>,
    pub ui_ready: AtomicBool,
    pub api_port: Mutex<Option<u16>>,
    /// Requests waiting for their "Download File" popup window to pick them up.
    pub prompts: Mutex<HashMap<String, AddRequest>>,
    pub prompt_seq: AtomicU64,
    /// KuAirSend (None when its device certificate could not be created).
    pub airsend: Option<Arc<kucore::airsend::AirSend>>,
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

/// IDM-style "Download File" window for a download caught in the browser:
/// small, always on top, independent of the main window (which may be in the tray).
fn open_prompt_window(app: &AppHandle, request: AddRequest) {
    let state = app.state::<AppState>();
    let id = state.prompt_seq.fetch_add(1, Ordering::Relaxed).to_string();
    state.prompts.lock().unwrap().insert(id.clone(), request);
    let dark = prefers_dark(app);
    let bg = if dark { (0x16, 0x16, 0x18, 255) } else { (0xF5, 0xF5, 0xF7, 255) };
    let built = WebviewWindowBuilder::new(app, format!("prompt-{id}"), WebviewUrl::App(format!("index.html#prompt={id}").into()))
        .title(i18n::tr("Download File"))
        .inner_size(600.0, 400.0)
        .min_inner_size(460.0, 260.0)
        .decorations(false)
        .always_on_top(true)
        .center()
        .focused(true)
        .visible(false)
        .background_color(bg.into())
        .build();
    // Several at once: cascade instead of stacking exactly on top of each other.
    if let Ok(w) = &built {
        let others = app.webview_windows().keys().filter(|l| l.starts_with("prompt-")).count().saturating_sub(1);
        if others > 0 {
            if let Ok(p) = w.outer_position() {
                let step = (28.0 * w.scale_factor().unwrap_or(1.0)) as i32 * others.min(8) as i32;
                let _ = w.set_position(tauri::PhysicalPosition::new(p.x + step, p.y + step));
            }
        }
    }
    if let Err(e) = built {
        // Fall back to the dialog inside the main window.
        tracing::warn!("prompt window: {e}");
        if let Some(r) = state.prompts.lock().unwrap().remove(&id) {
            show_main(app);
            let _ = app.emit("ku", &CoreEvent::PromptAdd { request: Box::new(r) });
        }
    }
}

/// IDM-style progress window for one download (focused if already open).
pub fn open_progress_window(app: &AppHandle, id: &str) -> tauri::Result<()> {
    let label = format!("progress-{}", id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>());
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        return w.set_focus();
    }
    let dark = prefers_dark(app);
    let bg = if dark { (0x16, 0x16, 0x18, 255) } else { (0xF5, 0xF5, 0xF7, 255) };
    let w = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(format!("index.html#progress={id}").into()))
        .title(i18n::tr("Download progress"))
        .inner_size(560.0, 360.0)
        .min_inner_size(440.0, 240.0)
        .decorations(false)
        .center()
        .focused(true)
        .visible(false)
        .background_color(bg.into())
        .build()?;
    let others = app.webview_windows().keys().filter(|l| l.starts_with("progress-")).count().saturating_sub(1);
    if others > 0 {
        if let Ok(p) = w.outer_position() {
            let step = (28.0 * w.scale_factor().unwrap_or(1.0)) as i32 * others.min(8) as i32;
            let _ = w.set_position(tauri::PhysicalPosition::new(p.x + step, p.y + step));
        }
    }
    Ok(())
}

/// Dark or light for a new window: the setting, or the OS for "system".
fn prefers_dark(app: &AppHandle) -> bool {
    match app.state::<AppState>().core.settings().theme.as_str() {
        "light" => false,
        "dark" => true,
        _ => app.get_webview_window("main").and_then(|w| w.theme().ok()).map(|t| t == tauri::Theme::Dark).unwrap_or(true),
    }
}

fn progress_windows_open(app: &AppHandle) -> bool {
    app.webview_windows().keys().any(|l| l.starts_with("progress-"))
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
        .unwrap_or(false)
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
                    // Nothing visible to paint (main hidden, no progress window): skip the webviews.
                    if !window_visible(&app) && !progress_windows_open(&app) {
                        continue;
                    }
                }
                CoreEvent::Completed { name, .. } if s.notify_complete => notify(&app, &i18n::tr("Download complete"), name),
                CoreEvent::Notice { level, title, message, .. } if level == "error" && s.notify_error && !window_visible(&app) => {
                    notify(&app, &i18n::te(title), &i18n::te(message))
                }
                CoreEvent::QueueDone { name, .. } if s.notify_queue_done => notify(&app, &i18n::tr("Queue finished"), &i18n::trf("All downloads in “{name}” are done.", &[("name", name)])),
                CoreEvent::PowerCountdown { action, seconds } => {
                    show_main(&app);
                    let key = if action == "sleep" { "Your computer will sleep in {n} seconds." } else { "Your computer will shut down in {n} seconds." };
                    notify(&app, &i18n::tr("Downloads finished"), &i18n::trf(key, &[("n", &seconds.to_string())]));
                }
                CoreEvent::Show => show_main(&app),
                CoreEvent::PromptAdd { request } if request.source.as_deref() == Some("browser") => {
                    open_prompt_window(&app, (**request).clone());
                    continue;
                }
                CoreEvent::PromptAdd { .. } => show_main(&app),
                CoreEvent::AirSendRequest { request } => {
                    show_main(&app);
                    let what = if request.file_count == 1 {
                        request.files.first().map(|f| format!("“{}”", f.name)).unwrap_or_else(|| i18n::tr("1 file"))
                    } else {
                        i18n::trf("{n} files", &[("n", &request.file_count.to_string())])
                    };
                    notify(&app, "KuAirSend", &i18n::trf("{peer} wants to send you {files}.", &[("peer", &request.peer), ("files", &what)]));
                }
                CoreEvent::AirSendMessage { message } if !window_visible(&app) => notify(&app, &i18n::trf("Message from {peer}", &[("peer", &message.peer)]), &message.text),
                CoreEvent::AirSendTransfer { transfer } if transfer.direction == "receive" && transfer.state == "done" && transfer.text.is_none() && !window_visible(&app) => {
                    let body = if transfer.file_count == 1 {
                        i18n::trf("Received 1 file from {peer}.", &[("peer", &transfer.peer)])
                    } else {
                        i18n::trf("Received {n} files from {peer}.", &[("n", &transfer.file_count.to_string()), ("peer", &transfer.peer)])
                    };
                    notify(&app, "KuAirSend", &body)
                }
                CoreEvent::ClipboardUrl { url } if !window_visible(&app) => notify(&app, &i18n::tr("Link copied"), &format!("{url}\n{}", i18n::tr("Open KuDownloader to download it."))),
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

    let airsend = kucore::airsend::AirSend::new(core.clone()).map_err(|e| tracing::error!("KuAirSend unavailable: {e:#}")).ok();
    let setup_air = airsend.clone();
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
            prompts: Mutex::new(HashMap::new()),
            prompt_seq: AtomicU64::new(1),
            airsend,
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
            if let Some(air) = setup_air.clone().filter(|_| core.settings().airsend_enabled) {
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = air.start().await {
                        tracing::warn!("KuAirSend: {e:#}");
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let Some(id) = window.label().strip_prefix("prompt-") {
                if let WindowEvent::Destroyed = event {
                    window.state::<AppState>().prompts.lock().unwrap().remove(id);
                }
                return;
            }
            if window.label() != "main" {
                return;
            }
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
