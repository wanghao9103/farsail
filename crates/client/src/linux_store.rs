//! Ubuntu desktop Secret Service storage. Never fall back to plaintext or a session-only keyring.
use crate::{Error, Result, SecureStore};
use keyring::Entry;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Mutex};

pub struct LinuxStore {
    service: String,
    access: Mutex<()>,
}

impl LinuxStore {
    pub fn new(directory: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&directory).map_err(|e| Error::Store(e.to_string()))?;
        let directory = directory
            .canonicalize()
            .map_err(|e| Error::Store(e.to_string()))?;
        // Isolate test profiles and separate application profiles in the same desktop keyring.
        use std::os::unix::ffi::OsStrExt;
        Ok(Self {
            service: format!(
                "app.farsail.desktop-{}",
                hex::encode(Sha256::digest(directory.as_os_str().as_bytes()))
            ),
            access: Mutex::new(()),
        })
    }

    fn entry(&self, key: &str) -> Result<Entry> {
        if key.is_empty() || !key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            return Err(Error::Store("invalid credential key".into()));
        }
        Entry::new(&self.service, key).map_err(store_error)
    }
}

fn store_error(_: keyring::Error) -> Error {
    // Backend errors can contain credential data; expose only actionable, public guidance.
    Error::Store("Ubuntu Secret Service unavailable; unlock the desktop login keyring and run FarSail in your desktop session".into())
}

impl SecureStore for LinuxStore {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let _access = self
            .access
            .lock()
            .map_err(|_| Error::Store("keyring lock poisoned".into()))?;
        match self.entry(key)?.get_secret() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(store_error(e)),
        }
    }

    fn write(&self, key: &str, value: &[u8]) -> Result<()> {
        let _access = self
            .access
            .lock()
            .map_err(|_| Error::Store("keyring lock poisoned".into()))?;
        self.entry(key)?.set_secret(value).map_err(store_error)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let _access = self
            .access
            .lock()
            .map_err(|_| Error::Store("keyring lock poisoned".into()))?;
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(store_error(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an unlocked Secret Service on an isolated session bus"]
    fn native_keyring_roundtrip_and_profile_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let other_dir = tempfile::tempdir().unwrap();
        let store = LinuxStore::new(dir.path().to_owned()).unwrap();
        let other = LinuxStore::new(other_dir.path().to_owned()).unwrap();
        let value = [0, 255, 42, 128];
        assert!(store.read("test-secret").unwrap().is_none());
        store.write("test-secret", &value).unwrap();
        assert_eq!(store.read("test-secret").unwrap(), Some(value.to_vec()));
        assert!(other.read("test-secret").unwrap().is_none());
        let reopened = LinuxStore::new(dir.path().join(".")).unwrap();
        assert_eq!(reopened.read("test-secret").unwrap(), Some(value.to_vec()));
        reopened.write("test-secret", b"replacement").unwrap();
        assert_eq!(
            store.read("test-secret").unwrap(),
            Some(b"replacement".to_vec())
        );
        store.delete("test-secret").unwrap();
        store.delete("test-secret").unwrap();
        assert!(store.read("test-secret").unwrap().is_none());
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }
}
