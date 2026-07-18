//! Serde models for the subset of the Zendesk Ticketing API this CLI uses.
//!
//! These intentionally cover only the fields we read or write. Zendesk returns
//! many more fields; unknown fields are ignored during deserialization.

use serde::{Deserialize, Serialize};

/// A Zendesk ticket. See <https://developer.zendesk.com/api-reference/ticketing/tickets/tickets/>.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: i64,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub requester_id: Option<i64>,
    #[serde(default)]
    pub assignee_id: Option<i64>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Wrapper for `GET /tickets/{id}.json`.
#[derive(Debug, Deserialize)]
pub struct TicketResponse {
    pub ticket: Ticket,
}

/// Wrapper for list endpoints returning multiple tickets.
#[derive(Debug, Deserialize)]
pub struct TicketsResponse {
    pub tickets: Vec<Ticket>,
    /// Full URL of the next page (offset pagination), or `None` on the last page.
    #[serde(default)]
    pub next_page: Option<String>,
    /// Total number of tickets matching (across all pages).
    #[serde(default)]
    pub count: Option<i64>,
}

/// Wrapper for `GET /search.json` — the Search API returns `results`, not `tickets`.
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub results: Vec<Ticket>,
    #[serde(default)]
    #[allow(dead_code)]
    pub next_page: Option<String>,
    #[serde(default)]
    pub count: Option<i64>,
}

/// A comment on a ticket. In Zendesk, a "reply" is a comment.
///
/// - `public == true`  => a public reply visible to the requester (the customer).
/// - `public == false` => an internal note visible only to agents.
///
/// See <https://developer.zendesk.com/api-reference/ticketing/tickets/ticket_comments/>.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: i64,
    #[serde(default)]
    pub author_id: Option<i64>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_body: Option<String>,
    #[serde(default)]
    pub public: bool,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Wrapper for `GET /tickets/{id}/comments.json`.
#[derive(Debug, Deserialize)]
pub struct CommentsResponse {
    pub comments: Vec<Comment>,
    #[serde(default)]
    #[allow(dead_code)]
    pub next_page: Option<String>,
}

/// A Zendesk user (used by `whoami`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

/// Wrapper for `GET /users/me.json`.
#[derive(Debug, Deserialize)]
pub struct UserResponse {
    pub user: User,
}

/// A Zendesk view (called an "agent filter" in the agent UI). The numeric ID
/// is the trailing segment of `.../agent/filters/{id}`.
///
/// See <https://developer.zendesk.com/api-reference/ticketing/business-rules/views/>.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub id: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub active: bool,
}

/// Wrapper for `GET /views.json`.
#[derive(Debug, Deserialize)]
pub struct ViewsResponse {
    pub views: Vec<View>,
    #[serde(default)]
    #[allow(dead_code)]
    pub next_page: Option<String>,
}
