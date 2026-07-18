//! Configuration loading.
//!
//! Credentials are resolved in this order (highest priority first):
//!   1. Command-line flags (`--subdomain`, `--email`, `--api-token`)
//!   2. Environment variables (`ZENDESK_SUBDOMAIN`, `ZENDESK_EMAIL`, `ZENDESK_API_TOKEN`)
//!   3. Config file at `$XDG_CONFIG_HOME/zendesk-cli/config.toml`
//!      (on macOS: `~/Library/Application Support/zendesk-cli/config.toml`)
//!
//! The API token is stored securely in the OS keychain (see [`crate::keychain`]),
//! not in the plaintext config file. A token left in a legacy config file is
//! still honored as a last-resort fallback.
//!
//! Zendesk uses API-token basic auth: the username is `{email}/token` and the
//! password is the API token. See
//! <https://developer.zendesk.com/api-reference/introduction/security-and-auth/>.

use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::keychain;

/// Where the resolved API token came from (for display / diagnostics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Flag,
    Env,
    Keychain,
    LegacyFile,
}

impl TokenSource {
    pub fn label(self) -> &'static str {
        match self {
            TokenSource::Flag => "--api-token flag",
            TokenSource::Env => "ZENDESK_API_TOKEN env var",
            TokenSource::Keychain => "OS keychain",
            TokenSource::LegacyFile => "config file (legacy, insecure)",
        }
    }
}

/// Resolved, ready-to-use credentials.
#[derive(Debug, Clone)]
pub struct Config {
    pub subdomain: String,
    pub email: String,
    pub api_token: String,
    /// Default view (agent filter) ID for `zd view tickets` with no argument.
    pub default_view: Option<i64>,
    /// Overrides the derived API base URL. Set via the hidden `ZENDESK_BASE_URL`
    /// env var; used for testing against a mock server (and advanced routing).
    pub base_url_override: Option<String>,
}

/// On-disk config file shape. All fields optional so a partial file is valid.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FileConfig {
    pub subdomain: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
    pub default_view: Option<i64>,
}

/// CLI-provided overrides, threaded down from the global args.
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub subdomain: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
}

impl Config {
    /// The base API URL, e.g. `https://acme.zendesk.com/api/v2`.
    pub fn base_url(&self) -> String {
        self.base_url_override
            .clone()
            .unwrap_or_else(|| format!("https://{}.zendesk.com/api/v2", self.subdomain))
    }

    /// Basic-auth username per Zendesk's API-token scheme.
    pub fn basic_auth_user(&self) -> String {
        format!("{}/token", self.email)
    }
}

/// Path to the config file (does not guarantee it exists).
pub fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("", "", "zendesk-cli")
        .ok_or_else(|| anyhow!("could not determine a config directory for this platform"))?;
    Ok(dirs.config_dir().join("config.toml"))
}

fn load_file() -> Result<FileConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config file {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing config file {}", path.display()))
}

/// Resolve one string setting by precedence: CLI flag → env var (ignoring
/// empty) → config file. This is the single source of truth for that order.
fn pick(flag: &Option<String>, env: &str, from_file: Option<String>) -> Option<String> {
    flag.clone()
        .or_else(|| std::env::var(env).ok().filter(|s| !s.is_empty()))
        .or(from_file)
}

/// Resolve a full [`Config`] from overrides, env, keychain, and file.
pub fn resolve(overrides: &Overrides) -> Result<Config> {
    Ok(resolve_with_source(overrides)?.0)
}

