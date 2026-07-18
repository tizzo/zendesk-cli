//! Offline CLI behavior: help/docs, argument validation, and the reply-safety
//! guard. None of these touch the network.

mod common;

use common::isolated_cmd;
use predicates::str::contains;

#[test]
fn docs_prints_overview() {
    let (mut cmd, _home) = isolated_cmd();
    cmd.arg("docs")
        .assert()
        .success()
        .stdout(contains("Zendesk ticket-reply CLI"));
}

#[test]
fn version_flag_works() {
    let (mut cmd, _home) = isolated_cmd();
    cmd.arg("--version")
        .assert()
        .success()
        .stdout(contains("zd"));
}

#[test]
fn reply_requires_explicit_visibility() {
    // The core safety contract: no visibility flag => refuse to post.
    let (mut cmd, _home) = isolated_cmd();
    cmd.args(["reply", "123", "--body", "hi"])
        .assert()
        .failure()
        .stderr(contains("exactly one visibility"));
}

#[test]
fn reply_rejects_both_visibilities() {
    let (mut cmd, _home) = isolated_cmd();
    cmd.args(["reply", "123", "--public", "--internal", "--body", "hi"])
        .assert()
        .failure()
        .stderr(contains("cannot be used with"));
}

#[test]
fn invalid_ticket_id_is_rejected() {
    let (mut cmd, _home) = isolated_cmd();
    cmd.args(["ticket", "show", "not-a-ticket"])
        .assert()
        .failure()
        .stderr(contains("not a numeric ID"));
}

#[test]
fn unknown_status_is_rejected() {
    let (mut cmd, _home) = isolated_cmd();
    cmd.args(["ticket", "list", "--status", "bogus"])
        .assert()
        .failure()
        .stderr(contains("invalid status"));
}

#[test]
fn view_tickets_without_id_or_default_errors_clearly() {
    // Isolated HOME => no default_view configured => actionable error.
    let (mut cmd, _home) = isolated_cmd();
    cmd.args(["view", "tickets"])
        .assert()
        .failure()
        .stderr(contains("no view ID given and no default view set"));
}
