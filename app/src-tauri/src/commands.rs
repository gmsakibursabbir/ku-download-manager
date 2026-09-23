//! IPC commands exposed to the UI. Thin wrappers around KuCore.

use crate::AppState;
use base64::Engine as _;
use ku_proto::*;
use kucore::{CoreEvent, Queue, Schedule, Settings};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

type R<T> = Result<T, String>;

fn e(err: impl std::fmt::Display) -> String {
    err.to_string()
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_ready,
        app_info,
        list_downloads,
        add_download,
        add_batch,
        probe_url,
        pause,
        resume,
        remove,
        pause_all,
        resume_all,
        clear_finished,
        edit_download,
        move_to_queue,
        reorder,
        get_details,
        verify_hash,
        open_file,
        open_folder,
        get_settings,
        save_settings,
        set_profile,
        list_queues,
        save_queue,
        delete_queue,
        start_queue,
        stop_queue,
        list_schedules,
        save_schedule,
        delete_schedule,
        set_after_all,
        get_after_all,
        cancel_power,
        media_analyze,
        media_download,
        torrent_info,
        engine_info,
        update_ytdlp,
        native_host_status,
        native_host_register,
        native_host_unregister,
        read_clipboard,
        stats,
        check_update,
        install_update,
        open_data_dir,
        grab_page,
        extension_dirs,
        reveal_path,
    ]
}

pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    let al = app.autolaunch();
    let current = al.is_enabled().unwrap_or(false);
    let r = match (enabled, current) {
        (true, false) => al.enable(),
        (false, true) => al.disable(),
        _ => Ok(()),
    };
    if let Err(err) = r {
        tracing::warn!("autostart: {err}");
    }
}

/// Called once by the UI after its first render: shows the window (unless
/// started hidden) and returns events that arrived while it was loading.
#[tauri::command]
fn app_ready(app: AppHandle, state: State<'_, AppState>) -> Vec<CoreEvent> {
    let first = !state.ui_ready.swap(true, Ordering::SeqCst);
    if first && !state.hidden_start {
        crate::show_main(&app);
    }
    std::mem::take(&mut *state.pending.lock().unwrap())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    data_dir: String,
    api_port: Option<u16>,
    platform: String,
    default_download_dir: String,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: paths::data_dir().display().to_string(),
        api_port: *state.api_port.lock().unwrap(),
        platform: std::env::consts::OS.into(),
        default_download_dir: paths::default_download_dir().display().to_string(),
    }
}

#[tauri::command]
fn list_downloads(state: State<'_, AppState>) -> Vec<Download> {
    state.core.list()
}

#[tauri::command]
fn stats(state: State<'_, AppState>) -> Stats {
    state.core.stats()
}

#[tauri::command]
async fn add_download(state: State<'_, AppState>, req: AddRequest) -> R<Download> {
    state.core.add(req).await.map_err(e)
}

#[tauri::command]
async fn add_batch(state: State<'_, AppState>, urls: Vec<String>, template: AddRequest) -> R<Value> {
    let (ok, failed) = state.core.add_batch(urls, template).await;
    Ok(json!({
        "added": ok,
        "failed": failed.into_iter().map(|(u, e)| json!({"url": u, "error": e})).collect::<Vec<_>>(),
    }))
}

#[tauri::command]
async fn probe_url(state: State<'_, AppState>, url: String, options: Option<DownloadOptions>) -> R<ProbeInfo> {
    let s = state.core.settings();
    let opts = options.unwrap_or_default();
    let url = url.trim().to_string();
    if !kucore::classify::scheme_allowed(&url) {
        return Err("Unsupported address. KuDownloader accepts http, https, ftp, sftp and magnet links.".into());
    }
    let mut info = match kucore::classify::classify(&url, &s.categories) {
        kucore::classify::Route::Ytdlp => ProbeInfo { url: url.clone(), final_url: url.clone(), engine: Some(Engine::Ytdlp), kind: Some(Kind::Media), ..Default::default() },
        kucore::classify::Route::Aria2(k) if k != Kind::Http => ProbeInfo {
            url: url.clone(),
            final_url: url.clone(),
            engine: Some(Engine::Aria2),
            kind: Some(k),
            filename: kucore::classify::filename_from_url(&url),
            ..Default::default()
        },
        _ => kucore::probe::probe(&url, &opts, &s).await,
    };
    if let Some(n) = &info.filename {
        info.category = Some(kucore::classify::category_for(n, &s.categories, info.kind.unwrap_or(Kind::Http)));
    }
    Ok(info)
}

#[tauri::command]
async fn pause(state: State<'_, AppState>, ids: Vec<String>) -> R<()> {
    state.core.pause(&ids).await.map_err(e)
}

#[tauri::command]
async fn resume(state: State<'_, AppState>, ids: Vec<String>) -> R<()> {
    state.core.resume(&ids).await.map_err(e)
}

