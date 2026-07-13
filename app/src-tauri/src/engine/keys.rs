//! API key storage in the Windows Credential Manager via the keyring crate.
//! The key never crosses the IPC boundary to the webview; the UI only sees
//! presence + last four characters.

use keyring::Entry;

use crate::error::{Error, Result};

const SERVICE: &str = "Compendium";
const USER: &str = "cohere-api-key";

fn entry() -> Result<Entry> {
    Entry::new(SERVICE, USER).map_err(|e| Error::Keyring(e.to_string()))
}

pub fn store_key(key: &str) -> Result<()> {
    entry()?
        .set_password(key)
        .map_err(|e| Error::Keyring(e.to_string()))
}

pub fn read_key() -> Result<Option<String>> {
    match entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::Keyring(e.to_string())),
    }
}

pub fn delete_key() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error::Keyring(e.to_string())),
    }
}
