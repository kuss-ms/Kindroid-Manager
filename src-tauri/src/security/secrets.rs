use keyring::Entry;
use serde::Serialize;
use thiserror::Error;

// Use a separate keyring entry in tests so `cargo test` doesn't clobber
// the user's real token. This previously left a 10-char "test-token"
// string in the user's OS keychain, which is exactly the bug that
// surfaced with the diagnostic.
#[cfg(test)]
pub const SERVICE: &str = "KindroidManager-test";
#[cfg(test)]
pub const USER: &str = "api_token-test";

#[cfg(not(test))]
pub const SERVICE: &str = "KindroidManager";
#[cfg(not(test))]
pub const USER: &str = "api_token";

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecretStoreError {
    #[error("OS keychain is not available")]
    Unavailable,
    #[error("keychain denied access")]
    AccessDenied,
    #[error("no token stored")]
    NotFound,
    #[error("keychain error: {0}")]
    Other(String),
}

pub struct Secrets;

impl Secrets {
    fn entry() -> Result<Entry, SecretStoreError> {
        Entry::new(SERVICE, USER).map_err(map_err)
    }

    pub fn set(token: &str) -> Result<(), SecretStoreError> {
        Self::entry()?.set_password(token).map_err(map_err)
    }

    pub fn exists() -> bool {
        Self::entry()
            .ok()
            .and_then(|e| e.get_password().ok())
            .is_some()
    }

    pub fn get() -> Result<String, SecretStoreError> {
        let entry = Self::entry()?;
        entry.get_password().map_err(map_err)
    }

    pub fn clear() -> Result<(), SecretStoreError> {
        let entry = Self::entry()?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

fn map_err(e: keyring::Error) -> SecretStoreError {
    use keyring::Error;
    match e {
        Error::NoEntry => SecretStoreError::NotFound,
        Error::PlatformFailure(_) | Error::NoStorageAccess(_) => SecretStoreError::Unavailable,
        Error::Ambiguous(_) | Error::Invalid(_, _) => SecretStoreError::AccessDenied,
        other => SecretStoreError::Other(other.to_string()),
    }
}