#[tauri::command]
async fn remove(state: State<'_, AppState>, ids: Vec<String>, delete_files: bool) -> R<()> {
    state.core.remove(&ids, delete_files).await.map_err(e)
}

#[tauri::command]
async fn pause_all(state: State<'_, AppState>) -> R<()> {
    state.core.pause_all().await.map_err(e)
}

#[tauri::command]
async fn resume_all(state: State<'_, AppState>) -> R<()> {
    state.core.resume_all().await.map_err(e)
}

#[tauri::command]
fn clear_finished(state: State<'_, AppState>) -> R<Vec<String>> {
    state.core.clear_finished().map_err(e)
}

#[tauri::command]
async fn edit_download(state: State<'_, AppState>, id: String, patch: Value) -> R<Download> {
    state.core.edit(&id, patch).await.map_err(e)
}

#[tauri::command]
fn move_to_queue(state: State<'_, AppState>, ids: Vec<String>, queue_id: Option<String>) -> R<()> {
    state.core.move_to_queue(&ids, queue_id).map_err(e)
}

#[tauri::command]
fn reorder(state: State<'_, AppState>, id: String, direction: String) -> R<()> {
    state.core.reorder(&id, &direction).map_err(e)
}

#[tauri::command]
async fn get_details(state: State<'_, AppState>, id: String) -> R<Value> {
    state.core.details(&id).await.map_err(e)
}

#[tauri::command]
async fn verify_hash(state: State<'_, AppState>, id: String, algo: String) -> R<String> {
    state.core.verify(&id, &algo).await.map_err(e)
}

fn target_path(d: &Download) -> PathBuf {
    d.file_path.clone().map(PathBuf::from).unwrap_or_else(|| Path::new(&d.dir).join(&d.name))
}

/// Open a finished file with its default application. Only ever triggered by
/// an explicit user action in the UI; downloads are never opened automatically.
#[tauri::command]
fn open_file(app: AppHandle, state: State<'_, AppState>, id: String) -> R<()> {
    let d = state.core.get(&id).ok_or("Download not found")?;
    if !d.status.is_finished() {
        return Err("The download has not finished yet.".into());
    }
    let p = target_path(&d);
    if !p.exists() {
        return Err(format!("The file was moved or deleted: {}", p.display()));
    }
    app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(e)
}

#[tauri::command]
fn open_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> R<()> {
    let d = state.core.get(&id).ok_or("Download not found")?;
    let p = target_path(&d);
    if p.exists() {
        return app.opener().reveal_item_in_dir(&p).map_err(e);
    }
    let dir = Path::new(&d.dir);
    if dir.exists() {
        return app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(e);
    }
    Err(format!("The folder does not exist: {}", dir.display()))
}

#[tauri::command]
fn open_data_dir(app: AppHandle) -> R<()> {
    app.opener().open_path(paths::data_dir().to_string_lossy(), None::<&str>).map_err(e)
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.core.settings()
}

#[tauri::command]
async fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> R<Settings> {
    let old = state.core.settings();
    let s = state.core.save_settings(settings).await.map_err(e)?;
    if old.start_with_os != s.start_with_os {
        sync_autostart(&app, s.start_with_os);
    }
    if old.extra_extension_ids != s.extra_extension_ids {
        let _ = kucore::nativehost::register(&s.extra_extension_ids);
    }
    Ok(s)
}

#[tauri::command]
async fn set_profile(state: State<'_, AppState>, id: String) -> R<()> {
    state.core.set_profile(&id).await.map_err(e)
}

#[tauri::command]
fn list_queues(state: State<'_, AppState>) -> Vec<Queue> {
    state.core.queues()
}

#[tauri::command]
fn save_queue(state: State<'_, AppState>, queue: Queue) -> R<Queue> {
    state.core.save_queue(queue).map_err(e)
}

#[tauri::command]
fn delete_queue(state: State<'_, AppState>, id: String) -> R<()> {
    state.core.delete_queue(&id).map_err(e)
}

#[tauri::command]
async fn start_queue(state: State<'_, AppState>, id: String) -> R<()> {
    state.core.start_queue(&id, None).await.map_err(e)
}

#[tauri::command]
async fn stop_queue(state: State<'_, AppState>, id: String) -> R<()> {
    state.core.stop_queue(&id).await.map_err(e)
}

#[tauri::command]
fn list_schedules(state: State<'_, AppState>) -> Vec<Schedule> {
    state.core.schedules()
}

#[tauri::command]
fn save_schedule(state: State<'_, AppState>, schedule: Schedule) -> R<Schedule> {
    state.core.save_schedule(schedule).map_err(e)
}

#[tauri::command]
fn delete_schedule(state: State<'_, AppState>, id: String) -> R<()> {
    state.core.delete_schedule(&id).map_err(e)
}

