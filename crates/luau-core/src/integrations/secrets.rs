//! Account secrets in the OS keychain (macOS Keychain, Windows Credential
//! Manager, Secret Service). Never written to settings, logs or exports.
//!
//! When no keychain is available (e.g. Linux without a Secret Service), the
//! secret is kept in memory for the session only and `persisted` is `false`.

use std::collections::HashMap;
use std::sync::LazyLock;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const SERVICE: &str = "app.luau.integrations";

/// Credentials of one account. `Debug` is redacted on purpose.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Secret {
    /// Jira Cloud: account email (Basic auth). Trello: API key. Server: username (legacy Basic).
    pub user: Option<String>,
    /// API token, PAT, Trello token or Slack bot token.
    pub token: String,
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret { <redacted> }")
    }
}

static MEMORY: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn use_memory_only() -> bool {
    cfg!(test) || std::env::var_os("LUAU_MEMORY_SECRETS").is_some()
}

/// Store a secret; returns whether it was persisted in the OS keychain.
pub fn set(account: &str, secret: &Secret) -> Result<bool> {
    let json = serde_json::to_string(secret).map_err(|e| Error::Other(e.to_string()))?;
    MEMORY.lock().insert(account.to_string(), json.clone());
    if use_memory_only() {
        return Ok(false);
    }
    match keyring::Entry::new(SERVICE, account).and_then(|e| e.set_password(&json)) {
        Ok(()) => Ok(true),
        Err(e) => {
            tracing::warn!(
                "keychain unavailable, keeping secret in memory: {}",
                keychain_err(&e)
            );
            Ok(false)
        }
    }
}

pub fn get(account: &str) -> Result<Secret> {
    if let Some(j) = MEMORY.lock().get(account) {
        return serde_json::from_str(j).map_err(|e| Error::Other(e.to_string()));
    }
    if use_memory_only() {
        return Err(Error::not_found("secret"));
    }
    let json = keyring::Entry::new(SERVICE, account)
        .and_then(|e| e.get_password())
        .map_err(|e| Error::NotFound(format!("secret: {}", keychain_err(&e))))?;
    MEMORY.lock().insert(account.to_string(), json.clone());
    serde_json::from_str(&json).map_err(|e| Error::Other(e.to_string()))
}

pub fn delete(account: &str) {
    MEMORY.lock().remove(account);
    if !use_memory_only()
        && let Ok(e) = keyring::Entry::new(SERVICE, account)
    {
        let _ = e.delete_credential();
    }
}

/// Error kind only (never the secret).
fn keychain_err(e: &keyring::Error) -> &'static str {
    match e {
        keyring::Error::NoEntry => "no entry",
        keyring::Error::NoStorageAccess(_) | keyring::Error::NoDefaultStore => "no storage access",
        keyring::Error::PlatformFailure(_) => "platform failure",
        _ => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_redacted_debug() {
        let s = Secret {
            user: Some("me@example.com".into()),
            token: "tok-123".into(),
        };
        assert!(!set("acc-test", &s).unwrap());
        assert_eq!(get("acc-test").unwrap(), s);
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("tok-123") && !dbg.contains("example.com"));
        delete("acc-test");
        assert!(get("acc-test").is_err());
    }
}
