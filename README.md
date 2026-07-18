# zendesk-cli (`zd`)

A fast, self-documenting Rust CLI for reading and writing Zendesk ticket
replies — both **public** replies (customer-facing) and **internal** notes
(agent-only).

- **Fast**: async (`tokio` + `reqwest`), small release binary (LTO + stripped).
- **Self-documenting**: rich `--help` on every command, plus `zd docs`.
- **Agent-friendly**: `--json` on every command for structured output; explicit,
  unambiguous flags; never sends a customer-facing reply without `--public`.

## Install

```sh
cargo install --path .
# or run directly during development:
cargo run -- <args>
```

The binary is named `zd`.

## Authentication

Zendesk uses **API-token basic auth**. Create a token in Admin Center →
*Apps and integrations → APIs → Zendesk API*.

The **API token is stored securely in the OS keychain** (macOS Keychain,
Windows Credential Manager, or Linux Secret Service) — never written to disk in
plaintext. The non-secret `subdomain` and `email` live in the config file.

Token resolution order (highest priority first):

1. `--api-token` flag
2. `ZENDESK_API_TOKEN` env var
3. OS keychain (keyed by subdomain)
4. Legacy plaintext config file (fallback; a warning is printed)

```sh
# Option A: environment (nothing persisted)
export ZENDESK_SUBDOMAIN=acme        # the "acme" in acme.zendesk.com
export ZENDESK_EMAIL=you@acme.com
export ZENDESK_API_TOKEN=xxxxxxxx

# Option B: persist — subdomain/email to config file, token to the keychain
zd config set --subdomain acme --email you@acme.com --api-token xxxxxxxx

zd config show      # shows where the token was resolved from
zd config clear-token   # remove the token from the keychain
zd whoami           # verify
```

## Usage

```sh
zd whoami                                   # confirm auth
zd ticket show 12345                        # show one ticket
zd ticket list --limit 20                   # recent tickets
zd ticket search status:open priority:high  # Zendesk search syntax
zd ticket comments 12345                    # read replies (labeled PUBLIC/INTERNAL)

# Views (a.k.a. agent filters — the .../agent/filters/<ID> pages):
zd view list                                # list views with their IDs
zd view tickets 1500014631401               # tickets in a specific view
zd config set --default-view 1500014631401  # remember your usual queue
zd view tickets                             # ...then just run this
zd view tickets --status open,pending       # filter by status
zd view tickets --all                        # fetch every page (past 100)

# IDs or URLs are interchangeable — paste an interface URL anywhere an ID goes:
zd ticket show https://acme.zendesk.com/agent/tickets/12345
zd view tickets https://acme.zendesk.com/agent/filters/67890

# Write replies — visibility is REQUIRED and explicit:
zd reply 12345 --internal --body "Investigating, will update shortly."
zd reply 12345 --public   --body "Thanks for your patience — this is resolved."

# Long / multi-line bodies:
zd reply 12345 --public --file ./response.txt
cat response.txt | zd reply 12345 --public --stdin

# Structured output for scripts and agents:
zd ticket show 12345 --json
zd ticket comments 12345 --json
```

## Concepts

In Zendesk, a "reply" is a **ticket comment**:

| Comment       | Flag         | Visible to        |
|---------------|--------------|-------------------|
| Public reply  | `--public`   | requester + agents |
| Internal note | `--internal` | agents only        |

`zd reply` has **no default visibility**. You must pass exactly one of
`--public` or `--internal`, so a customer-facing message is never sent by
accident.

## Development

```sh
cargo build          # debug build
cargo build --release
cargo run -- docs    # print the built-in overview
```

Source layout:

- `src/main.rs`   — CLI definition (clap) and command dispatch
- `src/client.rs` — async Zendesk HTTP client
- `src/models.rs` — serde models for the API subset used
- `src/config.rs` — credential resolution (flags → env → file)
- `src/output.rs` — human vs. JSON output formatting
