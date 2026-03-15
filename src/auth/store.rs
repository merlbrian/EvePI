use anyhow::Context as _;
use age::secrecy::ExposeSecret as _;
use std::collections::HashMap;
use std::io::{Read, Write};
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
const IDENTITY_FILE_BASENAME: &str = ".identity";

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
    /// Cached result of the keyring capability probe.
    /// `true` = keyring provides real cross-instance persistence.
    /// `false` = keyring is mock/unavailable; always use the age file.
    keyring_works: std::sync::Arc<std::sync::OnceLock<bool>>,
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
            keyring_works: std::sync::Arc::new(std::sync::OnceLock::new()),
        })
    }

    /// Persist a token entry under `key`. Tries keychain first; falls back to
    /// the encrypted file.
    pub fn save(&self, key: &str, entry: &TokenEntry) -> anyhow::Result<()> {
        let json = serde_json::to_string(entry)?;
        if self.keyring_is_functional() && let Ok(()) = self.try_keychain_save(key, &json) {
            return Ok(());
        }
        self.file_save(key, &json)
    }

    /// Retrieve a token entry by `key`.
    pub fn load(&self, key: &str) -> anyhow::Result<TokenEntry> {
        if self.keyring_is_functional() && let Ok(json) = self.try_keychain_load(key) {
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

    /// Probe the keyring once to see if it actually persists across `Entry`
    /// instances. The mock backend (used when no real keychain is available)
    /// always returns `Ok` from `set_password` but stores nothing globally —
    /// a subsequent `Entry::new()` + `get_password()` returns nothing.
    fn keyring_is_functional(&self) -> bool {
        *self.keyring_works.get_or_init(|| {
            const PROBE_USER: &str = "__evepi_probe__";
            const PROBE_VALUE: &str = "1";
            let probe = (|| -> Option<bool> {
                let e1 = keyring::Entry::new(KEYCHAIN_SERVICE, PROBE_USER).ok()?;
                e1.set_password(PROBE_VALUE).ok()?;
                let e2 = keyring::Entry::new(KEYCHAIN_SERVICE, PROBE_USER).ok()?;
                let v = e2.get_password().ok()?;
                let _ = e2.delete_credential();
                Some(v == PROBE_VALUE)
            })();
            probe.unwrap_or(false)
        })
    }

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

    /// Load the X25519 identity from `~/.config/evepi/.identity`, creating it
    /// with mode 0600 on first use.
    fn load_or_create_identity(&self) -> anyhow::Result<age::x25519::Identity> {
        let identity_path = self
            .file_path
            .parent()
            .context("token file path has no parent")?   
            .join(IDENTITY_FILE_BASENAME);

        if identity_path.exists() {
            let raw = std::fs::read_to_string(&identity_path)
                .context("failed to read age identity file")?;
            let identity = raw
                .trim()
                .parse::<age::x25519::Identity>()
                .map_err(|e| anyhow::anyhow!("failed to parse age identity: {e}"))?;
            return Ok(identity);
        }

        // Generate a fresh identity and persist it with restricted permissions.
        let identity = age::x25519::Identity::generate();
        let secret = identity.to_string();
        write_file_0600(&identity_path, secret.expose_secret().as_bytes())
            .context("failed to write age identity file")?;
        Ok(identity)
    }

    /// Read the full token map from the encrypted file. Returns an empty map
    /// if the file does not yet exist.
    fn read_token_map(
        &self,
        identity: &age::x25519::Identity,
    ) -> anyhow::Result<HashMap<String, String>> {
        if !self.file_path.exists() {
            return Ok(HashMap::new());
        }
        let ciphertext =
            std::fs::read(&self.file_path).context("failed to read token file")?;
        let decryptor = age::Decryptor::new(ciphertext.as_slice())
            .context("failed to parse age token file")?;
        let mut plaintext = vec![];
        match decryptor {
            age::Decryptor::Recipients(d) => {
                let mut reader = d
                    .decrypt(std::iter::once(identity as &dyn age::Identity))
                    .context("failed to decrypt token file")?;
                reader
                    .read_to_end(&mut plaintext)
                    .context("failed to read decrypted token data")?;
            }
            age::Decryptor::Passphrase(_) => {
                anyhow::bail!("token file uses passphrase encryption — expected recipients mode");
            }
        }
        serde_json::from_slice(&plaintext).context("failed to parse token map JSON")
    }

    /// Encrypt and atomically write the token map to disk (mode 0600).
    fn write_token_map(
        &self,
        identity: &age::x25519::Identity,
        map: &HashMap<String, String>,
    ) -> anyhow::Result<()> {
        let plaintext = serde_json::to_vec(map).context("failed to serialise token map")?;
        let recipient = identity.to_public();
        let encryptor =
            age::Encryptor::with_recipients(vec![Box::new(recipient)])
                .context("failed to create age encryptor (empty recipients list)")?;
        let mut ciphertext = vec![];
        let mut writer = encryptor
            .wrap_output(&mut ciphertext)
            .context("failed to create age writer")?;
        writer
            .write_all(&plaintext)
            .context("failed to write plaintext to age stream")?;
        writer.finish().context("failed to finalise age encryption")?;

        // Write to a temp file then rename for atomicity.
        let tmp = self.file_path.with_extension("tmp");
        write_file_0600(&tmp, &ciphertext).context("failed to write token temp file")?;
        std::fs::rename(&tmp, &self.file_path).context("failed to rename token file")?;
        Ok(())
    }

    fn file_save(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let identity = self.load_or_create_identity()?;
        let mut map = self.read_token_map(&identity).unwrap_or_default();
        map.insert(key.to_owned(), value.to_owned());
        self.write_token_map(&identity, &map)
    }

    fn file_load(&self, key: &str) -> anyhow::Result<String> {
        let identity = self.load_or_create_identity()?;
        let map = self.read_token_map(&identity)?;
        map.get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no token (age file) for key: {key}"))
    }

    fn file_delete(&self, key: &str) -> anyhow::Result<()> {
        let identity = self.load_or_create_identity()?;
        let mut map = self.read_token_map(&identity).unwrap_or_default();
        map.remove(key);
        self.write_token_map(&identity, &map)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write `data` to `path` creating the file with mode 0600 (owner read/write
/// only). Uses `O_TRUNC` so existing content is overwritten.
fn write_file_0600(path: &std::path::Path, data: &[u8]) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create file: {}", path.display()))?;
    f.write_all(data)
        .with_context(|| format!("failed to write file: {}", path.display()))?;
    Ok(())
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
