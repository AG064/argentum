// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Page and search requests made for the agent by the web modules.
//!
//! The runtime has already checked the policy and the owner's approval for the
//! host before anything here runs. These functions add the network rules that
//! hold for every request: http or https only, no credentials in the address,
//! no cookies, no proxy, redirects only within the approved host, and no
//! loopback or private addresses unless the owner allowed them. The address a
//! host resolves to is checked and then pinned for the request, so a second
//! lookup cannot swap in another address.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use url::Url;

use crate::parse_html;

const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SEARCH_BYTES: usize = 512 * 1024;
/// Text returned to the model for one page.
pub const MAX_AGENT_TEXT_BYTES: usize = 48 * 1024;
const MAX_REDIRECTS: usize = 5;
const MAX_SEARCH_RESULTS: usize = 10;
const MAX_SNIPPET_CHARS: usize = 400;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = concat!("Argentum-Agent/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentFetchOptions {
    /// Owner setting. Allows loopback, private and link-local addresses.
    pub allow_local_addresses: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPage {
    /// Address the text came from, after same-host redirects.
    pub url: String,
    pub title: String,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentFetchOutcome {
    Page(AgentPage),
    /// The page moved to another host. That host needs its own approval.
    RedirectedOffHost {
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFetchError(pub String);

impl std::fmt::Display for AgentFetchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn error(message: impl Into<String>) -> AgentFetchError {
    AgentFetchError(message.into())
}

/// Parse and check an address the agent asked for. Returns the URL and its
/// host in lower case, which is what an approval covers.
pub fn agent_address(input: &str) -> Result<(Url, String), AgentFetchError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 || trimmed.chars().any(char::is_control) {
        return Err(error(
            "the address is empty, too long, or has control characters",
        ));
    }
    let mut url = Url::parse(trimmed).map_err(|_| error("the address is not a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(error("only http and https addresses can be read"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(error("addresses with credentials are not read"));
    }
    url.set_fragment(None);
    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| error("the address has no host"))?
        .to_ascii_lowercase();
    Ok((url, host))
}

/// Whether an address is reachable on the public internet.
pub fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let segments = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || v6 == Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0, 0))
        }
    }
}

fn is_public_v4(v4: Ipv4Addr) -> bool {
    let [a, b, c, _] = v4.octets();
    !(v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
        || v4.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (18..20).contains(&b))
        || a >= 240)
}

async fn pinned_addresses(
    url: &Url,
    options: AgentFetchOptions,
) -> Result<Vec<SocketAddr>, AgentFetchError> {
    let host = url
        .host_str()
        .ok_or_else(|| error("the address has no host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| error("the address has no port"))?;
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let addresses: Vec<SocketAddr> = match bare.parse::<IpAddr>() {
        Ok(address) => vec![SocketAddr::new(address, port)],
        Err(_) => tokio::net::lookup_host((bare, port))
            .await
            .map_err(|_| error("the host name could not be resolved"))?
            .collect(),
    };
    if addresses.is_empty() {
        return Err(error("the host name could not be resolved"));
    }
    if !options.allow_local_addresses
        && addresses
            .iter()
            .any(|address| !is_public_address(address.ip()))
    {
        return Err(error(
            "the host is or resolves to a local or private address; the owner can allow local addresses in the module settings",
        ));
    }
    Ok(addresses)
}

async fn send(
    url: &Url,
    options: AgentFetchOptions,
    accept: &str,
) -> Result<reqwest::Response, AgentFetchError> {
    let addresses = pinned_addresses(url, options).await?;
    let host = url.host_str().unwrap_or_default();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .resolve_to_addrs(
            host.trim_start_matches('[').trim_end_matches(']'),
            &addresses,
        )
        .build()
        .map_err(|_| error("the request could not be prepared"))?;
    client
        .get(url.clone())
        .header(reqwest::header::ACCEPT, accept)
        .send()
        .await
        .map_err(|failure| {
            if failure.is_timeout() {
                error("the server took too long to answer")
            } else {
                error("the server could not be reached")
            }
        })
}

async fn read_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<(Vec<u8>, bool), AgentFetchError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| error("the response stopped before it finished"))?
    {
        let room = limit.saturating_sub(body.len());
        if chunk.len() > room {
            body.extend_from_slice(&chunk[..room]);
            return Ok((body, true));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, false))
}

