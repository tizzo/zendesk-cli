//! Parse a resource ID from either a bare number or a Zendesk URL.
//!
//! The agent interface uses URLs like:
//!   - ticket:  `https://acme.zendesk.com/agent/tickets/1162117`
//!   - view:    `https://acme.zendesk.com/agent/filters/1500014631401`
//!   - user:    `https://acme.zendesk.com/agent/users/12345`
//!
//! Rather than special-casing each, we extract the last numeric path segment,
//! which is the resource ID for every such URL. Bare numeric input is accepted
//! as-is, so `1162117` and the full URL are interchangeable anywhere an ID is
//! expected.

/// clap value parser: accept a numeric ID or a Zendesk URL and return the ID.
pub fn parse_id(input: &str) -> Result<i64, String> {
    let s = input.trim();

    // Fast path: a bare integer.
    if let Ok(n) = s.parse::<i64>() {
        return Ok(n);
    }

    // Otherwise treat it as a URL/path: drop any query string or fragment,
    // then take the last numeric path segment (handles trailing slashes and
    // sub-paths like `/agent/tickets/123/events`).
    let path = s.split(['?', '#']).next().unwrap_or(s);
    if let Some(id) = path.rsplit('/').find_map(|seg| seg.parse::<i64>().ok()) {
        return Ok(id);
    }

    Err(format!(
        "'{input}' is not a numeric ID or a recognizable Zendesk URL \
         (e.g. https://<subdomain>.zendesk.com/agent/tickets/12345)"
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_id;

    #[test]
    fn bare_number() {
        assert_eq!(parse_id("1162117").unwrap(), 1162117);
        assert_eq!(parse_id("  42 ").unwrap(), 42);
    }

    #[test]
    fn ticket_url() {
        assert_eq!(
            parse_id("https://privacy.zendesk.com/agent/tickets/1162117").unwrap(),
            1162117
        );
    }

    #[test]
    fn view_url() {
        assert_eq!(
            parse_id("https://privacy.zendesk.com/agent/filters/1500014631401").unwrap(),
            1500014631401
        );
    }

    #[test]
    fn url_with_subpath_and_query() {
        assert_eq!(
            parse_id("https://privacy.zendesk.com/agent/tickets/1162117/events?foo=bar").unwrap(),
            1162117
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_id("not-a-ticket").is_err());
    }
}
