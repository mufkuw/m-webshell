use std::path::Path;

use crate::totp::TotpVerifier;

fn pass(msg: &str) {
    println!("  \u{2713} {}", msg);
}

fn fail(msg: &str) {
    println!("  \u{2717} {}", msg);
}

fn warn(msg: &str) {
    println!("  \u{26A0} {}", msg);
}

pub fn run_check(secret_file: &Path, ttyd_bin: &Path, ttyd_socket: &Path, listen: &str) -> bool {
    let mut ok = true;

    println!();
    println!("  m-webshell pre-flight check");
    println!();

    // 1. Secret file
    if !secret_file.exists() {
        fail(&format!("secret file not found: {}", secret_file.display()));
        println!("    Run: m-webshell generate-secret");
        ok = false;
    } else {
        pass(&format!("secret file exists: {}", secret_file.display()));

        // Check permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(secret_file) {
                let mode = meta.permissions().mode();
                if mode & 0o004 != 0 {
                    fail("secret file is world-readable");
                    println!("    Fix: sudo chmod 640 {}", secret_file.display());
                    ok = false;
                } else {
                    pass("secret file permissions are correct (not world-readable)");
                }
            }
        }

        // Check secret is valid base32
        match TotpVerifier::from_secret_file(secret_file) {
            Ok(verifier) => {
                pass("secret is valid base32");
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let code = verifier.totp_generate(now);
                println!(
                    "    Current TOTP code: {}  (expires in ~{}s)",
                    code,
                    30 - (now % 30)
                );
            }
            Err(e) => {
                fail(&format!("secret file error: {}", e));
                ok = false;
            }
        }
    }

    // 2. Backend binary
    if !ttyd_bin.exists() {
        fail(&format!("backend binary not found: {}", ttyd_bin.display()));
        println!("    Install: curl -fsSL https://github.com/tsl0922/ttyd/releases/latest/download/ttyd.$(uname -m) -o {} && chmod 755 {}", ttyd_bin.display(), ttyd_bin.display());
        ok = false;
    } else {
        pass(&format!("backend binary found: {}", ttyd_bin.display()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(ttyd_bin) {
                let mode = meta.permissions().mode();
                if mode & 0o111 == 0 {
                    fail("backend binary is not executable");
                    println!("    Fix: sudo chmod 755 {}", ttyd_bin.display());
                    ok = false;
                } else {
                    pass("backend binary is executable");
                }
            }
        }
    }

    // 3. Runtime directory
    let socket_dir = ttyd_socket.parent().unwrap_or(Path::new("/run/m-webshell"));
    if !socket_dir.exists() {
        fail(&format!(
            "runtime directory does not exist: {}",
            socket_dir.display()
        ));
        println!(
            "    Fix: sudo mkdir -p {} && sudo chmod 0750 {}",
            socket_dir.display(),
            socket_dir.display()
        );
        ok = false;
    } else {
        pass(&format!(
            "runtime directory exists: {}",
            socket_dir.display()
        ));
    }

    // 4. Port availability
    let addr = listen.to_string();
    if let Ok(addr) = addr.parse::<std::net::SocketAddr>() {
        match std::net::TcpListener::bind(addr) {
            Ok(_) => pass(&format!("port {} is available", addr.port())),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                warn(&format!(
                    "port {} is already in use (m-webshell may be running)",
                    addr.port()
                ));
            }
            Err(_) => {
                fail(&format!("cannot bind to {}", addr));
                ok = false;
            }
        }
    }

    // 5. Systemd service file
    let service_path = Path::new("/etc/systemd/system/m-webshell.service");
    if service_path.exists() {
        pass("systemd service file installed");
    } else {
        warn("systemd service file not found at /etc/systemd/system/m-webshell.service");
        println!("    The service file is installed by the .deb package or manually.");
    }

    println!();
    if ok {
        pass("all checks passed");
    } else {
        fail("some checks failed — fix the issues above before starting the daemon");
    }
    println!();

    ok
}
