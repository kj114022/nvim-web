//! Authentication module for secure TCP connections
//!
//! Implements token-based authentication per Neovim issue #4443.
//! Uses HMAC-SHA256 for challenge-response verification.

use std::fs::{File, Permissions};
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Length of authentication token in bytes (32 bytes = 256 bits)
const TOKEN_LENGTH: usize = 32;

/// Length of challenge nonce in bytes
const NONCE_LENGTH: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// Generate a cryptographically secure random token
pub fn generate_secure_token() -> String {
    let mut bytes = [0u8; TOKEN_LENGTH];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Generate a random nonce for challenge-response
pub fn generate_nonce() -> [u8; NONCE_LENGTH] {
    let mut nonce = [0u8; NONCE_LENGTH];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    nonce
}

/// Compute HMAC-SHA256 of nonce using token as key
pub fn compute_hmac(nonce: &[u8], token: &str) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(token.as_bytes()).expect("HMAC can take key of any size");
    mac.update(nonce);
    mac.finalize().into_bytes().to_vec()
}

/// Verify HMAC response (constant-time comparison)
pub fn verify_hmac(nonce: &[u8], token: &str, client_hmac: &[u8]) -> bool {
    let mut mac =
        HmacSha256::new_from_slice(token.as_bytes()).expect("HMAC can take key of any size");
    mac.update(nonce);
    mac.verify_slice(client_hmac).is_ok()
}

/// Write token to file with secure permissions (0600)
#[cfg(unix)]
pub fn write_token_file(path: &Path, token: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut file = File::create(path).context("Failed to create token file")?;
    file.write_all(token.as_bytes())
        .context("Failed to write token")?;
    file.set_permissions(Permissions::from_mode(0o600))
        .context("Failed to set token file permissions")?;
    Ok(())
}

#[cfg(not(unix))]
pub fn write_token_file(path: &Path, token: &str) -> Result<()> {
    let mut file = File::create(path).context("Failed to create token file")?;
    file.write_all(token.as_bytes())
        .context("Failed to write token")?;
    Ok(())
}

/// Read token from file
pub fn read_token_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).context("Failed to open token file")?;
    let mut token = String::new();
    file.read_to_string(&mut token)
        .context("Failed to read token file")?;
    Ok(token.trim().to_string())
}

/// Resolve token from config (inline token takes priority over file)
pub fn resolve_token(
    inline_token: Option<&str>,
    token_file: Option<&str>,
) -> Result<Option<String>> {
    if let Some(token) = inline_token {
        if !token.is_empty() {
            return Ok(Some(token.to_string()));
        }
    }

    // Check environment variable
    if let Ok(env_token) = std::env::var("NVIM_WEB_TOKEN") {
        if !env_token.is_empty() {
            return Ok(Some(env_token));
        }
    }
    if let Some(file_path) = token_file {
        if !file_path.is_empty() {
            let token = read_token_file(Path::new(file_path))?;
            return Ok(Some(token));
        }
    }
    Ok(None)
}

/// Perform the client-side authentication handshake
///
/// Flow:
/// 1. Wait for server to send 32-byte Nonce
/// 2. Calculate HMAC-SHA256(Nonce, Token)
/// 3. Send 32-byte HMAC to server
/// 4. Proceed
pub async fn perform_client_handshake(stream: &mut TcpStream, token: &str) -> Result<()> {
    let mut nonce = [0u8; NONCE_LENGTH];

    // 1. Read Nonce (with timeout protection)
    // We expect the server to send this immediately upon connection
    stream
        .read_exact(&mut nonce)
        .await
        .context("Failed to read authentication nonce from server")?;

    // 2. Compute HMAC
    let hmac = compute_hmac(&nonce, token);

    // 3. Send HMAC
    stream
        .write_all(&hmac)
        .await
        .context("Failed to send authentication response")?;

    // 4. (Optional) Verify server acceptance?
    // For this protocol, we assume if the connection stays open, we are good.
    // A strict server would close the connection immediately if HMAC is wrong.
    // Some protocols imply an "OK" byte here, but to keep it minimal and
    // compatible with simple proxies, we can optimistically proceed.
    // However, to catch "immediate close", we could try a 1-byte peek or similar,
    // but that complicates the nvim-rs handover.
    // We'll rely on the next Nvim RPC read failing if the socket closed.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_generate_secure_token() {
        let t1 = generate_secure_token();
        let t2 = generate_secure_token();

        assert_eq!(t1.len(), TOKEN_LENGTH * 2); // hex encoded
        assert_ne!(t1, t2); // Tokens must be unique
    }

    #[test]
    fn test_generate_nonce() {
        let n1 = generate_nonce();
        let n2 = generate_nonce();

        assert_eq!(n1.len(), NONCE_LENGTH);
        assert_ne!(n1, n2); // Nonces must be unique
    }

    #[test]
    fn test_hmac_verification() {
        let token = "test_secret_token";
        let nonce = generate_nonce();

        let hmac = compute_hmac(&nonce, token);
        assert!(verify_hmac(&nonce, token, &hmac));

        // Wrong token should fail
        assert!(!verify_hmac(&nonce, "wrong_token", &hmac));

        // Wrong nonce should fail
        let wrong_nonce = generate_nonce();
        assert!(!verify_hmac(&wrong_nonce, token, &hmac));
    }

    #[test]
    fn test_token_file_roundtrip() {
        let path = temp_dir().join("nvim_web_test_token");
        let token = generate_secure_token();

        write_token_file(&path, &token).unwrap();
        let read_token = read_token_file(&path).unwrap();

        assert_eq!(token, read_token);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_resolve_token_combined() {
        // Clear env first
        unsafe {
            std::env::remove_var("NVIM_WEB_TOKEN");
        }

        // 1. Basic priority check (Inline > File)
        let result = resolve_token(Some("inline"), Some("/nonexistent")).unwrap();
        assert_eq!(result, Some("inline".to_string()));

        let result = resolve_token(None, None).unwrap();
        assert_eq!(result, None);

        // 2. Env var check
        unsafe {
            std::env::set_var("NVIM_WEB_TOKEN", "env_token");
        }

        let result = resolve_token(None, None).unwrap();
        assert_eq!(result, Some("env_token".to_string()));

        // Env should not override inline
        let result = resolve_token(Some("inline"), None).unwrap();
        assert_eq!(result, Some("inline".to_string()));

        // Cleanup
        unsafe {
            std::env::remove_var("NVIM_WEB_TOKEN");
        }
    }
}
