use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Default values are duplicated here and in DESIGN.md. Keep them in sync.
#[derive(Debug, Clone, Parser)]
#[command(name = "m-webshell")]
#[command(about = "The keep for your web terminal")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Address to bind the gate HTTP server to
    #[arg(long, env = "TTG_LISTEN", default_value = "127.0.0.1:12479")]
    pub listen: String,

    /// Unix socket path that ttyd will listen on
    #[arg(long, env = "TTG_TTYD_SOCKET", default_value = "/run/m-webshell/ttyd.sock")]
    pub ttyd_socket: PathBuf,

    /// Path to the ttyd binary
    #[arg(long, env = "TTG_TTYD_BIN", default_value = "/usr/local/bin/ttyd")]
    pub ttyd_bin: PathBuf,

    /// UID to drop the spawned login shell to
    #[arg(long, env = "TTG_TTYD_UID")]
    pub ttyd_uid: Option<u32>,

    /// File containing the base32-encoded TOTP secret
    #[arg(long, env = "TTG_SECRET_FILE", default_value = "/etc/m-webshell/m-webshell.totp")]
    pub secret_file: PathBuf,

    /// Per-IP rate limit, e.g. 30/minute
    #[arg(long, env = "TTG_RATE", default_value = "30/minute")]
    pub rate: String,

    /// Allowed 30s steps of TOTP skew
    #[arg(long, env = "TTG_TOTP_WINDOW", default_value_t = 0)]
    pub totp_window: u8,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Run the gate daemon (default)
    Serve,
    /// Print the provisioning URI and QR code for the secret
    ShowSecret,
}

impl Cli {
    /// Merge `serve` as the default subcommand.
    pub fn command_or_default(&self) -> Command {
        self.command.clone().unwrap_or(Command::Serve)
    }
}
