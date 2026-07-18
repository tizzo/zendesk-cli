//! `zd` — a fast, self-documenting Zendesk ticket-reply CLI.
//!
//! Read and write both public replies (customer-facing) and internal notes
//! (agent-only) on Zendesk tickets. Every command supports `--json` for
//! machine-readable output.

mod client;
mod config;
mod idref;
mod keychain;
mod models;
mod output;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use client::ZendeskClient;
use config::{Config, Overrides};
use output::{print_json, Format};

/// Read and write Zendesk ticket replies — public and internal — from the terminal.
///
/// AUTH: set ZENDESK_SUBDOMAIN, ZENDESK_EMAIL, and ZENDESK_API_TOKEN, or run
/// `zd config set`. A Zendesk API token is created under Admin Center →
/// Apps and integrations → APIs → Zendesk API.
///
/// Add `--json` to any command for structured output that is easy to parse.
#[derive(Parser, Debug)]
#[command(name = "zd", version, about, long_about = None, propagate_version = true)]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,

    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Debug)]
struct GlobalArgs {
    /// Zendesk subdomain, i.e. the `acme` in `acme.zendesk.com`.
    #[arg(long, global = true, env = "ZENDESK_SUBDOMAIN")]
    subdomain: Option<String>,

    /// Agent email used for API-token auth.
    #[arg(long, global = true, env = "ZENDESK_EMAIL")]
    email: Option<String>,

    /// Zendesk API token.
    #[arg(long, global = true, env = "ZENDESK_API_TOKEN", hide_env_values = true)]
    api_token: Option<String>,

    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,
}

impl GlobalArgs {
    fn overrides(&self) -> Overrides {
        Overrides {
            subdomain: self.subdomain.clone(),
            email: self.email.clone(),
            api_token: self.api_token.clone(),
        }
    }
    fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else {
            Format::Human
        }
    }
    fn config(&self) -> Result<Config> {
        config::resolve(&self.overrides())
    }
    fn client(&self) -> Result<ZendeskClient> {
        ZendeskClient::new(&self.config()?)
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Verify credentials by fetching the authenticated user.
    Whoami,

    /// Inspect tickets: show, list, search, and read replies.
    #[command(subcommand)]
    Ticket(TicketCommand),

    /// Work with views (agent filters): list them, or fetch their tickets.
    #[command(subcommand)]
    View(ViewCommand),

    /// Post a reply to a ticket (public reply or internal note).
    Reply(ReplyArgs),

    /// Manage stored credentials (`~/.config/zendesk-cli/config.toml`).
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Print a concise overview of the tool for humans and agents.
    Docs,
}

#[derive(Subcommand, Debug)]
enum TicketCommand {
    /// Show a single ticket, including its description.
    Show {
        /// Ticket ID or any Zendesk ticket URL (e.g. .../agent/tickets/1162117).
        #[arg(value_parser = idref::parse_id)]
        id: i64,
    },
    /// List recent tickets (newest first).
    List {
        /// Maximum number of tickets to return (1-1000; paginates as needed).
        #[arg(long, default_value_t = 25, value_parser = clap::value_parser!(u32).range(1..=1000))]
        limit: u32,
        /// Filter by status (comma-separated): new,open,pending,hold,solved,closed.
        #[arg(long, value_delimiter = ',')]
        status: Vec<String>,
    },
    /// Search tickets using Zendesk search syntax, e.g. `status:open requester:a@b.com`.
    Search {
        /// Query terms (joined with spaces). `type:ticket` is added automatically.
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// Also filter results by status (comma-separated).
        #[arg(long, value_delimiter = ',')]
        status: Vec<String>,
    },
    /// List a ticket's replies, labeling each PUBLIC or INTERNAL.
    Comments {
        /// Ticket ID or any Zendesk ticket URL.
        #[arg(value_parser = idref::parse_id)]
        id: i64,
    },
}

#[derive(Subcommand, Debug)]
enum ViewCommand {
    /// List active views (agent filters) with their IDs and titles.
    List,
    /// Fetch the tickets in a view.
    ///
    /// The view ID is the trailing number in the agent-filter URL
    /// (`.../agent/filters/1500014631401`). Omit it to use the configured
    /// default view (`zd config set --default-view <ID>`).
    Tickets {
        /// View ID or agent-filter URL. Defaults to the configured default view.
        #[arg(value_parser = idref::parse_id)]
        id: Option<i64>,
        /// Maximum number of tickets to return (paginates as needed).
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=1000))]
        limit: u32,
        /// Fetch every ticket in the view (ignores --limit; follows all pages).
        #[arg(long)]
        all: bool,
        /// Filter by status (comma-separated): new,open,pending,hold,solved,closed.
        #[arg(long, value_delimiter = ',')]
        status: Vec<String>,
    },
}

