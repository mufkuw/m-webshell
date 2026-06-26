use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use totp_rs::{Algorithm, Secret, TOTP};
use tracing::error;

#[derive(Debug, Error)]
pub enum TotpError {
    #[error("failed to read secret file: {0}")]
    ReadSecret(#[from] std::io::Error),
    #[error("invalid base32 secret")]
    InvalidSecret,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct TotpVerifier {
    totp: TOTP,
    window: u64,
}

impl TotpVerifier {
    pub fn from_secret_file(path: &Path, window: u8) -> Result<Self, TotpError> {
        let raw = std::fs::read_to_string(path)?;
        let secret_string = raw.trim().replace(' ', "").replace('\n', "");
        let secret = Secret::Encoded(secret_string.clone()).to_bytes().map_err(|_| {
            error!(path = %path.display(), "secret file contains invalid base32");
            TotpError::InvalidSecret
        })?;

        // totp-rs defaults SHA1 block size for RFC6238 test vectors.
        let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret, None, "ttyd".to_string())
            .map_err(|_| TotpError::InvalidSecret)?;

        // Sanity: ensure our generate matches the known RFC test vector at t=59.
        #[cfg(debug_assertions)]
        if totp.generate(59) != "287082" {
            error!(secret = %secret_string, generated = %totp.generate(59), "TOTP generator does not match RFC test vector");
        }

        Ok(Self {
            totp,
            window: window as u64,
        })
    }

    pub fn check(&self, code: &str) -> bool {
        let Ok(current) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        // totp-rs `check` only validates the exact current step. Use generate to
        // support a window around the current time.
        for step in -(self.window as i64)..=self.window as i64 {
            let ts = (current.as_secs() as i64 + step * 30).max(0) as u64;
            let generated = self.totp.generate(ts);
            tracing::trace!(step = %step, generated = %generated, "TOTP generate");
            if generated == code {
                return true;
            }
        }
        false
    }

    /// Validate against a specific Unix timestamp (for tests).
    #[cfg(test)]
    pub fn check_at(&self, code: &str, timestamp: u64) -> bool {
        self.totp.check(code, timestamp)
    }

    pub fn provisioning_uri(&self, _account: &str, _issuer: &str) -> String {
        self.totp.get_url()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn known_totp_value() {
        // Test vector from RFC 6238 secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ" (="12345678901234567890")
        let secret = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(secret.as_bytes()).unwrap();
        let verifier = TotpVerifier::from_secret_file(tmp.path(), 1).unwrap();

        // At 59 seconds the RFC TOTP value is 287082.
        assert!(verifier.check_at("287082", 59));
        assert!(!verifier.check_at("000000", 59));
    }
}
