//! Local HTTP server: the viewer page, a live event stream per session, and image paste.

use std::convert::Infallible;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, Query, Request};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

use crate::{hook, transcript};

const APP_HTML: &str = include_str!("../web/app.html");
const ICON_SVG: &str = include_str!("../web/icon.svg");
const POLL: Duration = Duration::from_millis(300);
const MAX_PASTE: usize = 32 * 1024 * 1024;

pub async fn serve(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/", get(page))
        .route("/s/:id", get(page))
        .route("/icon.svg", get(icon))
        .route("/api/sessions", get(sessions))
        .route("/api/s/:id/events", get(events))
        .route("/api/s/:id/paste", post(paste).layer(DefaultBodyLimit::max(MAX_PASTE)))
        .route("/api/file", get(file))
        .layer(middleware::from_fn(guard));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Only answer requests addressed to localhost (blocks DNS rebinding), and only accept
/// writes from our own pages.
async fn guard(headers: HeaderMap, req: Request, next: Next) -> Response {
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("");
    let host_name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if !matches!(host_name, "127.0.0.1" | "localhost") {
        return StatusCode::FORBIDDEN.into_response();
    }
    if req.method() != Method::GET {
        let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok());
        if origin != Some(&format!("http://{host}")) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    next.run(req).await
}

async fn icon() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "max-age=86400")], ICON_SVG)
}

async fn page() -> Html<&'static str> {
    Html(APP_HTML)
}

async fn sessions() -> Json<Vec<transcript::SessionInfo>> {
    Json(tokio::task::spawn_blocking(|| transcript::list_sessions(100)).await.unwrap_or_default())
}

async fn events(
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    let info = tokio::task::spawn_blocking(move || transcript::find_session(&id))
        .await
        .ok()
        .flatten()
        .ok_or(StatusCode::NOT_FOUND)?;
    let (tx, rx) = mpsc::channel(512);
    let meta = json!({ "id": info.id, "title": info.title, "project": info.project });
    let _ = tx.send(Event::default().event("meta").data(meta.to_string())).await;
    tokio::spawn(tail(info.path, tx));
    Ok(Sse::new(ReceiverStream::new(rx).map(Ok)).keep_alive(KeepAlive::default()))
}

/// Streams every message already in the transcript, then follows it as Claude Code appends.
/// Ends when the client disconnects.
async fn tail(path: PathBuf, tx: mpsc::Sender<Event>) {
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
        }
        if tx.is_closed() {
            return;
        }
        tokio::time::sleep(POLL).await;
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