#[derive(Args, Debug)]
struct ReplyArgs {
    /// Ticket ID or any Zendesk ticket URL to reply to.
    #[arg(value_parser = idref::parse_id)]
    id: i64,

    /// Reply text. Use `--file` or `--stdin` instead for long or multi-line bodies.
    #[arg(long, conflicts_with_all = ["file", "stdin"])]
    body: Option<String>,

    /// Read the reply body from a file.
    #[arg(long, conflicts_with_all = ["body", "stdin"])]
    file: Option<std::path::PathBuf>,

    /// Read the reply body from standard input.
    #[arg(long, conflicts_with_all = ["body", "file"])]
    stdin: bool,

    /// Post a PUBLIC reply, visible to the ticket requester (the customer).
    #[arg(long, group = "visibility")]
    public: bool,

    /// Post an INTERNAL note, visible only to agents.
    #[arg(long, group = "visibility")]
    internal: bool,
}

#[derive(Subcommand, Debug)]
enum ConfigCommand {
    /// Print the path to the config file.
    Path,
    /// Show the resolved configuration (API token is masked).
    Show,
    /// Save credentials: subdomain/email to the config file, API token to the OS keychain.
    Set {
        #[arg(long)]
        subdomain: Option<String>,
        #[arg(long)]
        email: Option<String>,
        /// API token — stored securely in the OS keychain, never written to disk in plaintext.
        #[arg(long, hide_env_values = true)]
        api_token: Option<String>,
        /// Default view (agent filter) ID used by `zd view tickets` with no argument.
        #[arg(long)]
        default_view: Option<i64>,
    },
    /// Remove the API token from the OS keychain for the current subdomain.
    ClearToken,
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let g = &cli.global;

    match &cli.command {
        Command::Whoami => {
            let user = g.client()?.whoami().await?;
            emit(g, &user, output::user_human)?;
        }

        Command::Ticket(cmd) => match cmd {
            TicketCommand::Show { id } => {
                let ticket = g.client()?.get_ticket(*id).await?;
                emit(g, &ticket, output::ticket_human)?;
            }
            TicketCommand::List { limit, status } => {
                let statuses = validate_statuses(status)?;
                let list = g.client()?.list_tickets(*limit as usize, &statuses).await?;
                emit_ticket_list(g, "Tickets", &list, &statuses)?;
            }
            TicketCommand::Search { query, status } => {
                let statuses = validate_statuses(status)?;
                let list = g
                    .client()?
                    .search_tickets(&query.join(" "), 100, &statuses)
                    .await?;
                emit_ticket_list(g, "Search", &list, &statuses)?;
            }
            TicketCommand::Comments { id } => {
                let comments = g.client()?.list_comments(*id).await?;
                emit(g, &comments, |c| output::comments_human(*id, c))?;
            }
        },

        Command::View(cmd) => match cmd {
            ViewCommand::List => {
                let views = g.client()?.list_views().await?;
                emit(g, &views, |v| output::views_table(v))?;
            }
            ViewCommand::Tickets {
                id,
                limit,
                all,
                status,
            } => {
                let view_id = resolve_view_id(*id)?;
                let statuses = validate_statuses(status)?;
                let cap = if *all { None } else { Some(*limit as usize) };
                let list = g
                    .client()?
                    .list_view_tickets(view_id, cap, &statuses)
                    .await?;
                emit_ticket_list(g, &format!("View {view_id}"), &list, &statuses)?;
            }
        },

        Command::Reply(args) => reply(g, args).await?,

        Command::Config(cmd) => config_cmd(g, cmd)?,

        Command::Docs => print!("{}", DOCS),
    }

    Ok(())
}

