//! Local API on 127.0.0.1 used by the CLI and the native messaging host.
//!
//! Security: loopback only, random port, bearer token stored in the per-user
//! data directory, and requests carrying an `Origin` header (i.e. coming from
//! a web page) or a foreign `Host` (DNS rebinding) are refused.

use crate::core::Core;
use crate::types::CoreEvent;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ku_proto::{paths, AddRequest, ApiInfo, GrabRequest, MediaRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
struct Ctx {
    core: Arc<Core>,
    token: Arc<String>,
    port: u16,
}

struct ApiErr(StatusCode, String);

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<anyhow::Error> for ApiErr {
    fn from(e: anyhow::Error) -> Self {
        ApiErr(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
    }
}

type R<T> = Result<Json<T>, ApiErr>;

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn guard(State(ctx): State<Ctx>, req: Request, next: Next) -> Response {
    let h = req.headers();
    if h.contains_key(header::ORIGIN) {
        return ApiErr(StatusCode::FORBIDDEN, "browser requests are not accepted".into()).into_response();
    }
    let host_ok = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("127.0.0.1:{}", ctx.port) || v == format!("localhost:{}", ctx.port));
    let auth_ok = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.as_bytes(), ctx.token.as_bytes()));
    if !host_ok || !auth_ok {
        return ApiErr(StatusCode::UNAUTHORIZED, "unauthorized".into()).into_response();
    }
    next.run(req).await
}

pub async fn serve(core: Arc<Core>) -> anyhow::Result<ApiInfo> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let info = ApiInfo { port, token: token.clone(), pid: std::process::id(), version: env!("CARGO_PKG_VERSION").into() };
    let ctx = Ctx { core, token: Arc::new(token), port };
    let app = Router::new()
        .route("/v1/health", get(|| async { Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})) }))
        .route("/v1/stats", get(stats))
        .route("/v1/config/browser", get(browser_config))
        .route("/v1/downloads", get(list).post(add))
        .route("/v1/downloads/batch", post(add_batch))
        .route("/v1/downloads/{id}", get(get_one).delete(remove))
        .route("/v1/downloads/{id}/{action}", post(action))
        .route("/v1/downloads-all/{action}", post(action_all))
        .route("/v1/queues", get(queues))
        .route("/v1/queues/{id}/{action}", post(queue_action))
        .route("/v1/media/analyze", post(analyze))
        .route("/v1/media/download", post(media_download))
        .route("/v1/grab", post(grab))
        .route("/v1/ui/show", post(show))
        .layer(middleware::from_fn_with_state(ctx.clone(), guard))
        .layer(axum::extract::DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(ctx);
    write_api_file(&info)?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("local API stopped: {e}");
        }
    });
    Ok(info)
}