#[tauri::command]
fn set_after_all(state: State<'_, AppState>, action: String) {
    state.core.set_after_all(&action)
}

#[tauri::command]
fn get_after_all(state: State<'_, AppState>) -> String {
    state.core.after_all()
}

#[tauri::command]
fn cancel_power(state: State<'_, AppState>) {
    state.core.cancel_power()
}

#[tauri::command]
async fn media_analyze(state: State<'_, AppState>, url: String, playlist: bool) -> R<MediaInfo> {
    state.core.analyze(&url, playlist, Vec::new(), None).await.map_err(e)
}

#[tauri::command]
async fn media_download(state: State<'_, AppState>, req: MediaRequest) -> R<Download> {
    state.core.add_media(req).await.map_err(e)
}

/// Parse a .torrent from a path (native file picker) or base64 (drag & drop).
#[tauri::command]
fn torrent_info(path: Option<String>, data: Option<String>) -> R<Value> {
    let bytes = match (path, data) {
        (Some(p), _) => {
            let meta = std::fs::metadata(&p).map_err(e)?;
            if meta.len() > 20 << 20 {
                return Err("This .torrent file is too large.".into());
            }
            std::fs::read(&p).map_err(e)?
        }
        (None, Some(d)) => base64::engine::general_purpose::STANDARD.decode(d.trim()).map_err(e)?,
        _ => return Err("No torrent given".into()),
    };
    let info = kucore::torrent::parse(&bytes).map_err(e)?;
    Ok(json!({ "info": info, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) }))
}

#[tauri::command]
async fn engine_info(state: State<'_, AppState>) -> R<Value> {
    Ok(state.core.engine_info().await)
}

#[tauri::command]
async fn update_ytdlp(state: State<'_, AppState>) -> R<String> {
    state.core.update_ytdlp().await.map_err(e)
}

#[tauri::command]
fn native_host_status() -> kucore::nativehost::HostStatus {
    kucore::nativehost::status()
}

#[tauri::command]
fn native_host_register(state: State<'_, AppState>) -> R<kucore::nativehost::HostStatus> {
    kucore::nativehost::register(&state.core.settings().extra_extension_ids).map_err(e)
}

#[tauri::command]
fn native_host_unregister() -> R<kucore::nativehost::HostStatus> {
    kucore::nativehost::unregister().map_err(e)
}

#[tauri::command]
fn read_clipboard() -> Option<String> {
    crate::clipboard::read_text()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    current_version: String,
    notes: Option<String>,
    date: Option<String>,
}

async fn find_update(app: &AppHandle, state: &AppState) -> R<Option<tauri_plugin_updater::Update>> {
    let s = state.core.settings();
    let mut b = app.updater_builder();
    if !s.update_endpoint.trim().is_empty() {
        let url = url::Url::parse(s.update_endpoint.trim()).map_err(|_| "The update feed URL is not valid.".to_string())?;
        b = b.endpoints(vec![url]).map_err(e)?;
    }
    let updater = b.build().map_err(e)?;
    updater.check().await.map_err(|err| format!("Could not check for updates: {err}"))
}

#[tauri::command]
async fn check_update(app: AppHandle, state: State<'_, AppState>) -> R<Option<UpdateInfo>> {
    Ok(find_update(&app, &state).await?.map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    }))
}

#[tauri::command]
async fn install_update(app: AppHandle, state: State<'_, AppState>) -> R<()> {
    let u = find_update(&app, &state).await?.ok_or("KuDownloader is up to date.")?;
    u.download_and_install(|_, _| {}, || {}).await.map_err(|err| format!("Update failed: {err}"))?;
    state.core.shutdown().await;
    app.restart();
}


#[tauri::command]
async fn grab_page(state: State<'_, AppState>, url: String) -> R<Vec<GrabLink>> {
    kucore::grab::grab_page(&url, &state.core.settings()).await.map_err(e)
}

/// Where the unpacked browser extensions live (bundled resources, or the
/// repository build output during development).
#[tauri::command]
fn extension_dirs(app: AppHandle) -> Value {
    let mut roots = Vec::new();
    if let Ok(r) = app.path().resource_dir() {
        roots.push(r.join("extension"));
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extension/dist"));
    let find = |name: &str| {
        roots
            .iter()
            .map(|r| r.join(name))
            .find(|p| p.join("manifest.json").is_file())
            .and_then(|p| p.canonicalize().ok())
            .map(|p| p.display().to_string().trim_start_matches(r"\\?\").to_string())
    };
    json!({ "chrome": find("chrome"), "firefox": find("firefox") })
}

/// Open a folder in the file manager (folders only).
#[tauri::command]
fn reveal_path(app: AppHandle, path: String) -> R<()> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err("Folder not found".into());
    }
    app.opener().open_path(p.to_string_lossy(), None::<&str>).map_err(e)
}
