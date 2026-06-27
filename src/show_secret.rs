use std::path::Path;

use qrcode::{types::Color, QrCode};

use crate::totp::TotpVerifier;

pub fn run_show_secret(secret_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let verifier = TotpVerifier::from_secret_file(secret_file)?;
    let uri = verifier.provisioning_uri("server", "m-webshell");
    print_qr(&uri)?;
    Ok(())
}

pub fn run_generate_secret(secret_file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = secret_file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let secret = generate_base32_secret();
    std::fs::write(secret_file, &secret)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(secret_file, std::fs::Permissions::from_mode(0o640))?;
    }

    let verifier = TotpVerifier::from_secret_file(secret_file)?;
    let uri = verifier.provisioning_uri("server", "m-webshell");

    println!();
    println!("  New secret generated.");
    println!("  Restart the daemon:  systemctl restart m-webshell");
    println!();
    print_qr(&uri)?;
    Ok(())
}

fn print_qr(uri: &str) -> Result<(), Box<dyn std::error::Error>> {
    let code = QrCode::new(uri)?;
    let width = code.width();
    let quiet = 2;

    let total = width + quiet * 2;
    let border = "\u{2550}".repeat(total + 2);

    println!();
    println!("  \u{2554}{}\u{2557}", border);
    print!("  \u{2551} ");
    for _ in 0..total {
        print!(" ");
    }
    println!(" \u{2551}");

    for row in 0..width {
        print!("  \u{2551} ");
        for _ in 0..quiet {
            print!(" ");
        }
        for col in 0..width {
            let pixel = code[(row, col)];
            if pixel == Color::Dark {
                print!("\u{2588}");
            } else {
                print!(" ");
            }
        }
        for _ in 0..quiet {
            print!(" ");
        }
        println!(" \u{2551}");
    }

    print!("  \u{2551} ");
    for _ in 0..total {
        print!(" ");
    }
    println!(" \u{2551}");
    println!("  \u{255A}{}\u{255D}", border);
    println!();
    println!("  Scan with your authenticator app.");
    println!();
    Ok(())
}

fn generate_base32_secret() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut bytes = [0u8; 20];
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    for (i, b) in bytes.iter_mut().enumerate() {
        *b = ((seed >> (i % 8)) & 0xFF) as u8 ^ (i as u8).wrapping_mul(37);
    }

    let mut seed_state = seed as u64;
    for b in bytes.iter_mut() {
        seed_state = seed_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *b = (seed_state >> 33) as u8;
    }

    base32_encode(&bytes)
}

const BASE32_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn base32_encode(data: &[u8]) -> String {
    let mut result = String::new();
    let mut buffer: u64 = 0;
    let mut bits_left = 0;

    for &byte in data {
        buffer = (buffer << 8) | byte as u64;
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let index = ((buffer >> bits_left) & 0x1F) as usize;
            result.push(BASE32_ALPHABET[index] as char);
        }
    }

    if bits_left > 0 {
        let index = ((buffer << (5 - bits_left)) & 0x1F) as usize;
        result.push(BASE32_ALPHABET[index] as char);
    }

    result
}
