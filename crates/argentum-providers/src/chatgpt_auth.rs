//! Native ChatGPT OAuth. Browser consent is owner work, never a model tool.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const ISSUER: &str = "https://auth.openai.com";
pub const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
pub const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
pub const JWKS: &str = "https://auth.openai.com/.well-known/jwks.json";
pub const RESOURCE: &str = "https://api.openai.com/v1";
pub const PROFILE_ID: &str = "chatgpt";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("ChatGPT sign-in was cancelled or denied")]
    Cancelled,
    #[error("ChatGPT authorization callback is missing, expired, reused or mismatched")]
    Callback,
    #[error("ChatGPT identity token could not be verified")]
    Identity,
    #[error("ChatGPT plan usage permission was not granted")]
    Permission,
    #[error("ChatGPT secure credential storage is unavailable")]
    Storage,
    #[error("ChatGPT authentication endpoint failed; retry sign-in")]
    Transport,
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PendingAuthorization {
    pub(crate) state: String,
    pub(crate) nonce: String,
    pub(crate) verifier: String,
    pub(crate) redirect: String,
    pub(crate) client_id: String,
    #[zeroize(skip)]
    pub(crate) started: std::time::Instant,
    pub(crate) consumed: bool,
}
impl std::fmt::Debug for PendingAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingAuthorization")
            .field("consumed", &self.consumed)
            .finish_non_exhaustive()
    }
}
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
fn random_value() -> Result<String, AuthError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| AuthError::Storage)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
impl PendingAuthorization {
    pub fn new(
        port: u16,
        host_id: &str,
        returning_client: Option<&str>,
    ) -> Result<(Self, Url), AuthError> {
        if port == 0 || canonical_host_id(host_id).ok().as_deref() != Some(host_id) {
            return Err(AuthError::Callback);
        }
        let pending = Self {
            state: random_value()?,
            nonce: random_value()?,
            verifier: random_value()?,
            redirect: format!("http://127.0.0.1:{port}/auth/callback"),
            client_id: returning_client.unwrap_or("dynamic_agent_client").into(),
            started: std::time::Instant::now(),
            consumed: false,
        };
        if returning_client.is_some_and(|id| !valid_client(id)) {
            return Err(AuthError::Callback);
        }
        let mut url = Url::parse(AUTHORIZE).map_err(|_| AuthError::Callback)?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("client_id", &pending.client_id)
                .append_pair("ext_agent_host_id", host_id)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &pending.redirect)
                .append_pair("scope", SCOPES)
                .append_pair("resource", RESOURCE)
                .append_pair("state", &pending.state)
                .append_pair("nonce", &pending.nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &pkce_challenge(&pending.verifier));
            if returning_client.is_none() {
                query.append_pair("agent_name_hint", "Argentum");
            }
        }
        Ok((pending, url))
    }
    pub fn consume_callback(&mut self, target: &str) -> Result<(String, String), AuthError> {
        if self.consumed
            || self.started.elapsed() > std::time::Duration::from_secs(300)
            || target.len() > 8192
        {
            return Err(AuthError::Callback);
        }
        let url =
            Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| AuthError::Callback)?;
        if !target.starts_with("/auth/callback?")
            || url.path() != "/auth/callback"
            || url.fragment().is_some()
        {
            return Err(AuthError::Callback);
        }
        let mut fields = std::collections::BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if fields
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(AuthError::Callback);
            }
        }
        if fields.get("state") != Some(&self.state) {
            return Err(AuthError::Callback);
        }
        self.consumed = true;
        if fields.contains_key("error") {
            return Err(AuthError::Cancelled);
        }
        let client = fields.get("client_id").unwrap_or(&self.client_id).clone();
        if !valid_client(&client)
            || (self.client_id != "dynamic_agent_client" && client != self.client_id)
        {
            return Err(AuthError::Callback);
        }
        let code = fields
            .get("code")
            .filter(|value| !value.is_empty() && value.len() <= 4096)
            .ok_or(AuthError::Callback)?
            .clone();
        Ok((code, client))
    }
}
/// UUID host identifiers are URIs, not bare UUID strings. Accept legacy stored
/// UUIDs only for same-identity migration; the wire constructor requires the URI.
pub(crate) fn canonical_host_id(value: &str) -> Result<String, AuthError> {
    let uuid = uuid::Uuid::parse_str(value).map_err(|_| AuthError::Storage)?;
    if uuid.get_version_num() != 4 || uuid.get_variant() != uuid::Variant::RFC4122 {
        return Err(AuthError::Storage);
    }
    if value != uuid.hyphenated().to_string() && value != uuid.urn().to_string() {
        return Err(AuthError::Storage);
    }
    Ok(uuid.urn().to_string())
}
pub(crate) fn valid_client(client: &str) -> bool {
    client.starts_with("oaiapp_")
        && client.len() <= 256
        && client
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub(crate) struct Credentials {
    pub subject: String,
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub scopes: String,
    pub expires_at: u64,
    pub earliest_refresh_at: u64,
}
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([REDACTED])")
    }
}

