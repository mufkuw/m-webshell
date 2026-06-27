use std::path::Path;
use std::process::Stdio;

use tokio::process::{Child, Command};
use tracing::{debug, error, info};

use crate::config::Config;

pub struct BackendProcess {
    child: Child,
}

impl BackendProcess {
    pub async fn spawn(config: &Config) -> Result<Self, std::io::Error> {
        let uid = config.ttyd_uid.to_string();

        if let Some(parent) = config.ttyd_socket.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let font_size = config.font_size.to_string();
        let title_fixed = format!("titleFixed={}", &config.title);
        let mut cmd = Command::new(&config.ttyd_bin);
        cmd.arg("-i")
            .arg(config.ttyd_socket.as_os_str())
            .arg("-W")
            .arg("-m")
            .arg("1")
            .arg("-u")
            .arg(&uid)
            .arg("-t")
            .arg(format!("fontSize={}", font_size))
            .arg("-t")
            .arg(&title_fixed)
            .arg("-t")
            .arg("enableZmodem=true")
            .arg("-t")
            .arg("enableTrzsz=true")
            .arg("/bin/login")
            .kill_on_drop(true);

        info!(
            bin = %config.ttyd_bin.display(),
            socket = %config.ttyd_socket.display(),
            uid = %uid,
            "spawning backend on unix socket"
        );

        let child = cmd.spawn()?;
        Ok(Self { child })
    }

    pub async fn wait(mut self) -> Option<i32> {
        match self.child.wait().await {
            Ok(status) => {
                info!(status = ?status, "backend process exited");
                status.code()
            }
            Err(e) => {
                error!(error = %e, "failed to wait for backend");
                None
            }
        }
    }

    pub async fn shutdown(&mut self) {
        if let Some(id) = self.child.id() {
            debug!(pid = id, "sending SIGTERM to backend");
            let _ = self.child.start_kill();
        }
    }
}

pub async fn spawn_mock_backend(socket: &Path) -> Result<Child, std::io::Error> {
    if let Some(parent) = socket.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    Command::new("python3")
        .arg("-c")
        .arg(include_str!("../tests/mock_ttyd.py"))
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
}
