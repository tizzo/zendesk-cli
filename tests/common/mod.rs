//! Shared test helpers.
// Each integration-test binary includes this module and uses a different
// subset of helpers, so unused-in-one-binary is expected.
#![allow(dead_code)]

use assert_cmd::Command;
use tempfile::TempDir;

/// Build a `zd` command fully isolated from the developer's real environment:
///   - `HOME`/`XDG_CONFIG_HOME` point at a throwaway temp dir (no real config
///     file is read or written)
///   - credentials are supplied via env, so the OS keychain is never touched
///
/// Returns the command plus the `TempDir` backing `HOME` — keep the `TempDir`
/// alive for the duration of the test (bind it, don't drop it).
pub fn isolated_cmd() -> (Command, TempDir) {
    let home = tempfile::tempdir().expect("create temp HOME");
    let mut cmd = Command::cargo_bin("zd").expect("locate zd binary");
    cmd.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("ZENDESK_SUBDOMAIN", "acme")
        .env("ZENDESK_EMAIL", "agent@acme.com")
        .env("ZENDESK_API_TOKEN", "test-token")
        .env_remove("ZENDESK_BASE_URL");
    (cmd, home)
}

/// The API base URL to point the CLI at for a given mock server.
pub fn api_base(server_base_url: &str) -> String {
    format!("{server_base_url}/api/v2")
}
