//! Streamable HTTP on an exact reviewed endpoint and pinned address set.
//! Credential-bearing redirects and automatic authentication helpers are absent.
use crate::MAX_MESSAGE_BYTES;
use argentum_security::SecretValue;
use futures_util::{
    stream::{self, BoxStream},
    StreamExt,
};
use http::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClient, StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
    StreamableHttpError, StreamableHttpPostResponse,
};
use sse_stream::{Error as SseError, Sse};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use url::Url;

type HttpError = StreamableHttpError<io::Error>;
fn rejected(reason: &'static str) -> HttpError {
    StreamableHttpError::Io(io::Error::new(io::ErrorKind::PermissionDenied, reason))
}

pub fn endpoint_origin(endpoint: &str) -> Result<String, io::Error> {
    let url = Url::parse(endpoint).map_err(|_| io::Error::other("invalid endpoint"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(io::Error::other("invalid endpoint origin"));
    }
    Ok(url.origin().ascii_serialization())
}

pub struct ScopedCredential {
    pub origin: String,
    pub account: String,
    pub secret: Arc<SecretValue>,
}

#[derive(Clone)]
pub struct ReviewedHttp {
    endpoint: Arc<str>,
    client: reqwest::Client,
    credential: Option<Arc<ScopedCredential>>,
    pub request_starts: Arc<AtomicU64>,
    pub received_bytes: Arc<AtomicU64>,
}

fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_documentation()
                || octets[0] == 0
                || octets[0] >= 240
                || (octets[0] == 100 && (64..128).contains(&octets[1]))
                || (octets[0] == 198 && matches!(octets[1], 18 | 19)))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_address(IpAddr::V4(v4));
            }
            let segments = ip.segments();
            segments[0] & 0xe000 == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

impl ReviewedHttp {
    /// Addresses are owner-reviewed or resolved by the trusted configuring path,
    /// then pinned. Runtime never resolves a new destination from server data.
    pub fn new(
        endpoint: &str,
        addresses: &[SocketAddr],
        local_demo: bool,
        credential: Option<ScopedCredential>,
    ) -> Result<Self, HttpError> {
        let url = Url::parse(endpoint).map_err(|_| rejected("invalid_endpoint"))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| rejected("invalid_endpoint_port"))?;
        let host = url
            .host_str()
            .ok_or_else(|| rejected("endpoint_host_missing"))?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || addresses.is_empty()
            || addresses.len() > 8
        {
            return Err(rejected("invalid_endpoint_binding"));
        }
        for address in addresses {
            if address.port() != port {
                return Err(rejected("address_port_binding_mismatch"));
            }
            if host
                .parse::<IpAddr>()
                .is_ok_and(|literal| literal != address.ip())
            {
                return Err(rejected("literal_endpoint_address_mismatch"));
            }
            if local_demo {
                if host != "127.0.0.1"
                    || url.scheme() != "http"
                    || address.ip() != IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                {
                    return Err(rejected("local_demo_requires_exact_ipv4_loopback"));
                }
            } else if url.scheme() != "https" || !public_address(address.ip()) {
                return Err(rejected(
                    "remote_endpoint_requires_https_and_public_addresses",
                ));
            }
        }
        if let Some(value) = &credential {
            if value.origin != url.origin().ascii_serialization()
                || value.account.is_empty()
                || value.account.len() > 128
            {
                return Err(rejected("credential_origin_or_account_mismatch"));
            }
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(0)
            .resolve_to_addrs(host, addresses)
            .build()
            .map_err(|_| rejected("http_client_unavailable"))?;
        Ok(Self {
            endpoint: Arc::from(url.as_str()),
            client,
            credential: credential.map(Arc::new),
            request_starts: Arc::default(),
            received_bytes: Arc::default(),
        })
    }

    pub fn transport(&self) -> StreamableHttpClientTransport<Self> {
        let mut config = StreamableHttpClientTransportConfig::with_uri(Arc::clone(&self.endpoint));
        config.max_concurrent_requests = 1;
        config.channel_buffer_capacity = 4;
        config.max_sse_event_size = MAX_MESSAGE_BYTES;
        config.reinit_on_expired_session = false;
        StreamableHttpClientTransport::with_client(self.clone(), config)
    }

    fn request(
        &self,
        method: reqwest::Method,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<reqwest::RequestBuilder, HttpError> {
        if uri.as_ref() != self.endpoint.as_ref() || auth_header.is_some() {
            return Err(rejected("connection_or_credential_binding_mismatch"));
        }
        let mut request = self
            .client
            .request(method, self.endpoint.as_ref())
            .header("accept", "application/json, text/event-stream");
        if let Some(session) = session {
            if session.len() > 256 || !session.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(rejected("invalid_session_identity"));
            }
            request = request.header("mcp-session-id", session.as_ref());
        }
        for (name, value) in headers {
            if !name.as_str().starts_with("mcp-") {
                return Err(rejected("unreviewed_http_header"));
            }
            request = request.header(name, value);
        }
        if let Some(credential) = &self.credential {
            request = request.bearer_auth(credential.secret.expose());
        }
        Ok(request)
    }

