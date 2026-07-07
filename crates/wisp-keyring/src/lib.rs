use std::path::Path;
use std::sync::{Mutex, OnceLock};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use keyring::{Entry, Error as KeyringError};
use rusqlite::{params, Connection};
use thiserror::Error;

const MASTER_KEY_ENTRY: &str = "master_key";

const TABLE_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS secrets (
    name TEXT PRIMARY KEY,
    value BLOB NOT NULL
)";

#[derive(Error, Debug)]
pub enum KeyManagerError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] KeyringError),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Encryption error: {0}")]
    Crypto(String),
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct KeyManager {
    master_key: Vec<u8>,
    conn: Mutex<Connection>,
}

static GLOBAL: OnceLock<KeyManager> = OnceLock::new();

impl KeyManager {
    pub fn new(service_name: &str, db_path: impl AsRef<Path>) -> Result<Self, KeyManagerError> {
        let db_path = db_path.as_ref();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let master_key = load_or_create_master_key(service_name)?;
        let conn = Connection::open(db_path)?;
        conn.execute_batch(TABLE_SCHEMA)?;
        Ok(Self {
            master_key,
            conn: Mutex::new(conn),
        })
    }

    /// Process-wide singleton backed by the system keyring and an encrypted
    /// SQLite database stored at the platform's default data location.
    pub fn global() -> &'static KeyManager {
        GLOBAL.get_or_init(|| {
            KeyManager::new("wisp", default_db_path())
                .expect("failed to initialize global key manager")
        })
    }

    fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, KeyManagerError> {
        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| KeyManagerError::Crypto(e.to_string()))?;
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| KeyManagerError::Crypto(e.to_string()))?;
        let mut out = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, blob: &[u8]) -> Result<String, KeyManagerError> {
        if blob.len() < 12 {
            return Err(KeyManagerError::Crypto("ciphertext too short".into()));
        }
        let (nonce, ciphertext) = blob.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| KeyManagerError::Crypto(e.to_string()))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|e| KeyManagerError::Crypto(e.to_string()))?;
        String::from_utf8(plaintext).map_err(|e| KeyManagerError::Crypto(e.to_string()))
    }

    pub fn set_api_key(&self, name: &str, key: &str) -> Result<(), KeyManagerError> {
        let blob = self.encrypt(key)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO secrets (name, value) VALUES (?1, ?2)",
            params![name, blob],
        )?;
        Ok(())
    }

    pub fn get_api_key(&self, name: &str) -> Result<String, KeyManagerError> {
        let conn = self.conn.lock().unwrap();
        let result: Result<Vec<u8>, rusqlite::Error> = conn.query_row(
            "SELECT value FROM secrets WHERE name = ?1",
            params![name],
            |row| row.get(0),
        );
        match result {
            Ok(blob) => self.decrypt(&blob),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(KeyManagerError::NotFound(name.to_string()))
            }
            Err(e) => Err(KeyManagerError::Db(e)),
        }
    }

    pub fn delete_api_key(&self, name: &str) -> Result<(), KeyManagerError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM secrets WHERE name = ?1", params![name])?;
        Ok(())
    }
}

fn load_or_create_master_key(service_name: &str) -> Result<Vec<u8>, KeyManagerError> {
    let entry = Entry::new(service_name, MASTER_KEY_ENTRY)?;
    match entry.get_password() {
        Ok(stored) => BASE64
            .decode(stored)
            .map_err(|e| KeyManagerError::Crypto(format!("invalid master key: {e}"))),
        Err(_) => {
            let key: [u8; 32] = rand::random();
            entry.set_password(&BASE64.encode(key))?;
            Ok(key.to_vec())
        }
    }
}

fn default_db_path() -> std::path::PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .ok()
            .map(std::path::PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .ok()
            .map(|h| std::path::PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(".local/share"))
            })
    };
    let mut base = base.unwrap_or_else(|| std::path::PathBuf::from("."));
    base.push("wisp");
    let _ = std::fs::create_dir_all(&base);
    base.push("keyring.db");
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "wisp-keyring-test-{}.db",
            rand::random::<u64>()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let path = temp_db();
        let km = KeyManager::new("test-svc", &path).unwrap();
        km.set_api_key("openai", "sk-secret-123").unwrap();

        let raw: Vec<u8> = km
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT value FROM secrets WHERE name = ?1",
                params!["openai"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("sk-secret-123"));

        assert_eq!(km.get_api_key("openai").unwrap(), "sk-secret-123");
        assert!(matches!(
            km.get_api_key("missing"),
            Err(KeyManagerError::NotFound(_))
        ));

        km.delete_api_key("openai").unwrap();
        assert!(matches!(
            km.get_api_key("openai"),
            Err(KeyManagerError::NotFound(_))
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn master_key_persists_across_instances() {
        let path = temp_db();
        let service = format!("test-svc-persist-{}", rand::random::<u64>());

        {
            let km = KeyManager::new(&service, &path).unwrap();
            km.set_api_key("anthropic", "topsecret").unwrap();
        }
        {
            let km = KeyManager::new(&service, &path).unwrap();
            assert_eq!(km.get_api_key("anthropic").unwrap(), "topsecret");
        }
        let _ = std::fs::remove_file(&path);
    }
}
