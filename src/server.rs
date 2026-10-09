//! Local HTTP server: the viewer page, a live event stream per session, image paste, and the
//! bridge that lets the viewer answer questions, plan approvals and permission prompts.

use std::collections::HashMap;
use std::convert::Infallible;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

use crate::{hook, transcript};

const APP_HTML: &str = include_str!("../web/app.html");
const ICON_SVG: &str = include_str!("../web/icon.svg");
const POLL: Duration = Duration::from_millis(300);
const MAX_PASTE: usize = 32 * 1024 * 1024;
/// How long a hook waits for an answer from the viewer. Below the hooks' 600 s timeout, so
/// the hook falls through to the terminal on its own terms rather than being killed.
const MAX_WAIT: Duration = Duration::from_secs(540);
/// Header the `megane hook` commands send. Browsers cannot attach it cross-site without a
/// CORS preflight, which this server never approves.
pub const HOOK_HEADER: &str = "x-megane-hook";

/// Viewers and pending questions, per session.
#[derive(Default)]
struct Hub {
    sessions: Mutex<HashMap<String, Session>>,
    next_id: AtomicU64,
}

#[derive(Default)]
struct Session {
    /// Connected viewers (one per event stream) and whether each wants to answer right now:
    /// its tab is visible and "answer in the browser" is on.
    viewers: HashMap<u64, bool>,
    pending: HashMap<u64, Pending>,
    events: Option<broadcast::Sender<(&'static str, String)>>,
}

struct Pending {
    payload: Value,
    reply: Option<oneshot::Sender<Value>>,
}

impl Hub {
    fn id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn with<R>(&self, session: &str, f: impl FnOnce(&mut Session) -> R) -> R {
        let mut sessions = self.sessions.lock().unwrap();
        f(sessions.entry(session.to_string()).or_default())
    }

    fn subscribe(&self, session: &str) -> broadcast::Receiver<(&'static str, String)> {
        self.with(session, |s| s.events.get_or_insert_with(|| broadcast::channel(64).0).subscribe())
    }

    fn broadcast(&self, session: &str, event: &'static str, data: String) {
        self.with(session, |s| {
            if let Some(tx) = &s.events {
                let _ = tx.send((event, data));
            }
        });
    }

    fn answering(&self, session: &str) -> bool {
        self.with(session, |s| s.viewers.values().any(|&active| active))
    }
}

/// Removes a pending question when its hook request ends, however it ends (answered, fell
/// through, or the hook process went away), and tells the viewers.
struct PendingGuard {
    hub: Arc<Hub>,
    session: String,
    id: u64,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        self.hub.with(&self.session, |s| s.pending.remove(&self.id));
        self.hub.broadcast(&self.session, "resolved", json!({ "id": self.id }).to_string());
    }
}

/// Drops a viewer from the hub when its event stream ends.
struct ViewerGuard {
    hub: Arc<Hub>,
    session: String,
    id: u64,
}

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.hub.with(&self.session, |s| s.viewers.remove(&self.id));
    }
}

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(page))
        .route("/s/:id", get(page))
        .route("/icon.svg", get(icon))
        .route("/api/sessions", get(sessions))
        .route("/api/s/:id/events", get(events))
        .route("/api/s/:id/paste", post(paste).layer(DefaultBodyLimit::max(MAX_PASTE)))
        .route("/api/s/:id/presence", post(presence))
        .route("/api/s/:id/answer", post(answer))
        .route("/api/hook/:id", post(hook_wait))
        .route("/api/file", get(file))
        .layer(middleware::from_fn(guard))
        .with_state(Arc::new(Hub::default()));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Only answer requests addressed to localhost (blocks DNS rebinding). Writes must come from
/// our own pages (same Origin) or from the `megane hook` commands (custom header).
async fn guard(headers: HeaderMap, req: Request, next: Next) -> Response {
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let host_name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if !matches!(host_name, "127.0.0.1" | "localhost") {
        return StatusCode::FORBIDDEN.into_response();
    }
    if req.method() != Method::GET {
        let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok());
        let same_origin = origin == Some(&format!("http://{host}"));
        let from_hook = origin.is_none() && headers.contains_key(HOOK_HEADER);
        if !same_origin && !from_hook {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(req).await
}

async fn page() -> Html<&'static str> {
    Html(APP_HTML)
}

async fn icon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "max-age=86400")], ICON_SVG)
}

async fn sessions() -> Json<Vec<transcript::SessionInfo>> {
    Json(tokio::task::spawn_blocking(|| transcript::list_sessions(100)).await.unwrap_or_default())
}

async fn events(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let info = tokio::task::spawn_blocking(move || transcript::find_session(&id))
        .await
        .ok()
        .flatten()
        .ok_or(StatusCode::NOT_FOUND)?;
    let viewer = hub.id();
    hub.with(&info.id, |s| s.viewers.insert(viewer, false));
    let guard = ViewerGuard { hub: hub.clone(), session: info.id.clone(), id: viewer };

    let (tx, rx) = mpsc::channel(512);
    let meta = json!({ "id": info.id, "title": info.title, "project": info.project, "viewer": viewer });
    let _ = tx.send(Event::default().event("meta").data(meta.to_string())).await;
    tokio::spawn(tail(info.path, tx, guard));
    Ok(Sse::new(ReceiverStream::new(rx).map(Ok)).keep_alive(KeepAlive::default()))
}