/// Read one page as text for the agent.
pub async fn fetch_for_agent(
    address: &str,
    options: AgentFetchOptions,
) -> Result<AgentFetchOutcome, AgentFetchError> {
    let (mut url, approved_host) = agent_address(address)?;
    let mut response = None;
    for _ in 0..=MAX_REDIRECTS {
        let current = send(&url, options, "text/html, text/plain;q=0.9, */*;q=0.1").await?;
        if !current.status().is_redirection() {
            response = Some(current);
            break;
        }
        let location = current
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| error("the server redirected without an address"))?;
        let next = url
            .join(location)
            .map_err(|_| error("the server redirected to an invalid address"))?;
        let (next, next_host) = agent_address(next.as_str())?;
        if next_host != approved_host {
            return Ok(AgentFetchOutcome::RedirectedOffHost {
                from: url.to_string(),
                to: next.to_string(),
            });
        }
        url = next;
    }
    let response = response.ok_or_else(|| error("the server redirected too many times"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(error(format!(
            "the server answered with status {}",
            status.as_u16()
        )));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("text/html")
        .to_ascii_lowercase();
    let is_html = content_type.contains("html");
    if !is_html && !content_type.starts_with("text/") && !content_type.contains("json") {
        return Err(error(format!(
            "the page is {content_type}, and only text pages can be read"
        )));
    }
    let (body, cut) = read_body(response, MAX_PAGE_BYTES).await?;
    let text = String::from_utf8_lossy(&body);
    let (title, text, parse_cut) = if is_html {
        let (title, blocks, cut) = parse_html(&text, &url);
        let joined = blocks
            .iter()
            .map(|block| block.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        (title, joined, cut)
    } else {
        let joined: String = text.chars().take(MAX_PAGE_BYTES).collect();
        (String::new(), joined, false)
    };
    let (text, text_cut) = cut_utf8(text, MAX_AGENT_TEXT_BYTES);
    Ok(AgentFetchOutcome::Page(AgentPage {
        url: url.to_string(),
        title,
        text,
        truncated: cut || parse_cut || text_cut,
    }))
}

/// Query a SearXNG-compatible search address for the agent.
pub async fn search_for_agent(
    search_address: &str,
    query: &str,
    options: AgentFetchOptions,
) -> Result<Vec<SearchResult>, AgentFetchError> {
    let (mut url, _) = agent_address(search_address)?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("format", "json");
    let response = send(&url, options, "application/json").await?;
    let status = response.status();
    if !status.is_success() {
        return Err(error(format!(
            "the search server answered with status {}",
            status.as_u16()
        )));
    }
    let (body, cut) = read_body(response, MAX_SEARCH_BYTES).await?;
    if cut {
        return Err(error("the search answer was too large"));
    }
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| error("the search server did not answer with JSON"))?;
    let results = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| error("the search answer has no results list"))?;
    let text = |item: &serde_json::Value, key: &str, limit: usize| {
        item.get(key)
            .and_then(serde_json::Value::as_str)
            .map(|text| {
                text.chars()
                    .filter(|character| !character.is_control() || *character == ' ')
                    .take(limit)
                    .collect::<String>()
                    .trim()
                    .to_owned()
            })
            .unwrap_or_default()
    };
    Ok(results
        .iter()
        .filter_map(|item| {
            let url = text(item, "url", 2048);
            let (url, _) = agent_address(&url).ok()?;
            Some(SearchResult {
                title: text(item, "title", 200),
                url: url.to_string(),
                snippet: text(item, "content", MAX_SNIPPET_CHARS),
            })
        })
        .take(MAX_SEARCH_RESULTS)
        .collect())
}

