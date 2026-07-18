//! Thin async HTTP client around the Zendesk Ticketing API.

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::config::Config;
use crate::models::*;

pub struct ZendeskClient {
    http: Client,
    base_url: String,
    auth_header: String,
}

/// A page-spanning ticket result plus the view/query's total count.
pub struct TicketList {
    pub tickets: Vec<Ticket>,
    /// Total tickets matching (across all pages), if the API reported it.
    pub total: Option<i64>,
}

/// True if the ticket's status matches any of `statuses` (case-insensitive).
/// An empty filter matches everything.
fn status_matches(ticket: &Ticket, statuses: &[String]) -> bool {
    statuses.is_empty()
        || ticket
            .status
            .as_deref()
            .is_some_and(|s| statuses.iter().any(|w| w.eq_ignore_ascii_case(s)))
}

impl ZendeskClient {
    pub fn new(config: &Config) -> Result<Self> {
        let http = Client::builder()
            .user_agent(concat!("zendesk-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("building HTTP client")?;

        let creds = format!("{}:{}", config.basic_auth_user(), config.api_token);
        let encoded = base64::engine::general_purpose::STANDARD.encode(creds);
        let auth_header = format!("Basic {encoded}");

        Ok(Self {
            http,
            base_url: config.base_url(),
            auth_header,
        })
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        self.http
            .request(method, url)
            .header(reqwest::header::AUTHORIZATION, &self.auth_header)
            .header(reqwest::header::ACCEPT, "application/json")
    }

    /// GET a relative path (e.g. `/tickets.json`) or an absolute URL (used for
    /// following the `next_page` links returned by paginated endpoints).
    fn get(&self, path_or_url: &str) -> RequestBuilder {
        if path_or_url.starts_with("http") {
            self.http
                .get(path_or_url)
                .header(reqwest::header::AUTHORIZATION, &self.auth_header)
                .header(reqwest::header::ACCEPT, "application/json")
        } else {
            self.request(Method::GET, path_or_url)
        }
    }

    /// Send a request and deserialize a successful JSON body, turning Zendesk
    /// error responses into readable errors.
    async fn send<T: DeserializeOwned>(&self, req: RequestBuilder) -> Result<T> {
        let resp = req.send().await.context("sending request to Zendesk")?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if status.is_success() {
            return serde_json::from_str::<T>(&text)
                .with_context(|| format!("decoding Zendesk response body: {text}"));
        }

        let hint = match status {
            StatusCode::UNAUTHORIZED => " (check email + API token; username must be `email/token`)",
            StatusCode::FORBIDDEN => " (token or user lacks permission for this action)",
            StatusCode::NOT_FOUND => " (ticket or resource not found)",
            StatusCode::TOO_MANY_REQUESTS => " (rate limited; retry later)",
            _ => "",
        };
        Err(anyhow!("Zendesk API error {status}{hint}: {text}"))
    }

    /// `GET /tickets/{id}.json`
    pub async fn get_ticket(&self, id: i64) -> Result<Ticket> {
        let resp: TicketResponse = self
            .send(self.request(Method::GET, &format!("/tickets/{id}.json")))
            .await?;
        Ok(resp.ticket)
    }

    /// `GET /tickets/{id}/comments.json` — the ticket's replies (public + internal).
    pub async fn list_comments(&self, id: i64) -> Result<Vec<Comment>> {
        let resp: CommentsResponse = self
            .send(self.request(Method::GET, &format!("/tickets/{id}/comments.json")))
            .await?;
        Ok(resp.comments)
    }

    /// List recent tickets (newest first), following pagination up to `limit`.
    pub async fn list_tickets(&self, limit: usize, statuses: &[String]) -> Result<TicketList> {
        let path = "/tickets.json?sort_by=created_at&sort_order=desc&per_page=100".to_string();
        self.collect_tickets(path, Some(limit), statuses).await
    }

    /// Tickets in a view (agent filter). `GET /views/{id}/tickets.json`.
    ///
    /// `limit == None` fetches every page; otherwise pagination stops once
    /// `limit` matching tickets are collected. `statuses` filters client-side.
    pub async fn list_view_tickets(
        &self,
        view_id: i64,
        limit: Option<usize>,
        statuses: &[String],
    ) -> Result<TicketList> {
        let path = format!("/views/{view_id}/tickets.json?per_page=100");
        self.collect_tickets(path, limit, statuses).await
    }

    /// Search tickets using Zendesk search syntax. `GET /search.json?query=...`
    ///
    /// The Search API returns results under `results` (not `tickets`).
    pub async fn search_tickets(
        &self,
        query: &str,
        limit: usize,
        statuses: &[String],
    ) -> Result<TicketList> {
        let full_query = format!("type:ticket {query}");
        let path = format!("/search.json?query={}&per_page=100", url_encode(&full_query));
        let resp: SearchResponse = self.send(self.get(&path)).await?;
        let mut tickets: Vec<Ticket> = resp
            .results
            .into_iter()
            .filter(|t| status_matches(t, statuses))
            .collect();
        tickets.truncate(limit);
        Ok(TicketList {
            tickets,
            total: resp.count,
        })
    }

    /// Fetch tickets across pages from `first_path`, applying an optional status
    /// filter, stopping once `limit` matching tickets are collected (or after
    /// exhausting pages / a safety cap when `limit` is `None`).
    async fn collect_tickets(
        &self,
        first_path: String,
        limit: Option<usize>,
        statuses: &[String],
    ) -> Result<TicketList> {
        // Safety cap so a runaway `next_page` chain can't loop forever.
        const MAX_PAGES: usize = 100;

        let mut url = first_path;
        let mut collected: Vec<Ticket> = Vec::new();
        let mut total: Option<i64> = None;
        let mut pages = 0usize;

        loop {
            let resp: TicketsResponse = self.send(self.get(&url)).await?;
            if total.is_none() {
                total = resp.count;
            }
            for t in resp.tickets {
                if status_matches(&t, statuses) {
                    collected.push(t);
                }
            }
            pages += 1;

            if let Some(l) = limit {
                if collected.len() >= l {
                    collected.truncate(l);
                    break;
                }
            }
            match resp.next_page {
                Some(next) if pages < MAX_PAGES => url = next,
                _ => break,
            }
        }

        Ok(TicketList {
            tickets: collected,
            total,
        })
    }

    /// Add a reply (comment) to a ticket via `PUT /tickets/{id}.json`.
    ///
    /// `public = true` posts a public reply to the requester; `public = false`
    /// posts an internal note visible only to agents.
    pub async fn add_comment(&self, id: i64, body: &str, public: bool) -> Result<Ticket> {
        let payload = json!({
            "ticket": {
                "comment": {
                    "body": body,
                    "public": public
                }
            }
        });
        let req = self
            .request(Method::PUT, &format!("/tickets/{id}.json"))
            .json(&payload);
        let resp: TicketResponse = self.send(req).await?;
        Ok(resp.ticket)
    }

    /// List views (agent filters). `GET /views.json`
    pub async fn list_views(&self) -> Result<Vec<View>> {
        let resp: ViewsResponse = self
            .send(self.request(Method::GET, "/views.json?active=true"))
            .await?;
        Ok(resp.views)
    }

    /// `GET /users/me.json` — verify credentials.
    pub async fn whoami(&self) -> Result<User> {
        let resp: UserResponse = self
            .send(self.request(Method::GET, "/users/me.json"))
            .await?;
        Ok(resp.user)
    }
}

/// Minimal percent-encoding for query values (spaces, reserved chars).
fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
