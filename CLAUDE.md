# CLAUDE.md — guidance for Claude Code working in this repo

This is `zendesk-cli`, a Rust CLI (binary name `zd`) for reading and writing
Zendesk ticket replies.

## Orient yourself fast

- Run `cargo run -- docs` for a one-screen overview of every command.
- Run `zd <command> --help` (or `cargo run -- <command> --help`) for exact flags.
- The whole tool is small (~5 files under `src/`); read `src/main.rs` first —
  it defines the entire command surface via clap and dispatches each command.

## How the code is organized

| File            | Responsibility                                         |
|-----------------|--------------------------------------------------------|
| `src/main.rs`   | clap CLI definitions + `run()` dispatch                |
| `src/client.rs` | `ZendeskClient`: one async method per API operation    |
| `src/models.rs` | serde structs for the Zendesk API subset used          |
| `src/config.rs` | credential resolution: flags → env → keychain → file   |
| `src/keychain.rs` | secure API-token storage in the OS keychain (`keyring`) |
| `src/idref.rs`  | parse a resource ID from a bare number OR an interface URL |
| `src/output.rs` | `Format::{Human,Json}` rendering                       |

To add a command: add a variant to the relevant `enum` in `main.rs`, add a
client method in `client.rs`, and handle it in `run()`.

## Domain model (important)

A Zendesk "reply" is a **ticket comment**. Comments have a `public` boolean:

- `public: true`  → public reply, visible to the requester (customer)
- `public: false` → internal note, visible only to agents

Posting a reply is a `PUT /tickets/{id}.json` with a nested `comment` object —
see `ZendeskClient::add_comment`. Reading replies is
`GET /tickets/{id}/comments.json` — see `list_comments`.

A "view" (called an **agent filter** in the UI: `.../agent/filters/<ID>`) is a
saved ticket queue. `zd view tickets <id>` hits `GET /views/{id}/tickets.json`
(`list_view_tickets`); `zd view list` hits `GET /views.json`. A default view ID
can be stored in the config file (`default_view`) so `zd view tickets` needs no
argument — see `resolve_view_id` in `main.rs`.

Ticket/view **ID arguments also accept interface URLs** — every ID arg uses
`idref::parse_id` as its clap `value_parser`, which extracts the last numeric
path segment from a URL (so pasting `.../agent/tickets/123` works everywhere).

Ticket listings (`ticket list`, `ticket search`, `view tickets`) share
`ZendeskClient::collect_tickets`, which paginates via `next_page` and applies a
client-side `--status` filter, returning a `TicketList { tickets, total }`.
`--all` fetches every page (bounded by a `MAX_PAGES` safety cap). Rendering goes
through `emit_ticket_list` in `main.rs`.

## Conventions to preserve

- **Every command supports `--json`.** When adding output, render both a human
  form (in `output.rs`) and a JSON form (`print_json`).
- **`zd reply` must never default a visibility.** The user must pass exactly one
  of `--public` / `--internal`. Keep this guard.
- Errors go through `anyhow`; surface Zendesk API errors with useful hints
  (`client.rs::send` maps common status codes).
- Keep help text accurate — it is the primary documentation.

## Credentials & secrets

- The **API token is never stored in plaintext**. `zd config set --api-token`
  writes it to the OS keychain via `src/keychain.rs` (`keyring` crate), keyed by
  subdomain. `subdomain`/`email` go to the config file. Preserve this split.
- Token resolution priority lives in `config::resolve_with_source`:
  flag → `ZENDESK_API_TOKEN` env → keychain → legacy file (with warning).
- `zd config clear-token` deletes the keychain entry.

## Running against a real account

Needs `ZENDESK_SUBDOMAIN`, `ZENDESK_EMAIL`, `ZENDESK_API_TOKEN` (or
`zd config set`). Without credentials, `--help`, `docs`, and `config path`
still work offline. `zd whoami` is the cheapest way to validate auth.

## Build / check

```sh
cargo build            # or cargo build --release
cargo clippy --all-targets -- -D warnings
cargo test             # unit + integration; no network
```

## Tests (what's covered, and how to add more)

- **Unit tests** are `#[cfg(test)]` modules in `src/` (idref, config precedence,
  `status_matches`, `url_encode`, `validate_statuses`, `mask`, and a
  `Cli::command().debug_assert()` structure check).
- **Integration tests** in `tests/` drive the built binary via `assert_cmd`:
  - `tests/cli_offline.rs` — help/docs, arg validation, the reply-visibility guard.
  - `tests/api_contract.rs` — runs against a `httpmock` server, asserting the
    `--json` shapes and that `reply` sends the right `public` flag.
  - `tests/common/mod.rs` — `isolated_cmd()` gives a `zd` command with a temp
    `HOME` and env-provided creds (so no real config/keychain is touched);
    tests point it at the mock via the hidden `ZENDESK_BASE_URL` env var.
- To test a new API command: add a client method, then a mock-server test that
  pins its `--json` contract. `ZENDESK_BASE_URL` (see `config::resolve_with_source`)
  is the seam that makes the client point at any base URL.

## CI / releases

- `.github/workflows/ci.yml` — fmt + clippy + tests on Linux (push + PR).
- `.github/workflows/release.yml` — on push to `main`: Linux unit tests gate →
  build macOS (arm64 + x86_64), Windows, Linux → auto-publish a GitHub Release
  tagged `v<version>-<run_number>`. Bump `version` in `Cargo.toml` for a new
  version line.
