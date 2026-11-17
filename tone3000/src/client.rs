//! The HTTP layer: authentication, timeout/retry/backoff, and offset
//! pagination.
//!
//! [`Client`] is transport-agnostic. The production transport is
//! [`ReqwestTransport`]; tests inject a mock [`Transport`] so pagination,
//! retries and every error path can be exercised without the network.
//!
//! # Secrets
//!
//! Credentials come from [`crate::auth::Credentials`] and are only ever used
//! to build an `Authorization` header in memory. They are never written into a
//! [`Request`] URL, an error message, or a log line. [`Error`] Display output
//! contains status codes and server-provided messages only.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::time::sleep;

use crate::api::{Paginated, Tone, ToneList};
use crate::auth::Credentials;

/// v1 API base URL.
pub const DEFAULT_BASE_URL: &str = "https://www.tone3000.com/api/v1";
/// Per-request timeout for the production transport.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Hard stop for a pagination walk. A walk that needs more pages than this is
/// almost certainly a server bug or a malformed envelope; fail loudly instead
/// of looping forever.
const MAX_PAGES: u32 = 100_000;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A transport-level failure (DNS, TLS, timeout, connection reset). This never
/// carries a credential.
#[derive(Clone, Debug, thiserror::Error)]
pub enum TransportError {
    #[error("network error: {0}")]
    Network(String),
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("invalid URL: {0}")]
    Url(String),
}

/// Structured client errors.
///
/// The variants the task contract asks for are [`Error::Auth`],
/// [`Error::RateLimited`] and [`Error::Malformed`]; the rest cover timeouts,
/// transport failures and non-retryable HTTP statuses.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP 401/403: the credential is missing, invalid, or not entitled.
    #[error("authentication failed (HTTP {status}): {message}")]
    Auth { status: u16, message: String },

    /// HTTP 429 that survived the retry budget.
    #[error("rate limited (HTTP 429) after {attempts} attempt(s); retry after {retry_after:?}")]
    RateLimited {
        attempts: u32,
        retry_after: Option<Duration>,
    },

    /// The body was not the JSON shape the endpoint documents (bad syntax, or
    /// a paginated envelope with an impossible field).
    #[error("malformed response from {url}: {reason}")]
    Malformed { url: String, reason: String },

    /// A non-retryable HTTP status (400, 404, 422, 5xx after retries, …).
    #[error("HTTP {status} from {url}: {message}")]
    Http {
        status: u16,
        url: String,
        message: String,
    },

    /// The retry budget was spent on transport-level failures.
    #[error("request to {url} failed after {attempts} attempts: {source}")]
    RetryExhausted {
        url: String,
        attempts: u32,
        source: Box<Error>,
    },

    /// A single request exceeded the timeout (only surfaced directly when the
    /// caller uses the raw transport).
    #[error("request timed out after {0:?}")]
    Timeout(Duration),

    #[error("transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("invalid configuration: {0}")]
    Config(String),
}

// ---------------------------------------------------------------------------
// Request / response / transport
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// A transport-agnostic request. ``Debug` redacts `Authorization`; the credential is
/// never copied into [`Request::url`].
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(&str, &str)> = self
            .headers
            .iter()
            .map(|(k, v)| {
                let sensitive = k.eq_ignore_ascii_case("authorization")
                    || k.eq_ignore_ascii_case("proxy-authorization")
                    || k.eq_ignore_ascii_case("cookie");
                (k.as_str(), if sensitive { "***" } else { v.as_str() })
            })
            .collect();
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("query", &self.query)
            .field("headers", &headers)
            .field("body", &self.body.as_ref().map(|_| "<body>"))
            .finish()
    }
}

/// A transport-agnostic response.
#[derive(Clone, Debug)]
pub struct Response {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Response {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Case-insensitive header lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        let needle = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == needle)
            .map(|(_, v)| v.as_str())
    }

    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.body)
    }
}

/// Sends one request. Implementations must be cheap to clone/share.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: Request) -> Result<Response, TransportError>;
}

/// Production transport backed by `reqwest` + `rustls` (ring provider).
pub struct ReqwestTransport {
    client: reqwest::Client,
    timeout: Duration,
}