#[derive(Clone)]
pub struct ChatGptAuth {
    store: std::sync::Arc<dyn argentum_platform::SecretStore>,
    client: reqwest::Client,
}
impl std::fmt::Debug for ChatGptAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChatGptAuth([REDACTED])")
    }
}
#[derive(Debug, Serialize)]
pub struct AccountStatus {
    pub account: String,
    pub selected: bool,
    pub signed_in: bool,
    pub direct_usage_granted: bool,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    id_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
    #[serde(default)]
    earliest_refresh_at: u64,
}
fn now() -> Result<u64, AuthError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| AuthError::Identity)
}
fn permission(scopes: &str) -> bool {
    scopes
        .split_ascii_whitespace()
        .any(|s| s == "chatgpt.tokens.use.direct")
}
async fn bounded_response(
    response: reqwest::Response,
) -> Result<zeroize::Zeroizing<Vec<u8>>, AuthError> {
    use futures_util::StreamExt;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > 64 * 1024) {
        return Err(AuthError::Transport);
    }
    let mut data = zeroize::Zeroizing::new(Vec::new());
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AuthError::Transport)?;
        if data.len() + chunk.len() > 64 * 1024 {
            return Err(AuthError::Transport);
        }
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}
impl ChatGptAuth {
    pub fn new(
        store: std::sync::Arc<dyn argentum_platform::SecretStore>,
    ) -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| AuthError::Transport)?;
        Ok(Self { store, client })
    }
    pub fn accounts(&self) -> Result<Vec<AccountStatus>, AuthError> {
        let _lock = crate::chatgpt_vault::Lock::acquire()?;
        let index = crate::chatgpt_vault::index(self.store.as_ref())?;
        index
            .accounts
            .iter()
            .map(|a| {
                let credentials = crate::chatgpt_vault::load(self.store.as_ref(), &a.id)?;
                if credentials
                    .as_ref()
                    .is_some_and(|c| c.client_id != a.client || c.subject != a.subject)
                {
                    return Err(AuthError::Identity);
                }
                Ok(AccountStatus {
                    account: a.id.clone(),
                    selected: index.selected.as_ref() == Some(&a.id),
                    signed_in: credentials.is_some(),
                    direct_usage_granted: credentials
                        .as_ref()
                        .is_some_and(|c| permission(&c.scopes)),
                })
            })
            .collect()
    }
    pub fn configured(&self) -> bool {
        self.accounts().is_ok_and(|a| {
            a.iter()
                .any(|a| a.selected && a.signed_in && a.direct_usage_granted)
        })
    }
    pub(crate) fn selected_account(&self) -> Result<String, AuthError> {
        self.accounts()?
            .into_iter()
            .find(|a| a.selected && a.signed_in && a.direct_usage_granted)
            .map(|a| a.account)
            .ok_or(AuthError::Permission)
    }
    pub fn select(&self, account: &str) -> Result<(), AuthError> {
        let _lock = crate::chatgpt_vault::Lock::acquire()?;
        crate::chatgpt_vault::select(self.store.as_ref(), account)
    }
    async fn token(
        &self,
        form: &[(&str, &str)],
        client: &str,
        nonce: Option<&str>,
        subject: Option<&str>,
    ) -> Result<Credentials, AuthError> {
        let data = bounded_response(
            self.client
                .post(TOKEN)
                .form(form)
                .send()
                .await
                .map_err(|_| AuthError::Transport)?,
        )
        .await?;
        let token: TokenResponse =
            serde_json::from_slice(&data).map_err(|_| AuthError::Identity)?;
        if !token.token_type.eq_ignore_ascii_case("bearer")
            || token.access_token.is_empty()
            || token.refresh_token.is_empty()
            || token.expires_in == 0
            || token.expires_in > 24 * 3600
            || !valid_client(client)
        {
            return Err(AuthError::Identity);
        }
        let keys = bounded_response(
            self.client
                .get(JWKS)
                .send()
                .await
                .map_err(|_| AuthError::Transport)?,
        )
        .await?;
        let keys = serde_json::from_slice(&keys).map_err(|_| AuthError::Identity)?;
        let current = now()?;
        let identity = crate::chatgpt_identity::verify(
            &token.id_token,
            &keys,
            client,
            nonce,
            subject,
            current,
        )?;
        if !permission(&token.scope) {
            return Err(AuthError::Permission);
        }
        Ok(Credentials {
            subject: identity,
            client_id: client.into(),
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
            id_token: token.id_token.clone(),
            scopes: token.scope.clone(),
            expires_at: current
                .checked_add(token.expires_in)
                .ok_or(AuthError::Identity)?,
            earliest_refresh_at: token.earliest_refresh_at,
        })
    }
    /// The URL is sent only to the system browser. Neither it nor callback data
    /// are returned as events or printed. No generation request is submitted.
    pub async fn sign_in(&self, add_account: bool) -> Result<String, AuthError> {
        self.sign_in_with_cancellation(add_account, tokio_util::sync::CancellationToken::new())
            .await
    }
    pub async fn sign_in_with_cancellation(
        &self,
        add_account: bool,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<String, AuthError> {
        use std::io::{Read, Write};
        struct CancelOnDrop(tokio_util::sync::CancellationToken);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.cancel();
            }
        }
        let _cancel_on_drop = CancelOnDrop(cancel.clone());
        let (host, returning, subject) = {
            let _lock = crate::chatgpt_vault::Lock::acquire()?;
            let index = crate::chatgpt_vault::index(self.store.as_ref())?;
            let account = if add_account {
                None
            } else {
                index
                    .accounts
                    .iter()
                    .find(|a| index.selected.as_ref() == Some(&a.id))
            };
            (
                crate::chatgpt_vault::host(self.store.as_ref())?,
                match account {
                    Some(account) => Some(account.client.clone()),
                    None if !add_account => crate::chatgpt_vault::pending(self.store.as_ref())?,
                    None => None,
                },
                account.map(|a| a.subject.clone()),
            )
        };
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| AuthError::Callback)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| AuthError::Callback)?;
        let port = listener
            .local_addr()
            .map_err(|_| AuthError::Callback)?
            .port();
        let (mut pending, url) = PendingAuthorization::new(port, &host, returning.as_deref())?;
        open_browser(url.as_str())?;
        let callback=tokio::task::spawn_blocking(move|| {
            let limit=std::time::Instant::now()+std::time::Duration::from_secs(300);
            while std::time::Instant::now()<limit && !cancel.is_cancelled() {
                match listener.accept() {
                    Ok((mut socket,peer))=> {
                        if !peer.ip().is_loopback() {continue;}
                        socket.set_read_timeout(Some(std::time::Duration::from_secs(2))).map_err(|_|AuthError::Callback)?;
                        socket.set_write_timeout(Some(std::time::Duration::from_secs(2))).map_err(|_|AuthError::Callback)?;
                        let mut bytes=zeroize::Zeroizing::new(Vec::new());
                        let mut buf=[0u8;1024];
                        while bytes.len()<8192 && !bytes.windows(4).any(|w|w==b"\r\n\r\n") {
                            match socket.read(&mut buf) {Ok(0)|Err(_)=>break,Ok(n)=>bytes.extend_from_slice(&buf[..n])}
                        }
                        let text=std::str::from_utf8(&bytes).map_err(|_|AuthError::Callback)?;
                        let mut lines=text.split("\r\n");
                        let request:Vec<_>=lines.next().unwrap_or_default().split(' ').collect();
                        let expected_host=format!("127.0.0.1:{port}");
                        let hosts:Vec<_>=lines.filter_map(|l|l.split_once(':')).filter(|(k,_)|k.eq_ignore_ascii_case("host")).map(|(_,v)|v.trim()).collect();
                        let result=if bytes.len()<=8192 && request.len()==3 && request[0]=="GET" && request[2]=="HTTP/1.1" && hosts==[expected_host.as_str()] {
                            pending.consume_callback(request[1])
                        } else {Err(AuthError::Callback)};
                        let message=if result.is_ok(){"Authorization received. Return to Argentum to see the verified result."}else{"Authorization was not accepted. Return to Argentum."};
                        let _=write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",message.len(),message);
                        match result {Ok((code,client))=>return Ok((pending,code,client)),Err(AuthError::Cancelled)=>return Err(AuthError::Cancelled),Err(_) if pending.consumed=>return Err(AuthError::Callback),Err(_)=>{}}
                    },
                    Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(_)=>return Err(AuthError::Callback),
                }
            }
            Err(AuthError::Cancelled)
        }).await.map_err(|_|AuthError::Callback)??;
        let (pending, code, client) = callback;
        {
            let _lock = crate::chatgpt_vault::Lock::acquire()?;
            crate::chatgpt_vault::retain_registration(self.store.as_ref(), &client)?;
        }
        let code = zeroize::Zeroizing::new(code);
        let credentials = self
            .token(
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", &client),
                    ("code", &code),
                    ("code_verifier", &pending.verifier),
                    ("redirect_uri", &pending.redirect),
                    ("resource", RESOURCE),
                ],
                &client,
                Some(&pending.nonce),
                subject.as_deref(),
            )
            .await?;
        let _lock = crate::chatgpt_vault::Lock::acquire()?;
        let id = crate::chatgpt_vault::save(self.store.as_ref(), &credentials)?;
        crate::chatgpt_vault::retire_pending(self.store.as_ref(), &client)?;
        Ok(id)
    }
    pub(crate) async fn bearer_for(
        &self,
        expected_account: &str,
    ) -> Result<argentum_security::SecretValue, AuthError> {
        let _lock = crate::chatgpt_vault::Lock::acquire()?;
        let index = crate::chatgpt_vault::index(self.store.as_ref())?;
        let id = index.selected.ok_or(AuthError::Permission)?;
        if id != expected_account {
            return Err(AuthError::Identity);
        }
        let mut credentials =
            crate::chatgpt_vault::load(self.store.as_ref(), &id)?.ok_or(AuthError::Permission)?;
        let current = now()?;
        if current.saturating_add(60) >= credentials.expires_at {
            if current < credentials.earliest_refresh_at {
                return Err(AuthError::Permission);
            }
            credentials = self
                .token(
                    &[
                        ("grant_type", "refresh_token"),
                        ("client_id", &credentials.client_id),
                        ("refresh_token", &credentials.refresh_token),
                        ("resource", RESOURCE),
                    ],
                    &credentials.client_id,
                    None,
                    Some(&credentials.subject),
                )
                .await?;
            let saved = crate::chatgpt_vault::save(self.store.as_ref(), &credentials)?;
            if saved != id {
                return Err(AuthError::Identity);
            }
        }
        if current >= credentials.expires_at || !permission(&credentials.scopes) {
            return Err(AuthError::Permission);
        }
        Ok(argentum_security::SecretValue::new(
            credentials.access_token.clone(),
        ))
    }
    pub async fn sign_out(&self) -> Result<(), AuthError> {
        let _lock = crate::chatgpt_vault::Lock::acquire()?;
        let index = crate::chatgpt_vault::index(self.store.as_ref())?;
        let id = index.selected.ok_or(AuthError::Permission)?;
        if let Some(credentials) = crate::chatgpt_vault::load(self.store.as_ref(), &id)? {
            let discovery = bounded_response(
                self.client
                    .get(format!("{ISSUER}/.well-known/openid-configuration"))
                    .send()
                    .await
                    .map_err(|_| AuthError::Transport)?,
            )
            .await?;
            let discovery: serde_json::Value =
                serde_json::from_slice(&discovery).map_err(|_| AuthError::Transport)?;
            let endpoint = Url::parse(
                discovery["revocation_endpoint"]
                    .as_str()
                    .ok_or(AuthError::Transport)?,
            )
            .map_err(|_| AuthError::Transport)?;
            if endpoint.scheme() != "https"
                || endpoint.host_str() != Some("auth.openai.com")
                || endpoint.port_or_known_default() != Some(443)
                || !endpoint.username().is_empty()
                || endpoint.password().is_some()
                || endpoint.query().is_some()
                || endpoint.fragment().is_some()
            {
                return Err(AuthError::Transport);
            }
            let response = self
                .client
                .post(endpoint)
                .form(&[
                    ("token", credentials.refresh_token.as_str()),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", credentials.client_id.as_str()),
                ])
                .send()
                .await
                .map_err(|_| AuthError::Transport)?;
            if response.status() != reqwest::StatusCode::OK {
                return Err(AuthError::Transport);
            }
        }
        crate::chatgpt_vault::clear(self.store.as_ref(), &id)
    }
}
#[cfg(windows)]
fn open_browser(url: &str) -> Result<(), AuthError> {
    // Use the OS URI handler, not a PATH/CWD-resolved executable or shell.
    let operation: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
    let target: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(AuthError::Transport)
    }
}
#[cfg(not(windows))]
fn open_browser(_url: &str) -> Result<(), AuthError> {
    Err(AuthError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_is_one_use_state_bound_and_requires_issued_registration() {
        let (mut pending, url) =
            PendingAuthorization::new(4567, &uuid::Uuid::new_v4().urn().to_string(), None).unwrap();
        assert!(url.as_str().contains("client_id=dynamic_agent_client"));
        assert!(url.as_str().contains("agent_name_hint=Argentum"));
        assert!(pending
            .consume_callback("/auth/callback?state=wrong&code=synthetic&client_id=oaiapp_test")
            .is_err());
        let target = format!(
            "/auth/callback?state={}&code=synthetic&client_id=oaiapp_test",
            pending.state
        );
        assert_eq!(
            pending.consume_callback(&target).unwrap(),
            ("synthetic".into(), "oaiapp_test".into())
        );
        assert!(pending.consume_callback(&target).is_err());
    }
    #[test]
    fn returning_identity_and_callback_fields_cannot_be_replaced() {
        for target in [
            "/callback",
            "/auth/callback?state=bad&state=bad",
            "/auth/callback?state=bad",
        ] {
            let (mut pending, _) = PendingAuthorization::new(
                4567,
                &uuid::Uuid::new_v4().urn().to_string(),
                Some("oaiapp_original"),
            )
            .unwrap();
            assert!(pending.consume_callback(target).is_err());
        }
        let (mut pending, _) = PendingAuthorization::new(
            4567,
            &uuid::Uuid::new_v4().urn().to_string(),
            Some("oaiapp_original"),
        )
        .unwrap();
        assert!(pending
            .consume_callback(&format!(
                "/auth/callback?state={}&code=synthetic&client_id=oaiapp_other",
                pending.state
            ))
            .is_err());
        assert!(!format!("{pending:?}").contains(&pending.verifier));
    }

    #[test]
    fn authorization_serializes_the_documented_host_uri_for_new_and_returning_clients() {
        let uuid = uuid::Uuid::new_v4();
        let host = uuid.urn().to_string();
        for returning in [None, Some("oaiapp_existing")] {
            let (_, url) = PendingAuthorization::new(4567, &host, returning).unwrap();
            let fields: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
            assert_eq!(
                fields.get("ext_agent_host_id").map(|v| v.as_ref()),
                Some(host.as_str())
            );
            assert!(url.as_str().contains("ext_agent_host_id=urn%3Auuid%3A"));
        }
        assert!(PendingAuthorization::new(4567, &uuid.to_string(), None).is_err());
        assert!(PendingAuthorization::new(
            4567,
            "urn:uuid:00000000-0000-0000-0000-000000000000",
            None
        )
        .is_err());
    }
}