/// Like [`resolve`], but also reports where the token came from.
///
/// Token priority: `--api-token` flag → `ZENDESK_API_TOKEN` env → OS keychain
/// (keyed by subdomain) → legacy config file.
pub fn resolve_with_source(overrides: &Overrides) -> Result<(Config, TokenSource)> {
    let file = load_file()?;

    let missing = |name: &str| {
        let flag = name.replace('_', "-");
        let env = format!("ZENDESK_{}", name.to_uppercase());
        anyhow!(
            "missing {name}. Set it via --{flag}, the {env} env var, or run `zd config set`. \
             See `zd config path` for the config file location."
        )
    };

    let subdomain = pick(&overrides.subdomain, "ZENDESK_SUBDOMAIN", file.subdomain)
        .ok_or_else(|| missing("subdomain"))?;
    let email =
        pick(&overrides.email, "ZENDESK_EMAIL", file.email).ok_or_else(|| missing("email"))?;

    // Token: flag → env → keychain (by subdomain) → legacy file.
    let (api_token, source) = if let Some(t) = overrides.api_token.clone() {
        (t, TokenSource::Flag)
    } else if let Some(t) = std::env::var("ZENDESK_API_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
    {
        (t, TokenSource::Env)
    } else if let Some(t) = keychain::get_token(&subdomain)? {
        (t, TokenSource::Keychain)
    } else if let Some(t) = file.api_token {
        (t, TokenSource::LegacyFile)
    } else {
        return Err(anyhow!(
            "missing api_token. Store it securely with `zd config set --api-token <TOKEN>` \
             (saved to the OS keychain), or set the ZENDESK_API_TOKEN env var."
        ));
    };

    let base_url_override = std::env::var("ZENDESK_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty());

    Ok((
        Config {
            subdomain,
            email,
            api_token,
            default_view: file.default_view,
            base_url_override,
        },
        source,
    ))
}

/// Merge non-secret fields (`subdomain`, `email`) into the config file, creating
/// it if needed. The API token is never written here — use [`crate::keychain`].
///
/// If `clear_legacy_token` is set, any plaintext token left in an old config
/// file is removed (used when migrating a token into the keychain).
pub fn save_nonsecret(
    subdomain: Option<String>,
    email: Option<String>,
    default_view: Option<i64>,
    clear_legacy_token: bool,
) -> Result<PathBuf> {
    let path = config_path()?;
    let mut current = load_file().unwrap_or_default();

    if subdomain.is_some() {
        current.subdomain = subdomain;
    }
    if email.is_some() {
        current.email = email;
    }
    if default_view.is_some() {
        current.default_view = default_view;
    }
    if clear_legacy_token {
        current.api_token = None;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating config dir {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(&current).context("serializing config")?;
    std::fs::write(&path, text)
        .with_context(|| format!("writing config file {}", path.display()))?;
    Ok(path)
}

/// Resolve just the subdomain from overrides → env → file (no token needed).
/// Used when storing a token so we know which keychain entry to write.
pub fn resolve_subdomain(overrides: &Overrides) -> Result<String> {
    let file = load_file()?;
    pick(&overrides.subdomain, "ZENDESK_SUBDOMAIN", file.subdomain).ok_or_else(|| {
        anyhow!(
            "a subdomain is required to store the token (the keychain entry is keyed by it). \
             Pass --subdomain, set ZENDESK_SUBDOMAIN, or include it in `zd config set`."
        )
    })
}

/// Read the configured default view ID from the config file only — no keychain
/// access, so `zd view tickets` (with no ID) doesn't trigger a credential fetch.
pub fn default_view() -> Result<Option<i64>> {
    Ok(load_file()?.default_view)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(subdomain: &str, base_url_override: Option<&str>) -> Config {
        Config {
            subdomain: subdomain.into(),
            email: "e@x.com".into(),
            api_token: "tok".into(),
            default_view: None,
            base_url_override: base_url_override.map(str::to_string),
        }
    }

    #[test]
    fn base_url_derived_from_subdomain() {
        assert_eq!(
            cfg("acme", None).base_url(),
            "https://acme.zendesk.com/api/v2"
        );
    }

    #[test]
    fn base_url_override_wins() {
        assert_eq!(
            cfg("acme", Some("http://localhost:1234/api/v2")).base_url(),
            "http://localhost:1234/api/v2"
        );
    }

    #[test]
    fn basic_auth_user_appends_token_scheme() {
        assert_eq!(cfg("acme", None).basic_auth_user(), "e@x.com/token");
    }

    #[test]
    fn pick_prefers_flag_over_env_and_file() {
        // Unique env name so parallel tests never collide on it.
        assert_eq!(
            pick(&Some("flag".into()), "ZD_TEST_PICK_1", Some("file".into())),
            Some("flag".into())
        );
    }

    #[test]
    fn pick_uses_env_over_file() {
        std::env::set_var("ZD_TEST_PICK_2", "envval");
        assert_eq!(
            pick(&None, "ZD_TEST_PICK_2", Some("file".into())),
            Some("envval".into())
        );
        std::env::remove_var("ZD_TEST_PICK_2");
    }

    #[test]
    fn pick_ignores_empty_env() {
        std::env::set_var("ZD_TEST_PICK_3", "");
        assert_eq!(
            pick(&None, "ZD_TEST_PICK_3", Some("file".into())),
            Some("file".into())
        );
        std::env::remove_var("ZD_TEST_PICK_3");
    }

    #[test]
    fn pick_falls_back_to_file() {
        assert_eq!(
            pick(&None, "ZD_TEST_PICK_4_UNSET", Some("file".into())),
            Some("file".into())
        );
    }
}