/// Streams every message already in the transcript, then follows it as Claude Code appends,
/// together with questions waiting for an answer. Ends when the client disconnects.
async fn tail(path: PathBuf, tx: mpsc::Sender<Event>, viewer: ViewerGuard) {
    let mut questions = viewer.hub.subscribe(&viewer.session);
    let mut offset = 0u64;
    let mut partial = Vec::new();
    let mut caught_up = false;
    loop {
        let len = tokio::fs::metadata(&path).await.map_or(0, |m| m.len());
        if len < offset {
            // Rewritten from scratch: start over.
            offset = 0;
            partial.clear();
            if tx.send(Event::default().event("reset").data("")).await.is_err() {
                return;
            }
        }
        if len > offset {
            if let Ok(mut f) = tokio::fs::File::open(&path).await {
                let mut buf = Vec::with_capacity((len - offset) as usize);
                if f.seek(SeekFrom::Start(offset)).await.is_ok() && f.read_to_end(&mut buf).await.is_ok() {
                    offset += buf.len() as u64;
                    partial.extend_from_slice(&buf);
                    let complete = partial.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
                    for line in partial[..complete].split(|&b| b == b'\n') {
                        let Some(msg) = transcript::normalize(&String::from_utf8_lossy(line)) else {
                            continue;
                        };
                        if tx.send(Event::default().event("msg").data(msg.to_string())).await.is_err() {
                            return;
                        }
                    }
                    partial.drain(..complete);
                }
            }
        }
        if !caught_up {
            caught_up = true;
            if tx.send(Event::default().event("ready").data("")).await.is_err() {
                return;
            }
            let waiting: Vec<String> = viewer.hub.with(&viewer.session, |s| {
                s.pending.values().map(|p| p.payload.to_string()).collect()
            });
            for payload in waiting {
                if tx.send(Event::default().event("pending").data(payload)).await.is_err() {
                    return;
                }
            }
        }
        loop {
            match questions.try_recv() {
                Ok((event, data)) => {
                    if tx.send(Event::default().event(event).data(data)).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        if tx.is_closed() {
            return;
        }
        tokio::time::sleep(POLL).await;
    }
}

#[derive(Deserialize)]
struct PresenceBody {
    viewer: u64,
    active: bool,
}

async fn presence(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(body): Json<PresenceBody>) -> StatusCode {
    hub.with(&id, |s| {
        if let Some(active) = s.viewers.get_mut(&body.viewer) {
            *active = body.active;
            StatusCode::NO_CONTENT
        } else {
            StatusCode::NOT_FOUND
        }
    })
}

/// Called by `megane hook pretool|permission`. When a viewer is ready to answer, the request
/// waits until the viewer replies and returns the reply; otherwise (or when the viewer goes
/// away, or hands the question back to the terminal) it returns 204 and the hook falls through.
async fn hook_wait(State(hub): State<Arc<Hub>>, Path(session): Path<String>, Json(mut payload): Json<Value>) -> Response {
    if !hub.answering(&session) {
        return StatusCode::NO_CONTENT.into_response();
    }
    let id = hub.id();
    payload["id"] = json!(id);
    let (reply, mut answered) = oneshot::channel();
    hub.with(&session, |s| s.pending.insert(id, Pending { payload: payload.clone(), reply: Some(reply) }));
    let _guard = PendingGuard { hub: hub.clone(), session: session.clone(), id };
    hub.broadcast(&session, "pending", payload.to_string());

    let deadline = Instant::now() + MAX_WAIT;
    loop {
        tokio::select! {
            r = &mut answered => {
                return match r {
                    Ok(answer) if answer["action"] != "terminal" => Json(answer).into_response(),
                    _ => StatusCode::NO_CONTENT.into_response(),
                };
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                if !hub.answering(&session) || Instant::now() > deadline {
                    return StatusCode::NO_CONTENT.into_response();
                }
            }
        }
    }
}

#[derive(Deserialize)]
struct AnswerBody {
    id: u64,
    answer: Value,
}

async fn answer(State(hub): State<Arc<Hub>>, Path(session): Path<String>, Json(body): Json<AnswerBody>) -> StatusCode {
    let reply = hub.with(&session, |s| s.pending.get_mut(&body.id).and_then(|p| p.reply.take()));
    match reply.map(|tx| tx.send(body.answer)) {
        Some(Ok(())) => StatusCode::NO_CONTENT,
        _ => StatusCode::GONE,
    }
}

async fn paste(Path(id): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
    if transcript::find_session(&id).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let ext = match headers.get(header::CONTENT_TYPE).and_then(|h| h.to_str().ok()) {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        _ => return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response(),
    };
    let dir = hook::inbox_dir(&id);
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let path = dir.join(format!("paste-{ms}.{ext}"));
    let saved = async {
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(&path, &body).await
    };
    match saved.await {
        Ok(()) => Json(json!({ "path": path.to_string_lossy() })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

/// Serves local images that Claude's messages link to. Image types only.
async fn file(Query(q): Query<FileQuery>) -> Response {
    let path = PathBuf::from(&q.path);
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    let mime = match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => return StatusCode::FORBIDDEN.into_response(),
    };
    match tokio::fs::read(&path).await {
        // `sandbox` keeps an SVG opened directly from running script on our origin.
        Ok(bytes) => {
            ([(header::CONTENT_TYPE, mime), (header::CONTENT_SECURITY_POLICY, "sandbox")], bytes)
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
