use std::path::PathBuf;

use crate::cli::Cli;

/// Runtime configuration for the gate. All values are resolved after CLI parsing
/// and secret loading.
#[derive(Debug, Clone)]
pub struct Config {
    pub listen: String,
    pub ttyd_socket: PathBuf,
    pub ttyd_bin: PathBuf,
    pub ttyd_uid: u32,
    pub secret_file: PathBuf,
    pub rate: String,
}

impl Config {
    pub fn new(cli: &Cli) -> Self {
        Self {
            listen: cli.listen.clone(),
            ttyd_socket: cli.ttyd_socket.clone(),
            ttyd_bin: cli.ttyd_bin.clone(),
            ttyd_uid: cli
                .ttyd_uid
                .expect("--ttyd-uid is required (or set TTG_TTYD_UID)"),
            secret_file: cli.secret_file.clone(),
            rate: cli.rate.clone(),
        }
    }
}
