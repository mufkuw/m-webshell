use std::path::PathBuf;

use axum::body::Body;
use axum::extract::ws::{CloseFrame as AxumCloseFrame, Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::FromRequestParts;
use axum::http::{HeaderValue, Request};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio_tungstenite::{client_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::CloseFrame as TungCloseFrame, tungstenite::Message as TungMessage};
use tracing::{debug, error, info, warn};

use crate::gate::not_found;

pub async fn bridge_ws_upgrade(req: Request<Body>, backend_socket: PathBuf, rewritten_path: String) -> Response {
    let (mut parts, _body) = req.into_parts();

    let protocols: Vec<String> = parts.headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|p| p.trim().to_string()).collect())
        .unwrap_or_default();

    let mut ws = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws,
        Err(_) => {
            warn!("failed to parse WebSocket upgrade request");
            return not_found();
        }
    };

    if !protocols.is_empty() {
        ws = ws.protocols(protocols.into_iter().map(std::borrow::Cow::Owned));
    }

    info!(path = %rewritten_path, "accepting WebSocket upgrade");

    ws.on_upgrade(move |client_ws| async move {
        bridge(client_ws, backend_socket, rewritten_path).await;
    })
}

async fn bridge(client: WebSocket, backend_socket: PathBuf, upstream_path: String) {
    let stream = match UnixStream::connect(&backend_socket).await {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, socket = %backend_socket.display(), "failed to connect to backend unix socket");
            let _ = client.close().await;
            return;
        }
    };

    let ws_url = format!("ws://localhost{}", upstream_path);
    let mut req = ws_url.into_client_request().unwrap();
    req.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static("tty"));
    let (upstream, _) = match client_async(req, stream).await {
        Ok(pair) => pair,
        Err(e) => {
            error!(error = %e, "WebSocket handshake with backend failed");
            let _ = client.close().await;
            return;
        }
    };

    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();

    let client_to_upstream = async {
        while let Some(Ok(msg)) = client_rx.next().await {
            let tung = axum_to_tungstenite(msg);
            if let Err(e) = upstream_tx.send(tung).await {
                debug!(error = %e, "error forwarding to upstream");
                break;
            }
        }
    };

    let upstream_to_client = async {
        while let Some(Ok(msg)) = upstream_rx.next().await {
            let axum = tungstenite_to_axum(msg);
            if let Err(e) = client_tx.send(axum).await {
                debug!(error = %e, "error forwarding to client");
                break;
            }
        }
    };

    tokio::select! {
        _ = client_to_upstream => {}
        _ = upstream_to_client => {}
    }

    let _ = client_tx.close().await;
    let _ = upstream_tx.close().await;
}

fn axum_to_tungstenite(msg: AxumMessage) -> TungMessage {
    match msg {
        AxumMessage::Text(t) => TungMessage::Text(t),
        AxumMessage::Binary(b) => TungMessage::Binary(b),
        AxumMessage::Ping(p) => TungMessage::Ping(p),
        AxumMessage::Pong(p) => TungMessage::Pong(p),
        AxumMessage::Close(c) => TungMessage::Close(c.map(|f| TungCloseFrame {
            code: f.code.into(),
            reason: f.reason,
        })),
    }
}

fn tungstenite_to_axum(msg: TungMessage) -> AxumMessage {
    match msg {
        TungMessage::Text(t) => AxumMessage::Text(t),
        TungMessage::Binary(b) => AxumMessage::Binary(b),
        TungMessage::Ping(p) => AxumMessage::Ping(p),
        TungMessage::Pong(p) => AxumMessage::Pong(p),
        TungMessage::Close(c) => AxumMessage::Close(c.map(|f| AxumCloseFrame {
            code: f.code.into(),
            reason: f.reason,
        })),
        TungMessage::Frame(_) => AxumMessage::Pong(vec![]),
    }
}
