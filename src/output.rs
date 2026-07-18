//! Output formatting. Every command supports `--json` for machine-readable
//! output (agent-friendly) and a compact human-readable default.

use anyhow::Result;
use serde::Serialize;

use crate::models::{Comment, Ticket, User, View};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Human,
    Json,
}

/// Print any serializable value as pretty JSON to stdout.
pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

pub fn ticket_human(t: &Ticket) {
    println!("Ticket #{}", t.id);
    if let Some(s) = &t.subject {
        println!("  Subject : {s}");
    }
    println!("  Status  : {}", t.status.as_deref().unwrap_or("-"));
    println!("  Priority: {}", t.priority.as_deref().unwrap_or("-"));
    if let Some(r) = t.requester_id {
        println!("  Requester: {r}");
    }
    if let Some(a) = t.assignee_id {
        println!("  Assignee : {a}");
    }
    if !t.tags.is_empty() {
        println!("  Tags    : {}", t.tags.join(", "));
    }
    if let Some(c) = &t.created_at {
        println!("  Created : {c}");
    }
    if let Some(u) = &t.updated_at {
        println!("  Updated : {u}");
    }
    if let Some(desc) = t.description.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        println!("\n  Description:");
        for line in desc.lines() {
            println!("    {line}");
        }
    }
}

pub fn tickets_table(tickets: &[Ticket]) {
    if tickets.is_empty() {
        println!("(no tickets)");
        return;
    }
    println!("{:<8}  {:<10}  {:<8}  SUBJECT", "ID", "STATUS", "PRIORITY");
    for t in tickets {
        let subject = t.subject.as_deref().unwrap_or("");
        let subject = truncate(subject, 60);
        println!(
            "{:<8}  {:<10}  {:<8}  {}",
            t.id,
            t.status.as_deref().unwrap_or("-"),
            t.priority.as_deref().unwrap_or("-"),
            subject
        );
    }
}

pub fn views_table(views: &[View]) {
    if views.is_empty() {
        println!("(no views)");
        return;
    }
    println!("{:<16}  TITLE", "ID");
    for v in views {
        println!("{:<16}  {}", v.id, v.title.as_deref().unwrap_or("-"));
    }
}

pub fn comments_human(ticket_id: i64, comments: &[Comment]) {
    println!("Ticket #{ticket_id} — {} comment(s)\n", comments.len());
    for c in comments {
        let kind = if c.public { "PUBLIC " } else { "INTERNAL" };
        let when = c.created_at.as_deref().unwrap_or("");
        let author = c
            .author_id
            .map(|a| a.to_string())
            .unwrap_or_else(|| "?".into());
        println!("[{kind}] #{}  author={author}  {when}", c.id);
        if let Some(body) = &c.body {
            for line in body.lines() {
                println!("    {line}");
            }
        }
        println!();
    }
}

pub fn user_human(u: &User) {
    println!("Authenticated as:");
    println!("  id   : {}", u.id);
    println!("  name : {}", u.name.as_deref().unwrap_or("-"));
    println!("  email: {}", u.email.as_deref().unwrap_or("-"));
    println!("  role : {}", u.role.as_deref().unwrap_or("-"));
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
