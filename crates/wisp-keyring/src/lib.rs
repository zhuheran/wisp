use thiserror::Error;
use keyring::{Entry, Error as KeyringError};
use serde::{Deserialize, Serialize};

#[derive(Error, Debug)]
pub enum KeyManagerError {
	#[error("Keyring error: {0}")]
    KeyringError(#[from] KeyringError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyManager {
    service_name: String,
}

impl KeyManager {
    pub fn new(service_name: String) -> Self {
        Self { service_name }
    }

	fn get_credential_name(&self, name: &str) -> String {
		format!("config.provider.{}.key", name)
	}

    pub fn set_api_key(&self, name: &str, key: &str) -> Result<(), KeyManagerError> {
        Entry::new(&self.service_name, &self.get_credential_name(name))?
            .set_password(key)?;
        Ok(())
    }

    pub fn get_api_key(&self, name: &str) -> Result<String, KeyManagerError> {
        let key = Entry::new(&self.service_name, &self.get_credential_name(name))?
            .get_password()?;
		Ok(key)
    }

    pub fn delete_api_key(&self, name: &str) -> Result<(), KeyManagerError> {
        Entry::new(&self.service_name, &self.get_credential_name(name))?
            .delete_credential()?;
		Ok(())
    }
}
