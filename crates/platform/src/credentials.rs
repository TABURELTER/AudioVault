//! Secrets in the OS credential store (Windows Credential Manager, macOS
//! Keychain, Secret Service on Linux). Never stored in SQLite or logs.

use crate::{PlatformError, Result};

pub trait CredentialStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
}

/// Store backed by the `keyring` crate; entries live under one service name.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new(service: impl Into<String>) -> Self {
        KeyringStore { service: service.into() }
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, account).map_err(|e| PlatformError::Credentials(e.to_string()))
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        KeyringStore::new("AudioVault")
    }
}

impl CredentialStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        match self.entry(account)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(PlatformError::Credentials(e.to_string())),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.entry(account)?.set_password(secret).map_err(|e| PlatformError::Credentials(e.to_string()))
    }

    fn delete(&self, account: &str) -> Result<()> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(PlatformError::Credentials(e.to_string())),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore {
    map: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl CredentialStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.map.lock().map_err(|_| PlatformError::Other("poisoned".into()))?.get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.map.lock().map_err(|_| PlatformError::Other("poisoned".into()))?.insert(account.into(), secret.into());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.map.lock().map_err(|_| PlatformError::Other("poisoned".into()))?.remove(account);
        Ok(())
    }
}
