//! Telegram channel configuration
//!
//! Handles secure token storage using systemd credentials.
//! Tokens are encrypted at rest using the host key (or TPM2 if available).

use secrecy::{ExposeSecret, SecretString};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Credential name for Telegram bot token
const CREDENTIAL_NAME: &str = "telegram-token";
/// Default path for encrypted credentials
const CREDSTORE_PATH: &str = "/etc/credstore.encrypted";

/// Default debounce interval for message edits (500ms)
const DEFAULT_DEBOUNCE_MS: u64 = 500;
/// Telegram's maximum message length
const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4096;

/// Telegram configuration error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramConfigError {
    /// Token not found in credentials
    MissingToken,
    /// Credential store error
    CredentialError {
        /// Error description
        message: String,
    },
}

impl fmt::Display for TelegramConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingToken => write!(f, "Telegram token not found in systemd credentials"),
            Self::CredentialError { message } => write!(f, "credential error: {message}"),
        }
    }
}

impl std::error::Error for TelegramConfigError {}

/// Telegram channel configuration
#[derive(Clone)]
pub struct TelegramConfig {
    /// Bot token (zeroizes on drop, Debug won't expose)
    token: SecretString,
    /// Debounce interval for message edits
    pub debounce_interval: Duration,
    /// Maximum message length (Telegram limit is 4096)
    pub max_message_length: usize,
}

impl fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("token", &"[REDACTED]")
            .field("debounce_interval", &self.debounce_interval)
            .field("max_message_length", &self.max_message_length)
            .finish()
    }
}

impl TelegramConfig {
    /// Load configuration from systemd credentials
    ///
    /// Reads the decrypted token from `$CREDENTIALS_DIRECTORY/telegram-token`
    /// when running as a systemd service with `LoadCredentialEncrypted=`.
    ///
    /// # Errors
    ///
    /// Returns `MissingToken` if no credential is available.
    /// Returns `CredentialError` if reading fails.
    pub fn load() -> Result<Self, TelegramConfigError> {
        let token = Self::load_from_credentials()?;
        Ok(Self {
            token,
            debounce_interval: Duration::from_millis(DEFAULT_DEBOUNCE_MS),
            max_message_length: TELEGRAM_MAX_MESSAGE_LENGTH,
        })
    }

    /// Load token from systemd credentials directory
    fn load_from_credentials() -> Result<SecretString, TelegramConfigError> {
        // Check environment variable first (for development/testing without sudo)
        if let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") {
            return Ok(SecretString::from(token));
        }

        // Check if running under systemd with credentials
        if let Ok(creds_dir) = std::env::var("CREDENTIALS_DIRECTORY") {
            let cred_path = PathBuf::from(creds_dir).join(CREDENTIAL_NAME);
            if cred_path.exists() {
                let token = fs::read_to_string(&cred_path).map_err(|e| {
                    TelegramConfigError::CredentialError {
                        message: format!("failed to read credential: {e}"),
                    }
                })?;
                return Ok(SecretString::from(token.trim().to_string()));
            }
        }

        // Try to decrypt from credstore directly (for CLI testing)
        let encrypted_path = PathBuf::from(CREDSTORE_PATH).join(CREDENTIAL_NAME);
        if encrypted_path.exists() {
            return Self::decrypt_credential(&encrypted_path);
        }

