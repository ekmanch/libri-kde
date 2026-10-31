//! What the HTTP-backed lookups share: one blocking client per provider
//! with a rate limiter (a scroll through a cover view must never burst
//! past a provider's limit), the offline mapping, the status check, the
//! back-off on 429, JSON decoding and the URL building.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{Failure, FailureKind, Reply};

/// The default wait for an answer before a request counts as offline.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// How many times a request refused with 429 is sent again after a pause.
const RATE_LIMIT_RETRIES: u32 = 2;
/// The first pause without a `Retry-After`; the next doubles it.
const BACKOFF_BASE: Duration = Duration::from_secs(2);
/// A pause the provider asks for beyond this is not waited for.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// How much of an answer is read at most.
const BODY_LIMIT: u64 = 20 * 1024 * 1024;

/// Keeps at least `min_gap` between the starts of the requests run through
/// it, in the order they asked.
pub struct Limiter {
    min_gap: Duration,
    next_allowed: Mutex<Option<Instant>>,
}

impl Limiter {
    pub fn new(min_gap: Duration) -> Limiter {
        Limiter {
            min_gap,
            next_allowed: Mutex::new(None),
        }
    }

    /// Takes the next slot and waits for it.
    pub fn wait(&self) {
        let wait = {
            let mut next = self.next_allowed.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let start = next.map_or(now, |n| n.max(now));
            *next = Some(start + self.min_gap);
            start.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    /// Holds every request back for `duration` from now, whatever the gap:
    /// a provider that answered 429 asked for a pause.
    pub fn hold_for(&self, duration: Duration) {
        let until = Instant::now() + duration;
        let mut next = self.next_allowed.lock().unwrap_or_else(|e| e.into_inner());
        if next.is_none_or(|n| until > n) {
            *next = Some(until);
        }
    }
}

/// One provider's client: an agent, a limiter and which statuses mean
/// "rate limited" (Apple says 403 for it).
pub struct Http {
    agent: ureq::Agent,
    limiter: Limiter,
    rate_limited: &'static [u16],
}

impl Http {
    pub fn new(min_gap: Duration) -> Http {
        Http::with_rate_limit_statuses(min_gap, &[429])
    }

    pub fn with_rate_limit_statuses(min_gap: Duration, rate_limited: &'static [u16]) -> Http {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .user_agent("Libri")
            .build();
        Http {
            agent: config.into(),
            limiter: Limiter::new(min_gap),
            rate_limited,
        }
    }

    pub fn is_rate_limited(&self, status: u16) -> bool {
        self.rate_limited.contains(&status)
    }

    /// A GET with `headers`, under the limiter; a 429 pauses the whole
    /// source and tries again, twice at most.
    pub fn get(&self, url: &str, headers: &[(&str, &str)]) -> Reply<Response> {
        self.send(|| {
            let mut request = self.agent.get(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.call()
        })
    }

    /// A POST of `body` with `headers`.
    pub fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> Reply<Response> {
        self.send(|| {
            let mut request = self.agent.post(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.send(body)
        })
    }

    fn send(
        &self,
        request: impl Fn() -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Reply<Response> {
        for attempt in 0.. {
            self.limiter.wait();
            let response = Response::read(request().map_err(transport_failure)?)?;
            if !self.is_rate_limited(response.status) || attempt >= RATE_LIMIT_RETRIES {
                return Ok(response);
            }
            let pause = response
                .retry_after
                .unwrap_or(BACKOFF_BASE * (1 << attempt));
            if pause > MAX_BACKOFF {
                return Ok(response);
            }
            self.limiter.hold_for(pause);
        }
        unreachable!("the loop returns")
    }

    /// Throws unless the status is among `ok`: a rate limit status is
    /// rate limited, anything else "other" with the body's message.
    pub fn check(&self, response: &Response, ok: &[u16]) -> Reply<()> {
        if ok.contains(&response.status) {
            return Ok(());
        }
        let kind = if self.is_rate_limited(response.status) {
            FailureKind::RateLimited
        } else {
            FailureKind::Other
        };
        Err(Failure {
            kind,
            status: Some(response.status),
            message: response.error_message(),
        })
    }

    /// A GET that must answer 200 with a JSON object.
    pub fn get_object(&self, url: &str, headers: &[(&str, &str)]) -> Reply<Value> {
        let response = self.get(url, headers)?;
        self.check(&response, &[200])?;
        response.object()
    }
}

/// An answer: its status, body and the headers the lookups read.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub bytes: Vec<u8>,
    pub content_type: String,
    /// The `Retry-After` header as a pause, when it is in seconds.
    pub retry_after: Option<Duration>,
}

impl Response {
    fn read(mut response: ureq::http::Response<ureq::Body>) -> Reply<Response> {
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let content_type = header("content-type").unwrap_or_default();
        let retry_after = header("retry-after")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let bytes = response
            .body_mut()
            .with_config()
            .limit(BODY_LIMIT)
            .read_to_vec()
            .map_err(transport_failure)?;
        Ok(Response {
            status,
            bytes,
            content_type,
            retry_after,
        })
    }

    /// The body as JSON, or "other" when it is not.
    pub fn json(&self) -> Reply<Value> {
        serde_json::from_slice(&self.bytes).map_err(|e| Failure {
            kind: FailureKind::Other,
            status: Some(self.status),
            message: format!("not JSON: {e}"),
        })
    }

    /// The body as a JSON object.
    pub fn object(&self) -> Reply<Value> {
        let json = self.json()?;
        if json.is_object() {
            Ok(json)
        } else {
            Err(Failure {
                kind: FailureKind::Other,
                status: Some(self.status),
                message: "not a JSON object".into(),
            })
        }
    }

    /// The provider's `{"status_message"}`, `{"message"}` or `{"error"}`,
    /// or nothing.
    fn error_message(&self) -> String {
        let Ok(json) = serde_json::from_slice::<Value>(&self.bytes) else {
            return String::new();
        };
        for key in ["status_message", "message", "error"] {
            match &json[key] {
                Value::String(text) => return text.clone(),
                Value::Object(map) => {
                    if let Some(Value::String(text)) = map.get("message") {
                        return text.clone();
                    }
                }
                _ => {}
            }
        }
        String::new()
    }
}

/// No connection, a refused connection, a timeout or a broken socket are
/// all "offline"; anything else is a failure of its own.
pub fn transport_failure(error: ureq::Error) -> Failure {
    let kind = match &error {
        ureq::Error::Io(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Timeout(_) => FailureKind::Offline,
        _ => FailureKind::Other,
    };
    Failure::new(kind, error.to_string())
}

/// `base` with `query` appended, every value percent-encoded.
pub fn url(base: &str, query: &[(&str, &str)]) -> String {
    let mut url = base.to_owned();
    for (i, (key, value)) in query.iter().enumerate() {
        url.push(if i == 0 && !base.contains('?') {
            '?'
        } else {
            '&'
        });
        url.push_str(key);
        url.push('=');
        url.push_str(&encode(value));
    }
    url
}

/// Everything but the unreserved characters percent-encoded.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// Reading JSON the way the sources need it.

/// A non-empty string at `key`.
pub fn str_at<'a>(json: &'a Value, key: &str) -> Option<&'a str> {
    json.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// A whole number at `key`.
pub fn int_at(json: &Value, key: &str) -> Option<i64> {
    json.get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
}

/// The objects in the list at `key`, or none.
pub fn objects_at<'a>(json: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    json.get(key)
        .and_then(Value::as_array)
        .map(|items| items.as_slice())
        .unwrap_or_default()
        .iter()
        .filter(|item| item.is_object())
}

/// The strings in the list at `key`, or none.
pub fn strings_at<'a>(json: &'a Value, key: &str) -> impl Iterator<Item = &'a str> {
    json.get(key)
        .and_then(Value::as_array)
        .map(|items| items.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_encode_their_query_values() {
        assert_eq!(
            url(
                "https://api.example.org/3/search",
                &[("query", "The Mummy?"), ("page", "1")]
            ),
            "https://api.example.org/3/search?query=The%20Mummy%3F&page=1"
        );
        assert_eq!(url("https://x/y?a=1", &[("b", "2")]), "https://x/y?a=1&b=2");
    }

    #[test]
    fn json_helpers_read_what_is_there() {
        let json: Value = serde_json::from_str(
            r#"{"name": "", "id": 12.0, "list": [{"a": 1}, 3, {"b": 2}], "tags": ["x", 4, "y"]}"#,
        )
        .unwrap();
        assert_eq!(str_at(&json, "name"), None, "empty counts as missing");
        assert_eq!(int_at(&json, "id"), Some(12));
        assert_eq!(objects_at(&json, "list").count(), 2);
        assert_eq!(strings_at(&json, "tags").collect::<Vec<_>>(), ["x", "y"]);
        assert_eq!(objects_at(&json, "nope").count(), 0);
    }

    #[test]
    fn the_limiter_spaces_requests_and_holds_on_request() {
        let limiter = Limiter::new(Duration::from_millis(20));
        let start = Instant::now();
        limiter.wait();
        limiter.wait();
        assert!(start.elapsed() >= Duration::from_millis(20));
        limiter.hold_for(Duration::from_millis(40));
        let before = Instant::now();
        limiter.wait();
        assert!(before.elapsed() >= Duration::from_millis(35));
    }

    #[test]
    fn error_messages_come_from_the_known_keys() {
        let response = Response {
            status: 401,
            bytes: br#"{"status_message": "Invalid API key"}"#.to_vec(),
            content_type: String::new(),
            retry_after: None,
        };
        assert_eq!(response.error_message(), "Invalid API key");
        let http = Http::new(Duration::ZERO);
        let failure = http.check(&response, &[200]).unwrap_err();
        assert_eq!(
            (failure.kind, failure.status),
            (FailureKind::Other, Some(401))
        );
        let busy = Response {
            status: 429,
            ..response.clone()
        };
        assert_eq!(
            http.check(&busy, &[200]).unwrap_err().kind,
            FailureKind::RateLimited
        );
    }
}
