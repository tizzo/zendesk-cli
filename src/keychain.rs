//! Secure storage for the Zendesk API token via the OS credential store.
//!
//! On macOS this is the login Keychain; on Windows the Credential Manager; on
//! Linux the Secret Service (e.g. GNOME Keyring / KWallet). The token is keyed
//! by Zendesk subdomain, so credentials for multiple instances coexist.
//!
//! Only the token lives here — the non-secret `subdomain` and `email` stay in
//! the plaintext config file.

use anyhow::{Context, Result};
use keyring::Entry;

/// Service name shown in the credential store (e.g. Keychain Access).
const SERVICE: &str = "zendesk-cli";

fn entry(subdomain: &str) -> Result<Entry> {
    Entry::new(SERVICE, subdomain)
        .with_context(|| format!("opening keychain entry for subdomain '{subdomain}'"))
}

/// Store (or overwrite) the API token for a subdomain.
pub fn store_token(subdomain: &str, token: &str) -> Result<()> {
    entry(subdomain)?
        .set_password(token)
        .context("storing API token in the OS keychain")
}

/// Fetch the API token for a subdomain, if one is stored.
pub fn get_token(subdomain: &str) -> Result<Option<String>> {
    match entry(subdomain)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).context("reading API token from the OS keychain"),
    }
}

/// Delete the stored token for a subdomain. Returns `true` if one was removed.
pub fn delete_token(subdomain: &str) -> Result<bool> {
    match entry(subdomain)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(e).context("deleting API token from the OS keychain"),
    }
}
