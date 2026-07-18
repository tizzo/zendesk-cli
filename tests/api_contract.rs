//! End-to-end JSON-contract tests against a mock Zendesk API.
//!
//! These lock the `--json` output shapes that automated consumers (scripts and
//! Claude Code workflows) depend on, and verify the public/internal reply
//! semantics by inspecting the actual request body sent to the server.

mod common;

use common::{api_base, isolated_cmd};
use httpmock::prelude::*;
use predicates::str::contains;
use serde_json::{json, Value};

/// Parse a successful command's stdout as JSON.
fn json_stdout(assert: assert_cmd::assert::Assert) -> Value {
    let out = assert.success();
    serde_json::from_slice(&out.get_output().stdout).expect("stdout is valid JSON")
}

#[test]
fn whoami_json_exposes_identity() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v2/users/me.json");
        then.status(200).json_body(json!({
            "user": {"id": 7, "name": "Ada", "email": "ada@acme.com", "role": "admin"}
        }));
    });

    let (mut cmd, _home) = isolated_cmd();
    let v = json_stdout(
        cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
            .args(["whoami", "--json"])
            .assert(),
    );
    mock.assert();
    assert_eq!(v["id"], 7);
    assert_eq!(v["email"], "ada@acme.com");
}

#[test]
fn ticket_show_json_includes_description() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v2/tickets/123.json");
        then.status(200).json_body(json!({
            "ticket": {"id": 123, "subject": "S", "description": "the body", "status": "open"}
        }));
    });

    let (mut cmd, _home) = isolated_cmd();
    let v = json_stdout(
        cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
            .args(["ticket", "show", "123", "--json"])
            .assert(),
    );
    mock.assert();
    assert_eq!(v["id"], 123);
    assert_eq!(v["description"], "the body");
}

#[test]
fn ticket_show_accepts_interface_url() {
    // Passing the full agent URL must resolve to the same ticket id — proves
    // idref parsing is wired through the clap value_parser end-to-end.
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v2/tickets/123.json");
        then.status(200).json_body(json!({"ticket": {"id": 123}}));
    });

    let (mut cmd, _home) = isolated_cmd();
    cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
        .args([
            "ticket",
            "show",
            "https://acme.zendesk.com/agent/tickets/123",
            "--json",
        ])
        .assert()
        .success();
    mock.assert();
}

#[test]
fn view_tickets_json_reports_total_and_filters_by_status() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v2/views/999/tickets.json");
        then.status(200).json_body(json!({
            "tickets": [
                {"id": 1, "status": "open",   "subject": "a"},
                {"id": 2, "status": "closed", "subject": "b"},
                {"id": 3, "status": "open",   "subject": "c"}
            ],
            "count": 3
        }));
    });

    let (mut cmd, _home) = isolated_cmd();
    let v = json_stdout(
        cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
            .args(["view", "tickets", "999", "--status", "open", "--json"])
            .assert(),
    );
    mock.assert();

    assert_eq!(v["total"], 3, "total reflects the unfiltered view count");
    assert_eq!(v["shown"], 2, "status filter drops the closed ticket");
    let ids: Vec<i64> = v["tickets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![1, 3]);
}

#[test]
fn comments_json_labels_public_and_internal() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET).path("/api/v2/tickets/55/comments.json");
        then.status(200).json_body(json!({
            "comments": [
                {"id": 1, "public": true,  "body": "customer-facing"},
                {"id": 2, "public": false, "body": "internal note"}
            ]
        }));
    });

    let (mut cmd, _home) = isolated_cmd();
    let v = json_stdout(
        cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
            .args(["ticket", "comments", "55", "--json"])
            .assert(),
    );
    mock.assert();
    assert_eq!(v[0]["public"], true);
    assert_eq!(v[1]["public"], false);
}

#[test]
fn reply_internal_sends_public_false() {
    let server = MockServer::start();
    // The mock only matches if the request body carries public:false.
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/v2/tickets/77.json")
            .json_body_partial(r#"{"ticket":{"comment":{"public":false}}}"#);
        then.status(200)
            .json_body(json!({"ticket": {"id": 77, "status": "open"}}));
    });

    let (mut cmd, _home) = isolated_cmd();
    let v = json_stdout(
        cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
            .args([
                "reply",
                "77",
                "--internal",
                "--body",
                "looking into it",
                "--json",
            ])
            .assert(),
    );
    mock.assert();
    assert_eq!(v["public"], false);
    assert_eq!(v["posted"], true);
}

#[test]
fn reply_public_sends_public_true() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/api/v2/tickets/77.json")
            .json_body_partial(r#"{"ticket":{"comment":{"public":true}}}"#);
        then.status(200)
            .json_body(json!({"ticket": {"id": 77, "status": "open"}}));
    });

    let (mut cmd, _home) = isolated_cmd();
    cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
        .args(["reply", "77", "--public", "--body", "all set", "--json"])
        .assert()
        .success();
    mock.assert();
}

#[test]
fn api_error_status_surfaces_a_readable_message() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/api/v2/tickets/404.json");
        then.status(404)
            .json_body(json!({"error": "RecordNotFound"}));
    });

    let (mut cmd, _home) = isolated_cmd();
    cmd.env("ZENDESK_BASE_URL", api_base(&server.base_url()))
        .args(["ticket", "show", "404"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}
