//! Test-only mock transport shared by the client and index unit tests.
//!
//! Not compiled into the library: `lib.rs` gates the module behind
//! `#[cfg(test)]`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;

use crate::client::{Request, Response, Transport, TransportError};

/// A scripted response: either an HTTP response or a transport-level failure.
pub type Scripted = Result<Response, TransportError>;

pub type Handler = Arc<dyn Fn(&Request) -> Result<Response, TransportError> + Send + Sync>;

#[derive(Clone)]
pub struct MockTransport {
    handler: Handler,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl MockTransport {
    pub fn new(handler: Handler) -> Self {
        Self {
            handler,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Replay `responses` in order. Running out is an error, so a client that
    /// makes more requests than the test scripted fails loudly.
    pub fn script(responses: Vec<Scripted>) -> Self {
        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        Self::new(Arc::new(move |request| {
            let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
            match queue.pop_front() {
                Some(scripted) => scripted,
                None => Err(TransportError::Network(format!(
                    "mock transport ran out of scripted responses for {}",
                    request.url
                ))),
            }
        }))
    }

    /// Every request the client sent, in order.
    pub fn requests(&self) -> Vec<Request> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request.clone());
        (self.handler)(&request)
    }
}

pub fn response(url: &str, status: u16, body: impl Into<String>) -> Response {
    Response {
        url: url.to_string(),
        status,
        headers: Vec::new(),
        body: body.into(),
    }
}

pub fn ok_json(url: &str, body: impl Into<String>) -> Response {
    response(url, 200, body)
}

pub fn with_header(mut response: Response, name: &str, value: &str) -> Response {
    response
        .headers
        .push((name.to_string(), value.to_string()));
    response
}

/// Serialize a body from a JSON value.
pub fn json_body(value: &serde_json::Value) -> String {
    serde_json::to_string(value).expect("test JSON serializes")
}

/// Build a `{data, page, page_size, total, total_pages}` envelope.
pub fn paginated<T: Serialize>(data: &[T], page: u32, page_size: u32, total: u64) -> String {
    let total_pages = if page_size == 0 {
        0
    } else {
        total.div_ceil(page_size as u64) as u32
    };
    json_body(&serde_json::json!({
        "data": data,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total_pages,
    }))
}

/// Build a non-paginated `{data: [...]}` envelope (trending/latest).
pub fn tone_list<T: Serialize>(data: &[T]) -> String {
    json_body(&serde_json::json!({ "data": data }))
}
