use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use http::Uri;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioIo;
use regex::Regex;
use tokio::net::UnixStream;
use tower::Service;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::ratelimit::RateLimiter;
use crate::totp::TotpVerifier;

#[derive(Clone)]
pub struct UnixConnector {
    socket_path: PathBuf,
}

impl UnixConnector {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }
}

impl Service<Uri> for UnixConnector {
    type Response = TokioIo<UnixStream>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let path = self.socket_path.clone();
        Box::pin(async move {
            let stream = UnixStream::connect(&path).await?;
            Ok(TokioIo::new(stream))
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub verifier: TotpVerifier,
    pub rate_limiter: Arc<RateLimiter>,
    pub client: Client<UnixConnector, Full<Bytes>>,
}

fn path_regex() -> Regex {
    Regex::new(r"^/system/manage-([0-9]{6})(/.*)?$").expect("valid regex")
}

pub fn normalize_path(path: &str) -> String {
    let re = Regex::new(r"^/system/manage-[0-9]{6}").unwrap();
    re.replace(path, "/system/manage-******").into_owned()
}

pub async fn gate_handler(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response {
    let peer_ip = addr.ip();
    let original_path = req.uri().path().to_string();
    let normalized = normalize_path(&original_path);

    let method = req.method().clone();
    if method != axum::http::Method::GET
        && method != axum::http::Method::HEAD
        && method != axum::http::Method::OPTIONS
    {
        warn!(%peer_ip, %method, path = %normalized, "rejected method");
        return not_found();
    }

    if state.rate_limiter.check_key(&peer_ip).is_err() {
        warn!(%peer_ip, path = %normalized, "rate limit exceeded");
        return not_found();
    }

    let re = path_regex();
    let caps = match re.captures(&original_path) {
        Some(c) => c,
        None => {
            warn!(%peer_ip, path = %normalized, "path does not match /manage-XXXXXX prefix");
            return not_found();
        }
    };

    let code = caps.get(1).unwrap().as_str();

    if !state.verifier.check(code) {
        warn!(%peer_ip, path = %normalized, "invalid TOTP");
        return not_found();
    }

    let tail = caps.get(2).map(|m| m.as_str()).unwrap_or("");
    let rewritten_path = if tail.is_empty() { "/" } else { tail };

    info!(%peer_ip, path = %normalized, "auth succeeded");

    let is_ws_upgrade = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws_upgrade {
        info!(%peer_ip, path = %normalized, "websocket upgrade");
        return crate::ws::bridge_ws_upgrade(
            req,
            state.config.ttyd_socket.clone(),
            rewritten_path.to_string(),
        )
        .await;
    }

    let (parts, body) = req.into_parts();

    match proxy_to_backend(state, parts, body, rewritten_path).await {
        Ok(resp) => resp,
        Err(e) => {
            error!(error = %e, "proxy error");
            (StatusCode::SERVICE_UNAVAILABLE,).into_response()
        }
    }
}

/// Rebuild the URI, then send the request to the backend over its Unix socket.
async fn proxy_to_backend(
    state: AppState,
    parts: http::request::Parts,
    body: Body,
    new_path: &str,
) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
    let query = parts
        .uri
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let uri = format!("http://localhost{}{}", new_path, query).parse::<Uri>()?;

    let mut buf = Vec::new();
    let mut stream = body.into_data_stream();
    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| std::io::Error::other(e.to_string()))?;
        buf.extend_from_slice(&chunk);
    }
    let hyper_body = http_body_util::Full::new(Bytes::from(buf));

    let mut builder = http::Request::builder()
        .method(parts.method)
        .uri(&uri)
        .version(parts.version);

    for (name, value) in &parts.headers {
        if name == "host" {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder = builder.header("Host", "m-webshell");

    let req = builder.body(hyper_body)?;

    let resp = state.client.request(req).await?;
    Ok(transform_response(resp))
}

pub fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

/// Convert the upstream hyper response into an axum response,
/// injecting security headers.
fn transform_response<B>(resp: hyper::Response<B>) -> Response
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let (parts, body) = resp.into_parts();
    let status = parts.status;
    let headers = parts.headers;
    let stream = http_body_util::BodyStream::new(body).map(|frame| {
        let frame = frame.map_err(|e| std::io::Error::other(e.to_string()))?;
        let data = frame.into_data().unwrap_or_default();
        Ok::<_, std::io::Error>(data)
    });
    let axum_body = axum::body::Body::from_stream(stream);
    let mut response = (status, axum_body).into_response();
    *response.headers_mut() = headers;
    let hdrs = response.headers_mut();
    hdrs.insert("X-Frame-Options", HeaderValue::from_static("DENY"));
    hdrs.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );
    hdrs.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );
    hdrs.insert("Referrer-Policy", HeaderValue::from_static("no-referrer"));
    response
}
