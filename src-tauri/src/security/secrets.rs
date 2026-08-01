use std::path::Path;
#[cfg(target_os = "android")]
use std::path::PathBuf;
#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(not(target_os = "android"))]
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

#[cfg(all(not(test), not(target_os = "android")))]
pub const SERVICE: &str = "KindroidManager";
#[cfg(all(not(test), not(target_os = "android")))]
pub const USER: &str = "api_token";

// Android has no `keyring` backend, so the token is stored as a plaintext
// file under the app's data dir. The app sandbox protects against
// cross-app reads, and Android's file-based encryption protects data at
// rest when the device is locked. A rooted device or backup extract can
// still read the file. This is acceptable for the personal-sideload
// threat model (see .kilo/plans/1785596514600-android-deployment-plan.md
// step 3); Keystore-backed encryption is a deferred upgrade.
#[cfg(target_os = "android")]
static ANDROID_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

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

#[cfg(not(target_os = "android"))]
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

#[cfg(target_os = "android")]
impl Secrets {
    /// Populate the Android data dir used to locate the token file. Must
    /// be called once during app setup, before any other `Secrets::*` call.
    pub fn init(data_dir: PathBuf) {
        let _ = ANDROID_DATA_DIR.set(data_dir);
    }

    fn path() -> Result<PathBuf, SecretStoreError> {
        let dir = ANDROID_DATA_DIR
            .get()
            .ok_or(SecretStoreError::Unavailable)?;
        Ok(dir.join("token"))
    }

    pub fn set(token: &str) -> Result<(), SecretStoreError> {
        write_token_file(&Self::path()?, token)
    }

    pub fn exists() -> bool {
        Self::path()
            .ok()
            .and_then(|p| std::fs::read(&p).ok())
            .is_some()
    }

    pub fn get() -> Result<String, SecretStoreError> {
        read_token_file(&Self::path()?)?.ok_or(SecretStoreError::NotFound)
    }

    pub fn clear() -> Result<(), SecretStoreError> {
        delete_token_file(&Self::path()?)
    }
}

#[cfg(not(target_os = "android"))]
fn map_err(e: keyring::Error) -> SecretStoreError {
    use keyring::Error;
    match e {
        Error::NoEntry => SecretStoreError::NotFound,
        Error::PlatformFailure(_) | Error::NoStorageAccess(_) => SecretStoreError::Unavailable,
        Error::Ambiguous(_) | Error::Invalid(_, _) => SecretStoreError::AccessDenied,
        other => SecretStoreError::Other(other.to_string()),
    }
}

// --- File I/O helpers ---
//
// These are shared between the Android backend and host-side unit tests
// (run with `cargo test`, regardless of `cfg(target_os = "android")`).
// They have no platform-specific code so the round-trip behaviour is
// identical on the test machine and on a real Android device.

fn read_token_file(p: &Path) -> Result<Option<String>, SecretStoreError> {
    match std::fs::read_to_string(p) {
        Ok(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SecretStoreError::Other(e.to_string())),
    }
}

fn write_token_file(p: &Path, token: &str) -> Result<(), SecretStoreError> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SecretStoreError::Other(e.to_string()))?;
    }
    std::fs::write(p, token).map_err(|e| SecretStoreError::Other(e.to_string()))
}

fn delete_token_file(p: &Path) -> Result<(), SecretStoreError> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(SecretStoreError::Other(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");

        assert_eq!(read_token_file(&path).unwrap(), None);

        write_token_file(&path, "abc123").unwrap();
        assert_eq!(read_token_file(&path).unwrap().as_deref(), Some("abc123"));

        delete_token_file(&path).unwrap();
        assert_eq!(read_token_file(&path).unwrap(), None);
    }

    #[test]
    fn write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("subdir").join("token");
        write_token_file(&path, "tok").unwrap();
        assert_eq!(read_token_file(&path).unwrap().as_deref(), Some("tok"));
    }

    #[test]
    fn empty_file_treated_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        write_token_file(&path, "").unwrap();
        assert_eq!(read_token_file(&path).unwrap(), None);
    }

    #[test]
    fn delete_missing_file_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist");
        delete_token_file(&path).unwrap();
    }

    #[test]
    fn write_overwrites_previous() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        write_token_file(&path, "first").unwrap();
        write_token_file(&path, "second").unwrap();
        assert_eq!(read_token_file(&path).unwrap().as_deref(), Some("second"));
    }
}