        Err(TelegramConfigError::MissingToken)
    }

    /// Decrypt a credential using systemd-creds
    fn decrypt_credential(path: &std::path::Path) -> Result<SecretString, TelegramConfigError> {
        let output = Command::new("systemd-creds")
            .args(["decrypt", path.to_str().unwrap_or(""), "-"])
            .output()
            .map_err(|e| TelegramConfigError::CredentialError {
                message: format!("failed to run systemd-creds: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TelegramConfigError::CredentialError {
                message: format!("systemd-creds decrypt failed: {stderr}"),
            });
        }

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() {
            return Err(TelegramConfigError::MissingToken);
        }

        Ok(SecretString::from(token))
    }

    /// Store token using systemd-creds encrypt
    ///
    /// Encrypts the token and stores it in `/etc/credstore.encrypted/telegram-token`.
    /// Requires root privileges.
    ///
    /// # Errors
    ///
    /// Returns error if encryption or storage fails.
    pub fn store_token(token: &str) -> Result<(), TelegramConfigError> {
        use std::os::unix::fs::PermissionsExt;

        // Ensure credstore directory exists with read permissions for all
        let credstore = PathBuf::from(CREDSTORE_PATH);
        if !credstore.exists() {
            fs::create_dir_all(&credstore).map_err(|e| TelegramConfigError::CredentialError {
                message: format!("failed to create {CREDSTORE_PATH}: {e} (try with sudo)"),
            })?;
        }
        // Directory: rwxr-xr-x (755) - readable by all, writable by root
        fs::set_permissions(&credstore, fs::Permissions::from_mode(0o755)).map_err(|e| {
            TelegramConfigError::CredentialError {
                message: format!("failed to set directory permissions: {e}"),
            }
        })?;

        let cred_path = credstore.join(CREDENTIAL_NAME);

        // Encrypt using systemd-creds with host key (allows any local process to decrypt)
        let mut child = Command::new("systemd-creds")
            .args([
                "encrypt",
                "--with-key=host",
                "--name", CREDENTIAL_NAME,
                "-",  // read from stdin
                cred_path.to_str().unwrap_or(""),
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| TelegramConfigError::CredentialError {
                message: format!("failed to run systemd-creds: {e}"),
            })?;

        // Write token to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(token.as_bytes()).map_err(|e| {
                TelegramConfigError::CredentialError {
                    message: format!("failed to write token: {e}"),
                }
            })?;
        }

        let output = child.wait_with_output().map_err(|e| {
            TelegramConfigError::CredentialError {
                message: format!("systemd-creds failed: {e}"),
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TelegramConfigError::CredentialError {
                message: format!("systemd-creds encrypt failed: {stderr}"),
            });
        }

        // File: rw-r--r-- (644) - readable by all (it's encrypted), writable by root
        fs::set_permissions(&cred_path, fs::Permissions::from_mode(0o644)).map_err(|e| {
            TelegramConfigError::CredentialError {
                message: format!("failed to set file permissions: {e}"),
            }
        })?;

        tracing::info!("Telegram token encrypted and stored at {}", cred_path.display());
        Ok(())
    }

    /// Delete token from credential store
    ///
    /// # Errors
    ///
    /// Returns error if deletion fails.
    pub fn delete_token() -> Result<(), TelegramConfigError> {
        let cred_path = PathBuf::from(CREDSTORE_PATH).join(CREDENTIAL_NAME);

        if cred_path.exists() {
            fs::remove_file(&cred_path).map_err(|e| TelegramConfigError::CredentialError {
                message: format!("failed to delete credential: {e} (try with sudo)"),
            })?;
            tracing::info!("Telegram token removed from {}", cred_path.display());
        } else {
            tracing::info!("No credential found at {}", cred_path.display());
        }

        Ok(())
    }

    /// Get the bot token for use with teloxide
    ///
    /// Returns the token value - caller should not log or persist this.
    #[must_use]
    pub fn token(&self) -> &str {
        self.token.expose_secret()
    }

    /// Create a configuration for testing
    #[cfg(test)]
    #[must_use]
    pub fn for_test(token: &str) -> Self {
        Self {
            token: SecretString::from(token.to_string()),
            debounce_interval: Duration::from_millis(DEFAULT_DEBOUNCE_MS),
            max_message_length: TELEGRAM_MAX_MESSAGE_LENGTH,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_debug_redacts_token() {
        let config = TelegramConfig::for_test("secret-token-value");

        let debug_output = format!("{config:?}");
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("secret-token-value"));
    }

    #[test]
    fn error_display() {
        assert_eq!(
            TelegramConfigError::MissingToken.to_string(),
            "Telegram token not found in systemd credentials"
        );
        assert_eq!(
            TelegramConfigError::CredentialError {
                message: "test error".to_string()
            }
            .to_string(),
            "credential error: test error"
        );
    }
}
