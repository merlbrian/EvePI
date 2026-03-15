use anyhow::Context as _;
use thiserror::Error;

/// Opaque token entry persisted per character/service.
#[derive(Debug, Clone)]
pub struct TokenEntry {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix timestamp (seconds) when the access token expires.
    pub expires_at: i64,
}

/// Named service entries used as keychain keys.
const KEYCHAIN_SERVICE: &str = "evepi";
const TOKEN_FILE_BASENAME: &str = "tokens.age";

#[derive(Debug, Error)]
pub enum TokenStoreError {
    #[error("token not found for key: {0}")]
    NotFound(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("storage I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialisation error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("encryption error: {0}")]
    Encryption(String),
}

/// Manages token persistence via OS keychain (primary) or an age-encrypted
/// file (fallback, common in WSL where no secret service daemon is running).
#[derive(Debug, Clone)]
pub struct TokenStore {
    /// Path to the age-encrypted fallback file.
    file_path: std::path::PathBuf,
}

impl TokenStore {
    /// Create a `TokenStore` using the default config directory.
    pub fn new() -> anyhow::Result<Self> {
        let config_dir = dirs::config_dir()
            .context("could not determine config directory")?
            .join("evepi");
        std::fs::create_dir_all(&config_dir).context("failed to create evepi config directory")?;
        Ok(Self {
            file_path: config_dir.join(TOKEN_FILE_BASENAME),
        })
    }

    /// Persist a token entry under `key`. Tries keychain first; falls back to
    /// the encrypted file.
    pub fn save(&self, key: &str, entry: &TokenEntry) -> anyhow::Result<()> {
        let json = serde_json::to_string(entry)?;
        if self.try_keychain_save(key, &json).is_ok() {
            return Ok(());
        }
        self.file_save(key, &json)
    }

    /// Retrieve a token entry by `key`.
    pub fn load(&self, key: &str) -> anyhow::Result<TokenEntry> {
        if let Ok(json) = self.try_keychain_load(key) {
            return Ok(serde_json::from_str(&json)?);
        }
        let json = self.file_load(key)?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Remove a token entry (e.g., on logout).
    pub fn delete(&self, key: &str) -> anyhow::Result<()> {
        let _ = self.try_keychain_delete(key);
        self.file_delete(key)
    }

    // --- Keychain helpers (best-effort) ---

    fn try_keychain_save(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, key).context("keyring entry creation failed")?;
        entry
            .set_password(value)
            .context("keyring set_password failed")?;
        Ok(())
    }

    fn try_keychain_load(&self, key: &str) -> anyhow::Result<String> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, key).context("keyring entry creation failed")?;
        entry.get_password().context("keyring get_password failed")
    }

    fn try_keychain_delete(&self, key: &str) -> anyhow::Result<()> {
        let entry =
            keyring::Entry::new(KEYCHAIN_SERVICE, key).context("keyring entry creation failed")?;
        entry
            .delete_credential()
            .context("keyring delete_credential failed")?;
        Ok(())
    }

    // --- Age-encrypted file helpers ---

    fn file_save(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        // TODO(Phase 2): implement age-encrypted file writes.
        // Passphrase is prompted once and cached in memory for the session.
        anyhow::bail!("age-encrypted token file storage not yet implemented")
    }

    fn file_load(&self, _key: &str) -> anyhow::Result<String> {
        // TODO(Phase 2): implement age-encrypted file reads.
        anyhow::bail!("age-encrypted token file storage not yet implemented")
    }

    fn file_delete(&self, _key: &str) -> anyhow::Result<()> {
        // TODO(Phase 2): implement age-encrypted file delete.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn eve_token_key_format() {
        let character_id: i64 = 12345678;
        let key = format!("eve_char_{character_id}");
        assert_eq!(key, "eve_char_12345678");
        assert!(key.starts_with("eve_char_"));
    }

    #[test]
    fn github_token_key() {
        // The GitHub token is stored under a fixed key — never a per-character key.
        let key = "github_token";
        assert!(!key.contains("eve_char_"));
        assert_eq!(key, "github_token");
    }

    #[test]
    fn token_key_no_collision() {
        // Different character IDs must produce different keys.
        let key_a = format!("eve_char_{}", 1i64);
        let key_b = format!("eve_char_{}", 2i64);
        assert_ne!(key_a, key_b);
        assert_ne!(key_a, "github_token");
    }
}

impl serde::Serialize for TokenEntry {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("TokenEntry", 3)?;
        st.serialize_field("access_token", &self.access_token)?;
        st.serialize_field("refresh_token", &self.refresh_token)?;
        st.serialize_field("expires_at", &self.expires_at)?;
        st.end()
    }
}

impl<'de> serde::Deserialize<'de> for TokenEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        struct Raw {
            access_token: String,
            refresh_token: String,
            expires_at: i64,
        }
        let r = Raw::deserialize(d)?;
        Ok(TokenEntry {
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            expires_at: r.expires_at,
        })
    }
}