impl ReqwestTransport {
    pub fn new(timeout: Duration) -> Result<Self, Error> {
        install_ring_provider();
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("pedal-tone3000/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Config(format!("failed to build the HTTP client: {e}")))?;
        Ok(Self { client, timeout })
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new(DEFAULT_TIMEOUT).expect("default HTTP client builds")
    }
}

fn install_ring_provider() {
    // `rustls-no-provider` means reqwest will not pick a crypto provider for
    // us; ring is installed process-globally, once.
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut builder = self.client.request(method, &request.url);
        // `request.url` already carries the percent-encoded query string
        // (see `Client::url_for`); do not append `request.query` again.
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| map_reqwest_error(e, self.timeout))?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let body = resp.text().await.map_err(|e| {
            if e.is_timeout() {
                TransportError::Timeout(self.timeout)
            } else {
                TransportError::Network(e.to_string())
            }
        })?;
        Ok(Response {
            url: request.url,
            status,
            headers,
            body,
        })
    }
}

fn map_reqwest_error(e: reqwest::Error, timeout: Duration) -> TransportError {
    if e.is_timeout() {
        TransportError::Timeout(timeout)
    } else if e.is_builder() {
        // A malformed URL is a configuration error, not a network one.
        TransportError::Url(e.to_string())
    } else {
        // DNS, connection resets and TLS handshake failures all surface here.
        TransportError::Network(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Retry policy
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Total attempts, including the first. 1 disables retries.
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    /// Add ±25 % jitter so a fleet of clients does not retry in lockstep.
    pub jitter: bool,
    /// Minimum gap between request starts. The API allows ~100 req/min, so the
    /// refresh CLI sets this to 600 ms; tests leave it at zero.
    pub min_interval: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            jitter: true,
            min_interval: Duration::ZERO,
        }
    }
}

impl RetryPolicy {
    /// Delay before the next attempt. `attempt` is the attempt that just
    /// failed (1-based). `retry_after` from `Retry-After` always wins if it is
    /// longer than the computed backoff.
    pub fn delay_for(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        let exponent = attempt.saturating_sub(1).min(6);
        // `Duration::saturating_mul` takes a u32 factor, so the shift is capped.
        let factor = 1u32 << exponent;
        let exponential = self
            .initial_delay
            .saturating_mul(factor)
            .min(self.max_delay);
        let jittered = if self.jitter {
            apply_jitter(exponential, attempt)
        } else {
            exponential
        };
        match retry_after {
            Some(ra) if ra > jittered => ra,
            _ => jittered,
        }
    }
}

fn apply_jitter(base: Duration, attempt: u32) -> Duration {
    let nanos = base.as_nanos();
    if nanos == 0 {
        return base;
    }
    let seed = system_time_seed()
        .wrapping_add((attempt as u64).wrapping_shl(32))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let quarter = (nanos / 4) as i128;
    let delta = (z % (quarter as u64 + 1)) as i128 - quarter;
    let out = (nanos as i128 + delta).max(0) as u128;
    Duration::from_nanos(out as u64)
}

fn system_time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// A typed TONE3000 v1 client.
pub struct Client {
    transport: Arc<dyn Transport>,
    base_url: String,
    credentials: Option<Credentials>,
    retry: RetryPolicy,
    last_request: StdMutex<Option<Instant>>,
}

