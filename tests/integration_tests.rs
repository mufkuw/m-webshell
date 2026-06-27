use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::Request;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tempfile::NamedTempFile;

use m_webshell::backend::spawn_mock_backend;
use m_webshell::config::Config;
use m_webshell::gate::{normalize_path, AppState, UnixConnector};
use m_webshell::ratelimit::RateLimiter;
use m_webshell::totp::TotpVerifier;

#[tokio::test]
async fn http_proxy_rewrites_path() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("ttyd.sock");
    let _mock = spawn_mock_backend(&socket).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let client: Client<UnixConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(UnixConnector::new(socket.clone()));

    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(secret.as_bytes()).unwrap();

    let verifier = TotpVerifier::from_secret_file(file.path()).unwrap();
    let state = AppState {
        config: Config {
            listen: "127.0.0.1:0".to_string(),
            ttyd_socket: socket.clone(),
            ttyd_bin: PathBuf::from("/usr/local/bin/ttyd"),
            ttyd_uid: 0,
            secret_file: file.path().to_path_buf(),
            rate: "30/minute".to_string(),
            title: "Terminal".to_string(),
            font_size: 16,
        },
        verifier,
        rate_limiter: Arc::new(RateLimiter::new("1000/minute")),
        client,
    };

    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let code = current_code(secret).await;

    let req = Request::builder()
        .uri(format!("/system/manage-{}/test?token=abc", code))
        .header("X-Test", "yes")
        .body(Body::empty())
        .unwrap();

    let resp = m_webshell::gate::gate_handler(
        axum::extract::ConnectInfo(addr),
        axum::extract::State(state),
        req,
    )
    .await;

    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn missing_prefix_returns_404() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("ttyd.sock");
    let _mock = spawn_mock_backend(&socket).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let client: Client<UnixConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build(UnixConnector::new(socket.clone()));

    let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(secret.as_bytes()).unwrap();
    let verifier = TotpVerifier::from_secret_file(file.path()).unwrap();
    let state = AppState {
        config: Config {
            listen: "127.0.0.1:0".to_string(),
            ttyd_socket: socket.clone(),
            ttyd_bin: PathBuf::from("/usr/local/bin/ttyd"),
            ttyd_uid: 0,
            secret_file: file.path().to_path_buf(),
            rate: "1000/minute".to_string(),
            title: "Terminal".to_string(),
            font_size: 16,
        },
        verifier,
        rate_limiter: Arc::new(RateLimiter::new("1000/minute")),
        client,
    };

    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let req = Request::builder()
        .uri("/other-path")
        .body(Body::empty())
        .unwrap();
    let resp = m_webshell::gate::gate_handler(
        axum::extract::ConnectInfo(addr),
        axum::extract::State(state),
        req,
    )
    .await;
    assert_eq!(resp.status(), 404);
}

#[test]
fn normalize_path_masks_totp() {
    assert_eq!(
        normalize_path("/system/manage-123456/token?x=1"),
        "/system/manage-******/token?x=1"
    );
    assert_eq!(
        normalize_path("/system/manage-123456"),
        "/system/manage-******"
    );
}

async fn current_code(secret: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        totp_rs::Secret::Encoded(secret.to_string())
            .to_bytes()
            .unwrap(),
        None,
        "m-webshell".to_string(),
    )
    .unwrap();
    totp.generate(now)
}
