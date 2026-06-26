use std::sync::Arc;

use axum::routing::any;
use axum::Router;
use clap::Parser;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tokio::signal;
use tracing::{error, info};

use m_webshell::cli::{Cli, Command};
use m_webshell::config::Config;
use m_webshell::gate::{AppState, UnixConnector};
use m_webshell::totp::TotpVerifier;
use m_webshell::{gate, ratelimit, show_secret, backend};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command_or_default() {
        Command::Serve => serve(cli).await,
        Command::ShowSecret => {
            show_secret::run_show_secret(&cli.secret_file, cli.totp_window)?;
            Ok(())
        }
        Command::GenerateSecret => {
            show_secret::run_generate_secret(&cli.secret_file, cli.totp_window)?;
            Ok(())
        }
    }
}

async fn serve(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::new(&cli);

    info!(listen = %config.listen, "starting m-webshell");

    let verifier = TotpVerifier::from_secret_file(&config.secret_file, config.totp_window)?;
    let rate_limiter = Arc::new(ratelimit::RateLimiter::new(&config.rate));

    let connector = UnixConnector::new(config.ttyd_socket.clone());
    let client = Client::builder(TokioExecutor::new()).build(connector);

    let state = AppState {
        config: config.clone(),
        verifier,
        rate_limiter,
        client,
    };

    // Spawn backend early, but do not fail hard if the binary is unavailable:
    // integration tests can bring up their own upstream.
    let mut backend = match backend::BackendProcess::spawn(&config).await {
        Ok(t) => Some(t),
        Err(e) => {
            error!(error = %e, "failed to spawn backend; continuing without managed child");
            None
        }
    };

    let app = Router::new()
        .route("/", any(gate::gate_handler))
        .route("/*path", any(gate::gate_handler))
        .with_state(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();

    let listener = tokio::net::TcpListener::bind(&config.listen).await?;
    info!("listening on {}", config.listen);

    let serve_task = axum::serve(listener, app);

    tokio::select! {
        result = serve_task => {
            if let Err(e) = result {
                error!(error = %e, "server error");
            }
        }
        _ = signal::ctrl_c() => {
            info!("received shutdown signal");
            if let Some(ref mut b) = &mut backend {
                b.shutdown().await;
            }
        }
    }

    if let Some(b) = backend {
        b.wait().await;
    }

    Ok(())
}