impl Client {
    pub fn new(
        transport: Arc<dyn Transport>,
        base_url: impl Into<String>,
        credentials: Option<Credentials>,
        retry: RetryPolicy,
    ) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
            credentials,
            retry,
            last_request: StdMutex::new(None),
        }
    }

    /// Production client: reqwest transport, credentials from the environment.
    pub fn from_env(base_url: impl Into<String>, retry: RetryPolicy) -> Result<Self, Error> {
        let transport = Arc::new(ReqwestTransport::new(DEFAULT_TIMEOUT)?);
        Ok(Self::new(
            transport,
            base_url,
            Credentials::from_env(),
            retry,
        ))
    }

    pub fn with_credentials(mut self, credentials: Option<Credentials>) -> Self {
        self.credentials = credentials;
        self
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry
    }

    /// One HTTP request with auth, timeout (transport-level) and retry.
    pub async fn get_raw(&self, path: &str, query: &[(String, String)]) -> Result<Response, Error> {
        let url = self.url_for(path, query);
        let mut headers = Vec::new();
        if let Some(creds) = &self.credentials {
            // Built in memory; never logged, never placed in the URL.
            headers.push(("Authorization".to_string(), creds.authorization_header()));
        }
        let request = Request {
            method: Method::Get,
            url: url.clone(),
            query: query.to_vec(),
            headers,
            body: None,
        };
        self.send_with_retry(request).await
    }

    /// GET and parse a JSON body of type `T`.
    pub async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<T, Error> {
        let resp = self.get_raw(path, query).await?;
        parse_json_response(&resp)
    }

    /// GET one page of a paginated endpoint.
    pub async fn get_paginated<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
        page: u32,
        page_size: u32,
    ) -> Result<Paginated<T>, Error> {
        let mut full = query.to_vec();
        full.push(("page".to_string(), page.to_string()));
        full.push(("page_size".to_string(), page_size.to_string()));
        let url = self.url_for(path, &full);
        let envelope: Paginated<T> = self.get_json(path, &full).await?;
        if envelope.page_size == 0 {
            return Err(Error::Malformed {
                url,
                reason: "paginated envelope reported page_size = 0".to_string(),
            });
        }
        if envelope.page != 0 && envelope.page != page {
            return Err(Error::Malformed {
                url,
                reason: format!(
                    "paginated envelope echoed page {} but page {} was requested",
                    envelope.page, page
                ),
            });
        }
        Ok(envelope)
    }

    /// Walk every page of a paginated endpoint using the envelope's
    /// `total_pages`. Fails (rather than looping) if the server stops echoing
    /// the requested page.
    pub async fn fetch_all_pages<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(String, String)],
        page_size: u32,
    ) -> Result<Vec<T>, Error> {
        let mut all = Vec::new();
        let mut page = 1u32;
        loop {
            let envelope = self.get_paginated(path, query, page, page_size).await?;
            let total_pages = envelope.total_pages;
            all.extend(envelope.data);
            if page >= total_pages {
                break;
            }
            page += 1;
            if page > MAX_PAGES {
                return Err(Error::Config(format!(
                    "pagination of {path} exceeded {MAX_PAGES} pages; aborting"
                )));
            }
        }
        Ok(all)
    }

    /// A non-paginated tone feed (`trending`, `latest`): `{data: [...]}`.
    pub async fn get_tone_feed(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Vec<Tone>, Error> {
        let list: ToneList = self.get_json(path, query).await?;
        Ok(list.into_inner())
    }

    async fn send_with_retry(&self, request: Request) -> Result<Response, Error> {
        let url = request.url.clone();
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            self.throttle().await;
            let outcome = self.transport.send(request.clone()).await;
            match outcome {
                Ok(resp) if resp.is_success() => return Ok(resp),
                Ok(resp) => {
                    let status = resp.status;
                    if status == 401 || status == 403 {
                        return Err(Error::Auth {
                            status,
                            message: error_message(&resp),
                        });
                    }
                    if status == 429 {
                        let retry_after = parse_retry_after_response(&resp);
                        if attempt >= self.retry.max_attempts {
                            return Err(Error::RateLimited {
                                attempts: attempt,
                                retry_after,
                            });
                        }
                        sleep(self.retry.delay_for(attempt, retry_after)).await;
                        continue;
                    }
                    if (500..=599).contains(&status) {
                        if attempt >= self.retry.max_attempts {
                            return Err(Error::Http {
                                status,
                                url,
                                message: error_message(&resp),
                            });
                        }
                        sleep(
                            self.retry
                                .delay_for(attempt, parse_retry_after_response(&resp)),
                        )
                        .await;
                        continue;
                    }
                    return Err(Error::Http {
                        status,
                        url,
                        message: error_message(&resp),
                    });
                }
                Err(TransportError::Timeout(d)) => {
                    if attempt >= self.retry.max_attempts {
                        return Err(Error::RetryExhausted {
                            url,
                            attempts: attempt,
                            source: Box::new(Error::Timeout(d)),
                        });
                    }
                    sleep(self.retry.delay_for(attempt, None)).await;
                }
                Err(e) => {
                    if attempt >= self.retry.max_attempts {
                        return Err(Error::RetryExhausted {
                            url,
                            attempts: attempt,
                            source: Box::new(Error::from(e)),
                        });
                    }
                    sleep(self.retry.delay_for(attempt, None)).await;
                }
            }
        }
    }

    async fn throttle(&self) {
        if self.retry.min_interval.is_zero() {
            return;
        }
        loop {
            let wait = {
                // Read-only lock: only the second block below takes a mutable one.
                let guard = self.last_request.lock().unwrap_or_else(|e| e.into_inner());
                match *guard {
                    Some(last) => {
                        let elapsed = last.elapsed();
                        if elapsed >= self.retry.min_interval {
                            None
                        } else {
                            Some(self.retry.min_interval - elapsed)
                        }
                    }
                    None => None,
                }
            };
            match wait {
                Some(d) => sleep(d).await,
                None => {
                    let mut guard = self.last_request.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = Some(Instant::now());
                    return;
                }
            }
        }
    }

    fn url_for(&self, path: &str, query: &[(String, String)]) -> String {
        let base = self.base_url.trim_end_matches('/');
        // Callers pass either `models` or `/models`, so tolerate both; the API
        // base URL ends without a trailing slash.
        let path = path.strip_prefix('/').unwrap_or(path);
        let mut url = format!("{base}/{path}");
        if !query.is_empty() {
            let pairs: Vec<String> = query
                .iter()
                .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
                .collect();
            url.push('?');
            url.push_str(&pairs.join("&"));
        }
        url
    }
}