fn cut_utf8(mut text: String, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    (text, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const LOCAL: AgentFetchOptions = AgentFetchOptions {
        allow_local_addresses: true,
    };

    /// A fake server on 127.0.0.1 that answers each request from `answer`,
    /// given the request line.
    async fn serve(answer: fn(&str, u16) -> String) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 8192];
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                    let line = request.lines().next().unwrap_or_default().to_owned();
                    let response = answer(&line, port);
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        port
    }

    fn http(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn addresses_are_checked() {
        assert_eq!(
            agent_address("https://Example.COM/a#frag").expect("ok").1,
            "example.com"
        );
        assert!(agent_address("https://Example.COM/a#frag")
            .expect("ok")
            .0
            .fragment()
            .is_none());
        for refused in [
            "file:///etc/passwd",
            "ftp://example.com",
            "https://user:pw@example.com",
            "example.com",
            "",
        ] {
            assert!(agent_address(refused).is_err(), "{refused} was accepted");
        }
    }

    #[test]
    fn local_and_private_addresses_are_not_public() {
        for local in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(
                !is_public_address(local.parse().expect("ip")),
                "{local} counted as public"
            );
        }
        for public in ["93.184.216.34", "2606:4700::1111"] {
            assert!(is_public_address(public.parse().expect("ip")));
        }
    }

    #[tokio::test]
    async fn local_addresses_are_refused_by_default() {
        let port = serve(|_, _| http("200 OK", "Content-Type: text/plain\r\n", "hi")).await;
        let refused = fetch_for_agent(
            &format!("http://127.0.0.1:{port}/"),
            AgentFetchOptions::default(),
        )
        .await
        .expect_err("refused");
        assert!(refused.0.contains("local"));
        let refused = fetch_for_agent(
            &format!("http://localhost:{port}/"),
            AgentFetchOptions::default(),
        )
        .await
        .expect_err("refused");
        assert!(refused.0.contains("local"));
    }

    #[tokio::test]
    async fn reads_html_as_text() {
        let port = serve(|_, _| {
            http(
                "200 OK",
                "Content-Type: text/html; charset=utf-8\r\nSet-Cookie: a=b\r\n",
                "<html><head><title>Docs</title><script>alert(1)</script></head><body><h1>Hello</h1><p>World &amp; more</p></body></html>",
            )
        })
        .await;
        let AgentFetchOutcome::Page(page) =
            fetch_for_agent(&format!("http://127.0.0.1:{port}/page"), LOCAL)
                .await
                .expect("page")
        else {
            panic!("expected a page");
        };
        assert_eq!(page.title, "Docs");
        assert!(page.text.contains("Hello"));
        assert!(page.text.contains("World & more"));
        assert!(!page.text.contains("alert"));
        assert!(!page.truncated);
    }

    #[tokio::test]
    async fn follows_redirects_only_on_the_same_host() {
        let port = serve(|line, port| {
            if line.starts_with("GET /old ") {
                http("302 Found", "Location: /new\r\n", "")
            } else if line.starts_with("GET /away ") {
                http(
                    "301 Moved",
                    &format!("Location: http://localhost:{port}/new\r\n"),
                    "",
                )
            } else {
                http("200 OK", "Content-Type: text/plain\r\n", "moved here")
            }
        })
        .await;
        let AgentFetchOutcome::Page(page) =
            fetch_for_agent(&format!("http://127.0.0.1:{port}/old"), LOCAL)
                .await
                .expect("page")
        else {
            panic!("expected a page");
        };
        assert!(page.url.ends_with("/new"));
        assert_eq!(page.text, "moved here");
        let outcome = fetch_for_agent(&format!("http://127.0.0.1:{port}/away"), LOCAL)
            .await
            .expect("outcome");
        assert_eq!(
            outcome,
            AgentFetchOutcome::RedirectedOffHost {
                from: format!("http://127.0.0.1:{port}/away"),
                to: format!("http://localhost:{port}/new"),
            }
        );
    }

    #[tokio::test]
    async fn refuses_binary_pages_and_errors() {
        let port = serve(|line, _| {
            if line.starts_with("GET /image ") {
                http("200 OK", "Content-Type: image/png\r\n", "PNG")
            } else {
                http("404 Not Found", "Content-Type: text/plain\r\n", "no")
            }
        })
        .await;
        let binary = fetch_for_agent(&format!("http://127.0.0.1:{port}/image"), LOCAL)
            .await
            .expect_err("binary");
        assert!(binary.0.contains("image/png"));
        let missing = fetch_for_agent(&format!("http://127.0.0.1:{port}/x"), LOCAL)
            .await
            .expect_err("missing");
        assert!(missing.0.contains("404"));
    }

    #[tokio::test]
    async fn long_pages_are_cut_for_the_model() {
        let port = serve(|_, _| {
            let body = "a".repeat(MAX_AGENT_TEXT_BYTES + 100);
            http("200 OK", "Content-Type: text/plain\r\n", &body)
        })
        .await;
        let AgentFetchOutcome::Page(page) =
            fetch_for_agent(&format!("http://127.0.0.1:{port}/"), LOCAL)
                .await
                .expect("page")
        else {
            panic!("expected a page");
        };
        assert_eq!(page.text.len(), MAX_AGENT_TEXT_BYTES);
        assert!(page.truncated);
    }

    #[tokio::test]
    async fn searches_a_searxng_server() {
        let port = serve(|line, _| {
            assert!(line.contains("q=rust+book"), "{line}");
            assert!(line.contains("format=json"), "{line}");
            http(
                "200 OK",
                "Content-Type: application/json\r\n",
                r#"{"results":[{"title":"The Book","url":"https://doc.rust-lang.org/book/","content":"Learn Rust"},{"title":"bad","url":"javascript:alert(1)","content":"x"}]}"#,
            )
        })
        .await;
        let results = search_for_agent(
            &format!("http://127.0.0.1:{port}/search"),
            "rust book",
            LOCAL,
        )
        .await
        .expect("results");
        assert_eq!(
            results,
            vec![SearchResult {
                title: "The Book".into(),
                url: "https://doc.rust-lang.org/book/".into(),
                snippet: "Learn Rust".into(),
            }]
        );
    }
}