async fn reply(g: &GlobalArgs, args: &ReplyArgs) -> Result<()> {
    // Enforce an explicit visibility choice so a public reply is never accidental.
    let public = match (args.public, args.internal) {
        (true, false) => true,
        (false, true) => false,
        _ => anyhow::bail!(
            "choose exactly one visibility: --public (customer-facing) or --internal (agent-only)"
        ),
    };

    let body = resolve_body(args)?;
    if body.trim().is_empty() {
        anyhow::bail!("reply body is empty");
    }

    let ticket = g.client()?.add_comment(args.id, &body, public).await?;

    match g.format() {
        Format::Json => print_json(&serde_json::json!({
            "ticket_id": ticket.id,
            "public": public,
            "status": ticket.status,
            "posted": true,
        }))?,
        Format::Human => {
            let kind = if public {
                "public reply"
            } else {
                "internal note"
            };
            println!("Posted {kind} to ticket #{}.", ticket.id);
        }
    }
    Ok(())
}

fn resolve_body(args: &ReplyArgs) -> Result<String> {
    if let Some(b) = &args.body {
        return Ok(b.clone());
    }
    if let Some(path) = &args.file {
        return std::fs::read_to_string(path)
            .with_context(|| format!("reading reply body from {}", path.display()));
    }
    if args.stdin {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("reading reply body from stdin")?;
        return Ok(buf);
    }
    anyhow::bail!("provide the reply body with --body, --file, or --stdin")
}

const VALID_STATUSES: [&str; 6] = ["new", "open", "pending", "hold", "solved", "closed"];

/// Normalize and validate `--status` values against the known Zendesk statuses.
fn validate_statuses(input: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for s in input {
        let low = s.trim().to_lowercase();
        if low.is_empty() {
            continue;
        }
        if !VALID_STATUSES.contains(&low.as_str()) {
            anyhow::bail!(
                "invalid status '{s}'. Valid statuses: {}",
                VALID_STATUSES.join(", ")
            );
        }
        out.push(low);
    }
    Ok(out)
}

/// Render `value` honoring the global `--json` flag: as pretty JSON, or via the
/// supplied human-formatting function. Centralizes the format decision so every
/// command respects `--json` without repeating the match.
fn emit<T: serde::Serialize>(g: &GlobalArgs, value: &T, human: impl FnOnce(&T)) -> Result<()> {
    match g.format() {
        Format::Json => print_json(value)?,
        Format::Human => human(value),
    }
    Ok(())
}

/// Render a [`client::TicketList`] as JSON (object with count metadata) or a
/// human table with a summary line.
fn emit_ticket_list(
    g: &GlobalArgs,
    context: &str,
    list: &client::TicketList,
    statuses: &[String],
) -> Result<()> {
    match g.format() {
        Format::Json => print_json(&serde_json::json!({
            "context": context,
            "total": list.total,
            "shown": list.tickets.len(),
            "status_filter": statuses,
            "tickets": list.tickets,
        }))?,
        Format::Human => {
            let filter = if statuses.is_empty() {
                String::new()
            } else {
                format!("  (status: {})", statuses.join(","))
            };
            match list.total {
                Some(t) => println!(
                    "{context} — showing {} of {} total{filter}",
                    list.tickets.len(),
                    t
                ),
                None => println!("{context} — {} ticket(s){filter}", list.tickets.len()),
            }
            output::tickets_table(&list.tickets);
        }
    }
    Ok(())
}

/// Resolve the view ID from an explicit argument, falling back to the
/// configured default view.
fn resolve_view_id(arg: Option<i64>) -> Result<i64> {
    if let Some(id) = arg {
        return Ok(id);
    }
    match config::default_view()? {
        Some(id) => Ok(id),
        None => anyhow::bail!(
            "no view ID given and no default view set. Pass an ID (the number in \
             .../agent/filters/<ID>), or run `zd config set --default-view <ID>`. \
             List views with `zd view list`."
        ),
    }
}

