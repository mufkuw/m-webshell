use std::path::Path;

use crate::totp::TotpVerifier;

pub fn run_show_secret(secret_file: &Path, totp_window: u8) -> Result<(), Box<dyn std::error::Error>> {
    let verifier = TotpVerifier::from_secret_file(secret_file, totp_window)?;
    let uri = verifier.provisioning_uri("server", "ttyd");
    println!("{}", uri);
    qr2term::print_qr(uri.clone())?;
    Ok(())
}