/// Build a query vector from string literals.
pub fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// Percent-encode a query value (error messages only; the real request uses
/// `reqwest`'s own encoding).
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn parse_json_response<T: DeserializeOwned>(resp: &Response) -> Result<T, Error> {
    if resp.body.trim().is_empty() {
        return Err(Error::Malformed {
            url: resp.url.clone(),
            reason: "empty body where a JSON document was expected".to_string(),
        });
    }
    resp.json::<T>().map_err(|e| Error::Malformed {
        url: resp.url.clone(),
        reason: e.to_string(),
    })
}

/// Extract a human-readable message from an error response without leaking
/// headers or credentials.
fn error_message(resp: &Response) -> String {
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&resp.body) {
        for key in ["error_description", "error", "message"] {
            if let Some(Value::String(s)) = map.get(key) {
                if !s.trim().is_empty() {
                    return truncate(s.trim(), 300);
                }
            }
        }
    }
    let trimmed = resp.body.trim();
    if !trimmed.is_empty() {
        return truncate(trimmed, 300);
    }
    status_reason(resp.status).to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn status_reason(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        408 => "Request Timeout",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "HTTP error",
    }
}

fn parse_retry_after_response(resp: &Response) -> Option<Duration> {
    if let Some(value) = resp.header("retry-after") {
        if let Some(d) = parse_retry_after(value) {
            return Some(d);
        }
    }
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&resp.body) {
        for key in ["retry_after", "retryAfter", "retry_after_seconds"] {
            match map.get(key) {
                Some(Value::Number(n)) if n.is_u64() => {
                    return Some(Duration::from_secs(n.as_u64().unwrap_or(0)));
                }
                Some(Value::String(s)) => {
                    if let Some(d) = parse_retry_after(s) {
                        return Some(d);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Parse a `Retry-After` value: delta-seconds or an HTTP date.
pub fn parse_retry_after(value: &str) -> Option<Duration> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let parsed = time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc2822)
        .or_else(|_| {
            // IMF-fixdate, e.g. "Wed, 21 Oct 2015 07:28:00 GMT".
            let normalized = value.replace("GMT", "+00:00");
            time::OffsetDateTime::parse(&normalized, &time::format_description::well_known::Rfc3339)
        });
    match parsed {
        Ok(dt) => {
            let now = time::OffsetDateTime::now_utc();
            // `Sub` yields a `SignedDuration`; only a positive remainder is a
            // wait (a date in the past means "retry now").
            let delta = dt - now;
            if delta.is_negative() {
                return Some(Duration::ZERO);
            }
            Some(std::time::Duration::try_from(delta).unwrap_or(std::time::Duration::ZERO))
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;
    use crate::mock::{self, MockTransport, Scripted};
    use serde_json::json;

    #[test]
    fn request_debug_redacts_authorization() {
        let r = Request {
            method: Method::Get,
            url: "https://x/y".into(),
            query: vec![],
            headers: vec![("Authorization".into(), "Bearer s3cret-canary".into())],
            body: None,
        };
        let shown = format!("{r:?}");
        assert!(!shown.contains("s3cret-canary"), "{shown}");
    }

    #[tokio::test]
    async fn reqwest_transport_sends_each_query_parameter_exactly_once() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).unwrap();
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = r#"{"data":[],"page":1,"page_size":300,"total":0,"total_pages":0}"#;
            let _ = write!(
                sock,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            head.lines().next().unwrap_or_default().to_string()
        });
        let client = Client::new(
            Arc::new(ReqwestTransport::default()),
            format!("http://{addr}/v1"),
            None,
            fast_retry(1),
        );
        let _ = client
            .get_raw("models", &pairs(&[("tone_id", "1"), ("page", "2")]))
            .await;
        let line = server.join().unwrap();
        assert_eq!(line, "GET /v1/models?tone_id=1&page=2 HTTP/1.1");
    }

    fn fast_retry(max_attempts: u32) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(4),
            jitter: false,
            min_interval: Duration::ZERO,
        }
    }

    /// A client backed by a scripted transport, plus the transport so the test
    /// can inspect what was sent.
    fn scripted(responses: Vec<Scripted>, retry: RetryPolicy) -> (Client, MockTransport) {
        let transport = MockTransport::script(responses);
        let client = Client::new(
            Arc::new(transport.clone()),
            "https://api.test/v1",
            None,
            retry,
        );
        (client, transport)
    }

    fn scripted_with_credentials(
        responses: Vec<Scripted>,
        retry: RetryPolicy,
        token: &str,
    ) -> (Client, MockTransport) {
        let transport = MockTransport::script(responses);
        let credentials = Credentials::from_token(token, AuthKind::AccessToken);
        let client = Client::new(
            Arc::new(transport.clone()),
            "https://api.test/v1",
            Some(credentials),
            retry,
        );
        (client, transport)
    }

    fn page_query(request: &Request, key: &str) -> Option<String> {
        request
            .query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    // -----------------------------------------------------------------
    // Retry-After parsing / backoff
    // -----------------------------------------------------------------

    #[test]
    fn retry_after_parses_seconds_and_dates() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after(" 5 "), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("not-a-date"), None);
    }

    #[test]
    fn backoff_grows_and_is_capped_and_retry_after_wins() {
        let policy = RetryPolicy {
            max_attempts: 8,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(1000),
            jitter: false,
            min_interval: Duration::ZERO,
        };
        assert_eq!(policy.delay_for(1, None), Duration::from_millis(100));
        assert_eq!(policy.delay_for(2, None), Duration::from_millis(200));
        assert_eq!(policy.delay_for(3, None), Duration::from_millis(400));
        assert_eq!(policy.delay_for(9, None), Duration::from_millis(1000));
        assert_eq!(
            policy.delay_for(1, Some(Duration::from_secs(3))),
            Duration::from_secs(3)
        );
    }

    #[test]
    fn retry_after_is_read_from_the_header_and_the_body() {
        let header = mock::with_header(mock::response("u", 429, ""), "Retry-After", "12");
        assert_eq!(
            parse_retry_after_response(&header),
            Some(Duration::from_secs(12))
        );

        for body in [
            json!({ "retry_after": 7 }),
            json!({ "retryAfter": 7 }),
            json!({ "retry_after_seconds": 7 }),
            json!({ "retry_after": "7" }),
        ] {
            let resp = mock::response("u", 429, mock::json_body(&body));
            assert_eq!(
                parse_retry_after_response(&resp),
                Some(Duration::from_secs(7)),
                "body {body}"
            );
        }

        let none = mock::response("u", 429, json!({ "error": "slow down" }).to_string());
        assert_eq!(parse_retry_after_response(&none), None);
    }

    #[test]
    fn url_encoding_keeps_safe_characters() {
        assert_eq!(urlencode("a b&c"), "a%20b%26c");
    }

    // -----------------------------------------------------------------
    // Pagination
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn pagination_walks_every_page_in_order() {
        let page = |n: u32| match n {
            1 => mock::paginated(&[json!(1), json!(2)], 1, 2, 5),
            2 => mock::paginated(&[json!(3), json!(4)], 2, 2, 5),
            3 => mock::paginated(&[json!(5)], 3, 2, 5),
            _ => panic!("unexpected page"),
        };
        let (client, transport) = scripted(
            vec![
                Ok(mock::ok_json("u", page(1))),
                Ok(mock::ok_json("u", page(2))),
                Ok(mock::ok_json("u", page(3))),
            ],
            fast_retry(1),
        );

        let all: Vec<serde_json::Value> = client
            .fetch_all_pages("models", &[], 2)
            .await
            .expect("walk succeeds");
        assert_eq!(all, vec![json!(1), json!(2), json!(3), json!(4), json!(5)]);

        let requests = transport.requests();
        assert_eq!(requests.len(), 3);
        for (request, expected) in requests.iter().zip(["1", "2", "3"]) {
            assert_eq!(page_query(request, "page").as_deref(), Some(expected));
            assert_eq!(page_query(request, "page_size").as_deref(), Some("2"));
        }
    }

    #[tokio::test]
    async fn pagination_handles_a_single_empty_page() {
        let (client, transport) = scripted(
            vec![Ok(mock::ok_json(
                "u",
                mock::paginated(&Vec::<serde_json::Value>::new(), 1, 25, 0),
            ))],
            fast_retry(1),
        );
        let all: Vec<serde_json::Value> = client
            .fetch_all_pages("models", &[], 25)
            .await
            .expect("walk succeeds");
        assert!(all.is_empty());
        assert_eq!(transport.requests().len(), 1);
    }

    #[tokio::test]
    async fn pagination_rejects_an_envelope_that_echoes_the_wrong_page() {
        let (client, _transport) = scripted(
            vec![Ok(mock::ok_json(
                "u",
                mock::paginated(&[json!(1)], 2, 25, 1),
            ))],
            fast_retry(1),
        );
        let error = client
            .fetch_all_pages::<serde_json::Value>("models", &[], 25)
            .await
            .expect_err("wrong echoed page must fail");
        assert!(matches!(error, Error::Malformed { .. }), "{error:?}");
        assert!(error.to_string().contains("page 2"), "{error}");
    }

    #[tokio::test]
    async fn pagination_rejects_a_zero_page_size() {
        let (client, _transport) = scripted(
            vec![Ok(mock::ok_json(
                "u",
                json!({"data": [], "page": 1, "page_size": 0, "total": 0, "total_pages": 0})
                    .to_string(),
            ))],
            fast_retry(1),
        );
        let error = client
            .fetch_all_pages::<serde_json::Value>("models", &[], 25)
            .await
            .expect_err("page_size = 0 must fail");
        assert!(matches!(error, Error::Malformed { .. }), "{error:?}");
    }

    #[tokio::test]
    async fn pagination_aborts_instead_of_looping_forever() {
        // Every response claims 100,001 pages and echoes the requested page, so
        // only the hard cap can stop the walk.
        let transport = MockTransport::new(Arc::new(|request| {
            let page = page_query(request, "page")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1);
            Ok(mock::ok_json(
                "u",
                mock::paginated(&Vec::<serde_json::Value>::new(), page, 2, 200_002),
            ))
        }));
        let client = Client::new(
            Arc::new(transport),
            "https://api.test/v1",
            None,
            fast_retry(1),
        );
        let error = client
            .fetch_all_pages::<serde_json::Value>("models", &[], 2)
            .await
            .expect_err("the cap must stop the walk");
        assert!(matches!(error, Error::Config(_)), "{error:?}");
    }

    // -----------------------------------------------------------------
    // Error types
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn auth_failure_is_reported_and_never_retried() {
        for status in [401u16, 403u16] {
            let (client, transport) = scripted(
                vec![Ok(mock::response(
                    "u",
                    status,
                    json!({"error": "Unauthorized"}).to_string(),
                ))],
                fast_retry(5),
            );
            let error = client
                .get_raw("user", &[])
                .await
                .expect_err("auth must fail");
            match error {
                Error::Auth {
                    status: got,
                    message,
                } => {
                    assert_eq!(got, status);
                    assert_eq!(message, "Unauthorized");
                }
                other => panic!("expected Auth, got {other:?}"),
            }
            assert_eq!(
                transport.requests().len(),
                1,
                "auth failures are not retried"
            );
        }
    }

    #[tokio::test]
    async fn rate_limit_retries_with_backoff_then_succeeds() {
        let (client, transport) = scripted(
            vec![
                Ok(mock::response(
                    "u",
                    429,
                    json!({"error": "slow down"}).to_string(),
                )),
                Ok(mock::response(
                    "u",
                    429,
                    json!({"error": "slow down"}).to_string(),
                )),
                Ok(mock::ok_json("u", json!({"ok": true}).to_string())),
            ],
            fast_retry(4),
        );
        let started = Instant::now();
        let response = client
            .get_raw("user", &[])
            .await
            .expect("third attempt works");
        assert_eq!(response.status, 200);
        assert_eq!(transport.requests().len(), 3);
        // 1 ms + 2 ms of exponential backoff.
        assert!(started.elapsed() >= Duration::from_millis(3));
    }

    #[tokio::test]
    async fn rate_limit_exhausts_the_budget() {
        let (client, transport) = scripted(
            vec![
                Ok(mock::response(
                    "u",
                    429,
                    json!({"error": "slow down"}).to_string(),
                )),
                Ok(mock::response(
                    "u",
                    429,
                    json!({"error": "slow down"}).to_string(),
                )),
                Ok(mock::response(
                    "u",
                    429,
                    json!({"error": "slow down"}).to_string(),
                )),
            ],
            fast_retry(3),
        );
        let error = client
            .get_raw("user", &[])
            .await
            .expect_err("budget must run out");
        match error {
            Error::RateLimited {
                attempts,
                retry_after,
            } => {
                assert_eq!(attempts, 3);
                assert_eq!(retry_after, None);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
        assert_eq!(transport.requests().len(), 3);
    }

    #[tokio::test]
    async fn retry_after_header_delays_the_retry() {
        let rate_limited = mock::with_header(mock::response("u", 429, ""), "Retry-After", "1");
        let (client, transport) = scripted(
            vec![Ok(rate_limited), Ok(mock::ok_json("u", "{}"))],
            fast_retry(2),
        );
        let started = Instant::now();
        client
            .get_raw("user", &[])
            .await
            .expect("second attempt works");
        assert!(
            started.elapsed() >= Duration::from_millis(900),
            "Retry-After: 1 must be honoured, waited only {:?}",
            started.elapsed()
        );
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn malformed_json_is_reported() {
        let (client, _transport) =
            scripted(vec![Ok(mock::ok_json("u", "{not json"))], fast_retry(2));
        let error = client
            .get_json::<serde_json::Value>("models", &[])
            .await
            .expect_err("bad JSON must fail");
        assert!(matches!(error, Error::Malformed { .. }), "{error:?}");
        assert!(error.to_string().contains("malformed"), "{error}");
    }

    #[tokio::test]
    async fn empty_body_is_reported_as_malformed() {
        let (client, _transport) = scripted(vec![Ok(mock::ok_json("u", ""))], fast_retry(2));
        let error = client
            .get_json::<serde_json::Value>("models", &[])
            .await
            .expect_err("empty body must fail");
        assert!(matches!(error, Error::Malformed { .. }), "{error:?}");
    }

    #[tokio::test]
    async fn transport_errors_retry_then_succeed() {
        let (client, transport) = scripted(
            vec![
                Err(TransportError::Network("connection reset".to_string())),
                Ok(mock::ok_json("u", "{}")),
            ],
            fast_retry(3),
        );
        client
            .get_raw("user", &[])
            .await
            .expect("second attempt works");
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn transport_errors_exhaust_the_budget() {
        let (client, transport) = scripted(
            vec![
                Err(TransportError::Network("dns".to_string())),
                Err(TransportError::Network("dns".to_string())),
            ],
            fast_retry(2),
        );
        let error = client
            .get_raw("user", &[])
            .await
            .expect_err("budget must run out");
        match error {
            Error::RetryExhausted { url, attempts, .. } => {
                assert_eq!(attempts, 2);
                assert!(url.contains("/user"));
            }
            other => panic!("expected RetryExhausted, got {other:?}"),
        }
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn timeouts_retry_and_have_their_own_error_variant() {
        let (client, transport) = scripted(
            vec![
                Err(TransportError::Timeout(Duration::from_secs(30))),
                Ok(mock::ok_json("u", "{}")),
            ],
            fast_retry(2),
        );
        client
            .get_raw("user", &[])
            .await
            .expect("second attempt works");
        assert_eq!(transport.requests().len(), 2);
        assert_eq!(
            Error::Timeout(Duration::from_secs(30)).to_string(),
            "request timed out after 30s"
        );
    }

    #[tokio::test]
    async fn server_errors_retry_then_succeed() {
        let (client, transport) = scripted(
            vec![
                Ok(mock::response(
                    "u",
                    503,
                    json!({"error": "unavailable"}).to_string(),
                )),
                Ok(mock::ok_json("u", "{}")),
            ],
            fast_retry(2),
        );
        client
            .get_raw("user", &[])
            .await
            .expect("second attempt works");
        assert_eq!(transport.requests().len(), 2);
    }

    #[tokio::test]
    async fn server_errors_exhaust_as_http() {
        let (client, _transport) = scripted(
            vec![
                Ok(mock::response(
                    "u",
                    500,
                    json!({"error": "boom"}).to_string(),
                )),
                Ok(mock::response(
                    "u",
                    500,
                    json!({"error": "boom"}).to_string(),
                )),
            ],
            fast_retry(2),
        );
        let error = client
            .get_raw("user", &[])
            .await
            .expect_err("budget must run out");
        match error {
            Error::Http {
                status, message, ..
            } => {
                assert_eq!(status, 500);
                assert_eq!(message, "boom");
            }
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn client_errors_are_not_retried() {
        let (client, transport) = scripted(
            vec![Ok(mock::response(
                "u",
                400,
                json!({"error": "gear must be one of: amp"}).to_string(),
            ))],
            fast_retry(5),
        );
        let error = client
            .get_raw("tones/trending", &[("gear".into(), "bogus".into())])
            .await
            .expect_err("400 must fail");
        match error {
            Error::Http {
                status,
                message,
                url,
            } => {
                assert_eq!(status, 400);
                assert_eq!(message, "gear must be one of: amp");
                assert!(url.contains("gear=bogus"), "{url}");
            }
            other => panic!("expected Http, got {other:?}"),
        }
        assert_eq!(transport.requests().len(), 1, "4xx is not retried");
    }

    #[tokio::test]
    async fn config_errors_are_reported() {
        let error = Error::Config("pagination of models exceeded 100000 pages".to_string());
        assert!(error.to_string().contains("invalid configuration"));
    }

    #[tokio::test]
    async fn min_interval_throttles_requests() {
        let retry = RetryPolicy {
            max_attempts: 1,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            jitter: false,
            min_interval: Duration::from_millis(40),
        };
        let (client, transport) = scripted(
            vec![Ok(mock::ok_json("u", "{}")), Ok(mock::ok_json("u", "{}"))],
            retry,
        );
        let started = Instant::now();
        client.get_raw("user", &[]).await.expect("first");
        client.get_raw("user", &[]).await.expect("second");
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert_eq!(transport.requests().len(), 2);
    }

    // -----------------------------------------------------------------
    // Credentials
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn credentials_go_in_the_authorization_header_and_nowhere_else() {
        let token = "t3k-super-secret-value";
        let (client, transport) =
            scripted_with_credentials(vec![Ok(mock::ok_json("u", "{}"))], fast_retry(1), token);
        client
            .get_raw("user", &[("q".into(), "secret".into())])
            .await
            .expect("request succeeds");

        let request = &transport.requests()[0];
        assert_eq!(
            request.headers,
            vec![("Authorization".to_string(), format!("Bearer {token}"))]
        );
        assert!(!request.url.contains(token), "token leaked into the URL");
        assert!(!format!("{:?}", request.url).contains(token));
    }

    #[tokio::test]
    async fn error_messages_never_contain_the_credential() {
        let token = "t3k-super-secret-value";
        let (client, _transport) = scripted_with_credentials(
            vec![Ok(mock::response(
                "u",
                401,
                json!({"error": "Unauthorized"}).to_string(),
            ))],
            fast_retry(2),
            token,
        );
        let error = client
            .get_raw("user", &[])
            .await
            .expect_err("401 must fail");
        let rendered = error.to_string();
        assert!(!rendered.contains(token), "{rendered}");
        assert!(rendered.contains("401"), "{rendered}");
    }

    // -----------------------------------------------------------------
    // Feeds
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn tone_feed_parses_the_data_envelope() {
        let (client, _transport) = scripted(
            vec![Ok(mock::ok_json(
                "u",
                mock::tone_list(&[json!({"id": 106343, "title": "Mesa"})]),
            ))],
            fast_retry(1),
        );
        let tones = client
            .get_tone_feed("tones/trending", &[])
            .await
            .expect("feed parses");
        assert_eq!(tones.len(), 1);
        assert_eq!(tones[0].id, 106343);
        assert_eq!(tones[0].title.as_deref(), Some("Mesa"));
    }

    #[tokio::test]
    async fn production_transport_classifies_a_malformed_url() {
        let transport = ReqwestTransport::new(Duration::from_secs(5)).expect("client builds");
        let error = transport
            .send(Request {
                method: Method::Get,
                url: "not a url".to_string(),
                query: Vec::new(),
                headers: Vec::new(),
                body: None,
            })
            .await
            .expect_err("a malformed URL must fail");
        assert!(matches!(error, TransportError::Url(_)), "{error:?}");
        assert!(error.to_string().contains("invalid URL"), "{error}");
    }
}