fn config_cmd(g: &GlobalArgs, cmd: &ConfigCommand) -> Result<()> {
    match cmd {
        ConfigCommand::Path => println!("{}", config::config_path()?.display()),
        ConfigCommand::Show => {
            let (cfg, source) = config::resolve_with_source(&g.overrides())?;
            let masked = mask(&cfg.api_token);
            match g.format() {
                Format::Json => print_json(&serde_json::json!({
                    "subdomain": cfg.subdomain,
                    "email": cfg.email,
                    "api_token": masked,
                    "api_token_source": source.label(),
                    "default_view": cfg.default_view,
                    "base_url": cfg.base_url(),
                }))?,
                Format::Human => {
                    println!("subdomain   : {}", cfg.subdomain);
                    println!("email       : {}", cfg.email);
                    println!("api_token   : {masked}  (from {})", source.label());
                    println!(
                        "default_view: {}",
                        cfg.default_view
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "-".into())
                    );
                    println!("base_url    : {}", cfg.base_url());
                    if source == config::TokenSource::LegacyFile {
                        println!(
                            "\nwarning: token is stored in plaintext in the config file. \
                             Run `zd config set --api-token <TOKEN>` to migrate it to the keychain."
                        );
                    }
                }
            }
        }
        ConfigCommand::Set {
            subdomain,
            email,
            api_token,
            default_view,
        } => {
            if subdomain.is_none()
                && email.is_none()
                && api_token.is_none()
                && default_view.is_none()
            {
                anyhow::bail!(
                    "nothing to set; pass --subdomain, --email, --api-token, and/or --default-view"
                );
            }

            let mut messages = Vec::new();

            // Store the token in the keychain (keyed by subdomain), and clear any
            // legacy plaintext token from the config file in the same pass.
            let clear_legacy = if let Some(token) = api_token {
                let sub = subdomain
                    .clone()
                    .map(Ok)
                    .unwrap_or_else(|| config::resolve_subdomain(&g.overrides()))?;
                keychain::store_token(&sub, token)?;
                messages.push(format!(
                    "Stored API token in the OS keychain (subdomain '{sub}')."
                ));
                true
            } else {
                false
            };

            if subdomain.is_some() || email.is_some() || default_view.is_some() || clear_legacy {
                let path = config::save_nonsecret(
                    subdomain.clone(),
                    email.clone(),
                    *default_view,
                    clear_legacy,
                )?;
                messages.push(format!("Saved settings to {}", path.display()));
            }

            for m in messages {
                println!("{m}");
            }
        }
        ConfigCommand::ClearToken => {
            let sub = config::resolve_subdomain(&g.overrides())?;
            if keychain::delete_token(&sub)? {
                println!("Removed API token from the keychain for subdomain '{sub}'.");
            } else {
                println!("No API token was stored in the keychain for subdomain '{sub}'.");
            }
        }
    }
    Ok(())
}

fn mask(token: &str) -> String {
    let n = token.chars().count();
    if n <= 4 {
        "****".into()
    } else {
        let tail: String = token.chars().skip(n - 4).collect();
        format!("****{tail}")
    }
}

const DOCS: &str = r#"zd — Zendesk ticket-reply CLI
==============================

PURPOSE
  Read and write Zendesk ticket replies. A Zendesk "reply" is a ticket comment:
    - public  => visible to the requester (the customer)
    - internal => an agent-only note

AUTH (token priority: flag > env > OS keychain > legacy file)
  Env  : ZENDESK_SUBDOMAIN, ZENDESK_EMAIL, ZENDESK_API_TOKEN
  Saved: `zd config set --subdomain acme --email you@co.com --api-token XXXX`
         subdomain/email -> config file; API token -> OS keychain (secure).
  Clear: `zd config clear-token` removes the token from the keychain.

COMMON COMMANDS
  zd whoami                         Verify credentials.
  zd ticket show 12345              Show a ticket.
  zd ticket list --limit 20         List recent tickets.
  zd ticket search status:open      Search (Zendesk query syntax).
  zd ticket comments 12345          Read all replies (labeled PUBLIC/INTERNAL).
  zd view list                      List views (agent filters) with IDs.
  zd view tickets 1500014631401     Tickets in a view (agent filter).
  zd view tickets                   Tickets in the configured default view.
  zd view tickets --status open     Filter by status; --all fetches every page.

IDs OR URLS
  Anywhere a ticket/view ID is expected you may paste the interface URL instead,
  e.g. `zd ticket show https://acme.zendesk.com/agent/tickets/12345` or
  `zd view tickets https://acme.zendesk.com/agent/filters/67890`.

STATUS FILTER
  --status accepts a comma-separated list: new,open,pending,hold,solved,closed.
  zd reply 12345 --internal --body "Looking into this."
  zd reply 12345 --public   --body "Thanks — fixed now!"
  echo "long text" | zd reply 12345 --public --stdin

MACHINE-READABLE OUTPUT
  Add --json to any command for structured output.

SAFETY
  `zd reply` requires an explicit --public or --internal; there is no default,
  so a customer-facing reply is never sent by accident.
"#;