fn write_api_file(info: &ApiInfo) -> anyhow::Result<()> {
    let path = paths::api_file();
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(info)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Remove api.json on exit if it still belongs to this process.
pub fn remove_api_file() {
    if let Some(info) = ku_proto::client::read_api_info() {
        if info.pid == std::process::id() {
            let _ = std::fs::remove_file(paths::api_file());
        }
    }
}

async fn stats(State(c): State<Ctx>) -> Json<ku_proto::Stats> {
    Json(c.core.stats())
}

async fn browser_config(State(c): State<Ctx>) -> Json<ku_proto::BrowserConfig> {
    Json(c.core.settings().browser_config())
}

async fn list(State(c): State<Ctx>) -> Json<Vec<ku_proto::Download>> {
    Json(c.core.list())
}

async fn get_one(State(c): State<Ctx>, Path(id): Path<String>) -> R<ku_proto::Download> {
    c.core.get(&id).map(Json).ok_or(ApiErr(StatusCode::NOT_FOUND, "Download not found".into()))
}

async fn add(State(c): State<Ctx>, Json(req): Json<AddRequest>) -> R<Value> {
    if req.prompt {
        // Let the user confirm in the app (browser interception with confirmation on).
        c.core.emit(CoreEvent::Show);
        c.core.emit(CoreEvent::PromptAdd { request: Box::new(req) });
        return Ok(Json(json!({ "prompted": true })));
    }
    let d = c.core.add(req).await?;
    Ok(Json(json!({ "prompted": false, "download": d })))
}

#[derive(Deserialize)]
struct BatchReq {
    urls: Vec<String>,
    #[serde(default)]
    template: AddRequest,
}

async fn add_batch(State(c): State<Ctx>, Json(req): Json<BatchReq>) -> R<Value> {
    if req.urls.len() > 5000 {
        return Err(ApiErr(StatusCode::PAYLOAD_TOO_LARGE, "At most 5000 links per batch".into()));
    }
    let (ok, failed) = c.core.add_batch(req.urls, req.template).await;
    Ok(Json(json!({
        "added": ok,
        "failed": failed.into_iter().map(|(u, e)| json!({"url": u, "error": e})).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize, Default)]
struct RemoveQuery {
    #[serde(default, rename = "deleteFiles")]
    delete_files: bool,
}

async fn remove(State(c): State<Ctx>, Path(id): Path<String>, Query(q): Query<RemoveQuery>) -> R<Value> {
    c.core.remove(&[id], q.delete_files).await?;
    Ok(Json(json!({"ok": true})))
}

async fn action(State(c): State<Ctx>, Path((id, action)): Path<(String, String)>) -> R<Value> {
    let ids = vec![id];
    match action.as_str() {
        "pause" => c.core.pause(&ids).await?,
        "resume" | "retry" => c.core.resume(&ids).await?,
        _ => return Err(ApiErr(StatusCode::NOT_FOUND, "Unknown action".into())),
    }
    Ok(Json(json!({"ok": true})))
}

async fn action_all(State(c): State<Ctx>, Path(action): Path<String>) -> R<Value> {
    match action.as_str() {
        "pause" => c.core.pause_all().await?,
        "resume" => c.core.resume_all().await?,
        _ => return Err(ApiErr(StatusCode::NOT_FOUND, "Unknown action".into())),
    }
    Ok(Json(json!({"ok": true})))
}

async fn queues(State(c): State<Ctx>) -> Json<Vec<crate::types::Queue>> {
    Json(c.core.queues())
}

async fn queue_action(State(c): State<Ctx>, Path((id, action)): Path<(String, String)>) -> R<Value> {
    match action.as_str() {
        "start" => c.core.start_queue(&id, None).await?,
        "stop" => c.core.stop_queue(&id).await?,
        _ => return Err(ApiErr(StatusCode::NOT_FOUND, "Unknown action".into())),
    }
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzeReq {
    url: String,
    #[serde(default)]
    playlist: bool,
    #[serde(default)]
    cookies: Vec<ku_proto::BrowserCookie>,
    #[serde(default)]
    referer: Option<String>,
}

async fn analyze(State(c): State<Ctx>, Json(r): Json<AnalyzeReq>) -> R<ku_proto::MediaInfo> {
    Ok(Json(c.core.analyze(&r.url, r.playlist, r.cookies, r.referer).await?))
}

async fn media_download(State(c): State<Ctx>, Json(r): Json<MediaRequest>) -> R<ku_proto::Download> {
    Ok(Json(c.core.add_media(r).await?))
}

async fn grab(State(c): State<Ctx>, Json(r): Json<GrabRequest>) -> R<Value> {
    if r.links.len() > 20_000 {
        return Err(ApiErr(StatusCode::PAYLOAD_TOO_LARGE, "Too many links".into()));
    }
    c.core.emit(CoreEvent::Show);
    c.core.emit(CoreEvent::Grab { request: Box::new(r) });
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct ShowReq {
    media_url: Option<String>,
    cookies: Vec<ku_proto::BrowserCookie>,
}

async fn show(State(c): State<Ctx>, body: Option<Json<ShowReq>>) -> Json<Value> {
    c.core.emit(CoreEvent::Show);
    if let Some(Json(r)) = body {
        if let Some(url) = r.media_url {
            c.core.emit(CoreEvent::PromptMedia {
                request: Box::new(MediaRequest { url, cookies: r.cookies, ..Default::default() }),
            });
        }
    }
    Json(json!({"ok": true}))
}