    async fn bounded_body(&self, response: reqwest::Response) -> Result<Vec<u8>, HttpError> {
        if response
            .content_length()
            .is_some_and(|size| size > MAX_MESSAGE_BYTES as u64)
        {
            return Err(rejected("http_response_capacity_exceeded"));
        }
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| rejected("http_response_disconnected"))?;
            self.received_bytes
                .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            if chunk.len() > MAX_MESSAGE_BYTES.saturating_sub(body.len()) {
                return Err(rejected("http_response_capacity_exceeded"));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

impl StreamableHttpClient for ReviewedHttp {
    type Error = io::Error;
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, HttpError> {
        let body = serde_json::to_vec(&message).map_err(|_| rejected("invalid_http_request"))?;
        if body.len() > MAX_MESSAGE_BYTES {
            return Err(rejected("http_request_capacity_exceeded"));
        }
        let request = self
            .request(reqwest::Method::POST, uri, session, auth_header, headers)?
            .header("content-type", "application/json")
            .body(body);
        self.request_starts.fetch_add(1, Ordering::Relaxed);
        let response = request
            .send()
            .await
            .map_err(|_| rejected("http_request_failed_usage_and_effects_may_be_unknown"))?;
        match response.status().as_u16() {
            202 | 204 => return Ok(StreamableHttpPostResponse::Accepted),
            200 => {}
            401 | 403 => {
                return Err(rejected(
                    "http_authentication_or_scope_required_use_secure_connection_configuration",
                ))
            }
            404 => return Err(StreamableHttpError::SessionExpired),
            300..=399 => return Err(rejected("http_redirect_denied")),
            _ => return Err(rejected("http_server_rejected_request")),
        }
        let session = response
            .headers()
            .get("mcp-session-id")
            .map(|value| value.to_str().map(str::to_owned))
            .transpose()
            .map_err(|_| rejected("invalid_session_header"))?;
        if session
            .as_ref()
            .is_some_and(|s| s.len() > 256 || !s.bytes().all(|b| b.is_ascii_graphic()))
        {
            return Err(rejected("invalid_session_header"));
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        let body = self.bounded_body(response).await?;
        if content_type == "application/json" {
            let message = serde_json::from_slice(&body)
                .map_err(|_| rejected("malformed_http_jsonrpc_response"))?;
            return Ok(StreamableHttpPostResponse::Json(message, session));
        }
        if content_type != "text/event-stream" {
            return Err(rejected("unsupported_http_response_content_type"));
        }
        let events = parse_bounded_sse(&body)?;
        Ok(StreamableHttpPostResponse::Sse(
            stream::iter(events.into_iter().map(Ok::<_, SseError>)).boxed(),
            session,
        ))
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session: Arc<str>,
        auth_header: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), HttpError> {
        let request = self.request(
            reqwest::Method::DELETE,
            uri,
            Some(session),
            auth_header,
            headers,
        )?;
        self.request_starts.fetch_add(1, Ordering::Relaxed);
        let response = request
            .send()
            .await
            .map_err(|_| rejected("session_teardown_failed"))?;
        if matches!(response.status().as_u16(), 200 | 202 | 204 | 404 | 405) {
            Ok(())
        } else {
            Err(rejected("session_teardown_rejected"))
        }
    }
    async fn get_stream(
        &self,
        _uri: Arc<str>,
        _session: Option<Arc<str>>,
        _last_event_id: Option<String>,
        _auth_header: Option<String>,
        _headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxStream<'static, Result<Sse, SseError>>, HttpError> {
        // Unsolicited subscriptions and replay are not part of this profile.
        Err(StreamableHttpError::ServerDoesNotSupportSse)
    }
}

fn parse_bounded_sse(body: &[u8]) -> Result<Vec<Sse>, HttpError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| rejected("malformed_sse_utf8"))?
        .replace("\r\n", "\n");
    if !text.ends_with("\n\n") {
        return Err(rejected("incomplete_sse_response"));
    }
    let mut events = Vec::new();
    for frame in text.split("\n\n").filter(|frame| !frame.is_empty()) {
        if events.len() >= 64 {
            return Err(rejected("sse_event_count_exceeded"));
        }
        let mut event = Sse::default();
        let mut data = Vec::new();
        for line in frame.lines() {
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match field {
                "data" => data.push(value),
                "event" => event.event = Some(value.to_owned()),
                "id" => {
                    if value.len() > 256 {
                        return Err(rejected("invalid_sse_event_identity"));
                    }
                    event.id = Some(value.to_owned());
                }
                "retry" => {}
                _ => {}
            }
        }
        if !data.is_empty() {
            event.data = Some(data.join("\n"));
            events.push(event);
        }
    }
    if events.is_empty() {
        return Err(rejected("sse_response_has_no_message"));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_origin_and_destination_controls_precede_dispatch() {
        let address = "127.0.0.1:18080".parse().unwrap();
        assert!(ReviewedHttp::new("http://127.0.0.1:18080/mcp", &[address], true, None).is_ok());
        assert!(ReviewedHttp::new("http://127.0.0.1:18080/mcp", &[address], false, None).is_err());
        assert!(ReviewedHttp::new(
            "https://127.0.0.1/mcp",
            &["8.8.8.8:443".parse().unwrap()],
            false,
            None
        )
        .is_err());
        let credential = ScopedCredential {
            origin: "http://127.0.0.1:18081".into(),
            account: "synthetic".into(),
            secret: Arc::new(SecretValue::new("synthetic-test-value")),
        };
        assert!(ReviewedHttp::new(
            "http://127.0.0.1:18080/mcp",
            &[address],
            true,
            Some(credential)
        )
        .is_err());
        for value in [
            "10.0.0.1",
            "169.254.169.254",
            "127.0.0.1",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!public_address(value.parse().unwrap()), "{value}");
        }
    }
    #[test]
    fn bounded_response_stream_framing() {
        assert_eq!(
            parse_bounded_sse(b"event: message\ndata: {\"jsonrpc\":\"2.0\"}\n\n")
                .unwrap()
                .len(),
            1
        );
        assert!(parse_bounded_sse(b"data: incomplete").is_err());
    }
}
