//! The GitHub calls Libri makes, against one repository with a
//! fine-grained token: the same six as the phone's `github_http_api.dart`
//! (the repo, its branches, a file in the raw format, a save through the
//! Contents API, the JSON files in the tree, and a file's commits). The
//! client blocks; the app runs each call on tokio's blocking pool from an
//! iced Task (`app::sync_task`), so the window never waits on the network.

mod blob;

use std::sync::Mutex;
use std::time::Duration;

use base64::Engine as _;

pub use blob::blob_sha;

use crate::clock::Timestamp;

const API: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const RAW: &str = "application/vnd.github.raw+json";
const JSON: &str = "application/vnd.github+json";
/// The header GitHub adds to every answer made with a token that
/// expires, like "2025-11-10 16:20:42 +0500" or "2025-11-10 11:20:42 UTC".
const TOKEN_EXPIRATION: &str = "github-authentication-token-expiration";
/// How long one call may take in all.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The raw format works to 100 MB; a tree file is opened only up to 20 MB
/// (the setup's listing).
const RAW_LIMIT: u64 = 100 * 1024 * 1024;
/// How many branches `branches` returns at most: one page.
pub const MAX_BRANCHES: usize = 100;

/// Why a call failed, as far as the app cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// 401: the token is wrong or expired.
    Unauthorized,
    /// 403 for any reason but the rate limit: the token lacks a permission.
    Forbidden,
    /// 404: no such repo or file, or a token that cannot see the repo (a
    /// fine-grained token answers 404, not 403, for those).
    NotFound,
    /// 409 or 422 on a write: the file changed under us, or already exists.
    Conflict,
    /// 403 or 429 with the rate limit used up; try again later.
    RateLimited,
    /// No connection, or the server did not answer in time.
    Offline,
    /// Anything else, including a 5xx.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    /// The HTTP status, when there was a response.
    pub status: Option<u16>,
    pub message: String,
}

impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Failure {
        Failure {
            kind,
            status: None,
            message: message.into(),
        }
    }

    /// The failure in the setup's words, as on the phone.
    pub fn describe(&self) -> String {
        match self.kind {
            FailureKind::Unauthorized => "GitHub refused the token. Check it and try again.".into(),
            FailureKind::NotFound => "Repository not found, or the token cannot see it.".into(),
            FailureKind::Forbidden => "The token is not allowed to use this repo.".into(),
            FailureKind::RateLimited => {
                "GitHub is rate limiting the token. Try again in a few minutes.".into()
            }
            FailureKind::Offline => "No connection. Check your network and try again.".into(),
            FailureKind::Conflict | FailureKind::Other => format!(
                "GitHub answered with an error{}. Try again.",
                self.status.map(|s| format!(" ({s})")).unwrap_or_default()
            ),
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.kind)?;
        if let Some(status) = self.status {
            write!(f, " {status}")?;
        }
        if !self.message.is_empty() {
            write!(f, ": {}", self.message)?;
        }
        Ok(())
    }
}

/// What `GET /repos/{owner}/{repo}` tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub default_branch: String,
    /// Whether the token may write to the repo.
    pub can_push: bool,
}

/// A file downloaded in the raw format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFile {
    pub bytes: Vec<u8>,
    /// The response's ETag, for the next conditional request. Opaque.
    pub etag: Option<String>,
    /// The sha git gives these bytes, computed locally.
    pub blob_sha: String,
}

impl RawFile {
    pub fn new(bytes: Vec<u8>, etag: Option<String>) -> RawFile {
        let blob_sha = blob_sha(&bytes);
        RawFile {
            bytes,
            etag,
            blob_sha,
        }
    }

    /// The text, decoded as UTF-8: the raw media type carries no charset.
    pub fn text(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.bytes)
    }
}

/// The result of a successful write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutResult {
    pub blob_sha: String,
    pub commit_sha: String,
}

/// A blob in the repo's tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFile {
    pub path: String,
    pub size: u64,
    pub sha: String,
}

/// A commit, from the commits list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub date: Timestamp,
}

/// A branch, from the branches list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    /// When the commit at its head was made; `None` when not known.
    pub updated: Option<Timestamp>,
}

/// The REST API with a fine-grained token, against "owner/repo".
pub struct Client {
    agent: ureq::Agent,
    token: String,
    repository: String,
    /// When the token expires, as the last successful answer said; a
    /// refused request says nothing about it.
    expiry: Mutex<Option<Timestamp>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("repository", &self.repository)
            .finish_non_exhaustive()
    }
}

type Reply<T> = Result<T, Failure>;

impl Client {
    pub fn new(token: impl Into<String>, repository: impl Into<String>) -> Client {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .user_agent("Libri")
            .build();
        Client {
            agent: config.into(),
            token: token.into(),
            repository: repository.into(),
            expiry: Mutex::new(None),
        }
    }

    /// "owner/repo".
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// When the token expires, from the expiration header on the last
    /// successful answer; `None` until one said, or for a token that
    /// never expires.
    pub fn token_expiry(&self) -> Option<Timestamp> {
        *self.expiry.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn url(&self, segments: &[&str], query: &[(&str, &str)]) -> String {
        let mut url = format!("{API}/repos/{}", self.repository);
        for segment in segments {
            url.push('/');
            url.push_str(&encode_segment(segment));
        }
        for (i, (key, value)) in query.iter().enumerate() {
            url.push(if i == 0 { '?' } else { '&' });
            url.push_str(key);
            url.push('=');
            url.push_str(&encode_segment(value));
        }
        url
    }

    /// Runs one request; transport trouble is `Offline`, a status outside
    /// `ok` a failure named by it. The body comes back whole.
    fn send(
        &self,
        request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
        ok: &[u16],
    ) -> Reply<Response> {
        self.finish(request.call(), ok)
    }

    fn send_body(
        &self,
        request: ureq::RequestBuilder<ureq::typestate::WithBody>,
        body: &str,
        ok: &[u16],
    ) -> Reply<Response> {
        self.finish(request.send(body), ok)
    }

    fn finish(
        &self,
        result: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
        ok: &[u16],
    ) -> Reply<Response> {
        let mut response = result.map_err(transport_failure)?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let etag = header("etag");
        let remaining = header("x-ratelimit-remaining");
        let expiration = header(TOKEN_EXPIRATION);
        let bytes = response
            .body_mut()
            .with_config()
            .limit(RAW_LIMIT)
            .read_to_vec()
            .map_err(transport_failure)?;
        if status < 400
            && let Some(expires) = expiration.as_deref().and_then(Timestamp::parse)
        {
            *self.expiry.lock().unwrap_or_else(|e| e.into_inner()) = Some(expires);
        }
        if ok.contains(&status) {
            return Ok(Response {
                status,
                bytes,
                etag,
            });
        }
        let kind = match status {
            401 => FailureKind::Unauthorized,
            403 if remaining.as_deref() == Some("0") => FailureKind::RateLimited,
            429 => FailureKind::RateLimited,
            403 => FailureKind::Forbidden,
            404 => FailureKind::NotFound,
            409 | 422 => FailureKind::Conflict,
            _ => FailureKind::Other,
        };
        Err(Failure {
            kind,
            status: Some(status),
            message: error_message(&bytes),
        })
    }

    fn get(&self, url: &str, accept: &str) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
        self.agent
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", accept)
            .header("X-GitHub-Api-Version", API_VERSION)
    }

    /// The repo's default branch and whether the token may push.
    pub fn repo(&self) -> Reply<RepoInfo> {
        let response = self.send(self.get(&self.url(&[], &[]), JSON), &[200])?;
        let json = response.json()?;
        Ok(RepoInfo {
            default_branch: json["default_branch"].as_str().unwrap_or("main").to_owned(),
            can_push: json["permissions"]["push"].as_bool().unwrap_or(false),
        })
    }

    /// The repo's branches (up to `MAX_BRANCHES`), in the order GitHub
    /// lists them, each with when its head was committed. The list
    /// carries only the head's sha; its date is one more request per
    /// branch, made a few at a time.
    pub fn branches(&self) -> Reply<Vec<BranchInfo>> {
        let per_page = MAX_BRANCHES.to_string();
        let url = self.url(&["branches"], &[("per_page", &per_page)]);
        let list = self.send(self.get(&url, JSON), &[200])?.json()?;
        let heads: Vec<(String, String)> = list
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some((
                            item["name"].as_str()?.to_owned(),
                            item["commit"]["sha"].as_str()?.to_owned(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut branches = Vec::with_capacity(heads.len());
        for chunk in heads.chunks(8) {
            let dates: Vec<Reply<Option<Timestamp>>> = std::thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|(_, sha)| scope.spawn(move || self.commit_date(sha)))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap_or(Ok(None)))
                    .collect()
            });
            for ((name, _), date) in chunk.iter().zip(dates) {
                branches.push(BranchInfo {
                    name: name.clone(),
                    updated: date?,
                });
            }
        }
        Ok(branches)
    }

    /// The committer date of one commit.
    fn commit_date(&self, sha: &str) -> Reply<Option<Timestamp>> {
        let url = self.url(&["commits", sha], &[]);
        let json = self.send(self.get(&url, JSON), &[200])?.json()?;
        Ok(commit_date(&json))
    }

    /// The file in the raw format. With `etag` the request is conditional,
    /// and `None` means "unchanged" (304).
    pub fn file_raw(&self, path: &str, branch: &str, etag: Option<&str>) -> Reply<Option<RawFile>> {
        let segments: Vec<&str> = std::iter::once("contents").chain(path.split('/')).collect();
        let url = self.url(&segments, &[("ref", branch)]);
        let mut request = self.get(&url, RAW);
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let response = self.send(request, &[200, 304])?;
        if response.status == 304 {
            return Ok(None);
        }
        Ok(Some(RawFile::new(response.bytes, response.etag)))
    }

    /// Creates (`sha` `None`) or replaces the file in one commit.
    /// Replacing needs the blob sha of the file being replaced; a stale
    /// sha, or a create on an existing file, fails with `Conflict`.
    pub fn put_file(
        &self,
        path: &str,
        branch: &str,
        bytes: &[u8],
        message: &str,
        sha: Option<&str>,
    ) -> Reply<PutResult> {
        let segments: Vec<&str> = std::iter::once("contents").chain(path.split('/')).collect();
        let url = self.url(&segments, &[]);
        let mut body = serde_json::json!({
            "message": message,
            "content": base64::engine::general_purpose::STANDARD.encode(bytes),
            "branch": branch,
        });
        if let Some(sha) = sha {
            body["sha"] = serde_json::Value::String(sha.to_owned());
        }
        let request = self
            .agent
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", JSON)
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("Content-Type", "application/json");
        let response = self.send_body(request, &body.to_string(), &[200, 201])?;
        let json = response.json()?;
        let field = |value: &serde_json::Value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| Failure::new(FailureKind::Other, "the answer names no sha"))
        };
        Ok(PutResult {
            blob_sha: field(&json["content"]["sha"])?,
            commit_sha: field(&json["commit"]["sha"])?,
        })
    }

    /// Every blob on `branch` whose path ends in `suffix`.
    pub fn files(&self, branch: &str, suffix: &str) -> Reply<Vec<TreeFile>> {
        let url = self.url(&["git", "trees", branch], &[("recursive", "1")]);
        let json = self.send(self.get(&url, JSON), &[200])?.json()?;
        let tree = json["tree"].as_array().cloned().unwrap_or_default();
        Ok(tree
            .iter()
            .filter(|item| item["type"].as_str() == Some("blob"))
            .filter_map(|item| {
                let path = item["path"].as_str()?;
                if !path.ends_with(suffix) {
                    return None;
                }
                Some(TreeFile {
                    path: path.to_owned(),
                    size: item["size"].as_u64().unwrap_or(0),
                    sha: item["sha"].as_str().unwrap_or_default().to_owned(),
                })
            })
            .collect())
    }

    /// The newest commits that touched `path` on `branch`, newest first.
    pub fn commits(&self, path: &str, branch: &str, limit: usize) -> Reply<Vec<CommitInfo>> {
        let per_page = limit.to_string();
        let url = self.url(
            &["commits"],
            &[("path", path), ("sha", branch), ("per_page", &per_page)],
        );
        let json = self.send(self.get(&url, JSON), &[200])?.json()?;
        Ok(json
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some(CommitInfo {
                            sha: item["sha"].as_str()?.to_owned(),
                            message: item["commit"]["message"]
                                .as_str()
                                .unwrap_or_default()
                                .to_owned(),
                            date: commit_date(item)?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// A successful answer: its status, body and ETag.
struct Response {
    status: u16,
    bytes: Vec<u8>,
    etag: Option<String>,
}

impl Response {
    fn json(&self) -> Reply<serde_json::Value> {
        serde_json::from_slice(&self.bytes)
            .map_err(|e| Failure::new(FailureKind::Other, format!("the answer is not JSON: {e}")))
    }
}

/// The committer date of a commit object from the commits API.
fn commit_date(item: &serde_json::Value) -> Option<Timestamp> {
    Timestamp::parse(item["commit"]["committer"]["date"].as_str()?)
}

/// GitHub's `{"message": "…"}`, or nothing.
fn error_message(bytes: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|json| json["message"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// No connection, a refused connection, a timeout or a broken socket are
/// all "offline" to the app; anything else is a failure of its own.
fn transport_failure(error: ureq::Error) -> Failure {
    let kind = match &error {
        ureq::Error::Io(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Timeout(_) => FailureKind::Offline,
        _ => FailureKind::Other,
    };
    Failure::new(kind, error.to_string())
}

/// A path segment or query value with everything but the unreserved
/// characters percent-encoded.
fn encode_segment(text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_name_the_repo_and_encode_the_rest() {
        let client = Client::new("t", "ekmanch/library");
        assert_eq!(
            client.url(
                &["contents", "backup", "library 2025.json"],
                &[("ref", "dev/x")]
            ),
            "https://api.github.com/repos/ekmanch/library/contents/backup/library%202025.json?ref=dev%2Fx"
        );
        assert_eq!(
            client.url(&[], &[]),
            "https://api.github.com/repos/ekmanch/library"
        );
    }

    #[test]
    fn failures_are_described_in_the_setups_words() {
        assert_eq!(
            Failure::new(FailureKind::Offline, "").describe(),
            "No connection. Check your network and try again."
        );
        let other = Failure {
            kind: FailureKind::Other,
            status: Some(502),
            message: "Bad gateway".into(),
        };
        assert_eq!(
            other.describe(),
            "GitHub answered with an error (502). Try again."
        );
        assert_eq!(other.to_string(), "Other 502: Bad gateway");
    }

    #[test]
    fn a_raw_file_knows_its_blob_sha() {
        let file = RawFile::new(b"hello\n".to_vec(), Some("\"x\"".into()));
        assert_eq!(file.blob_sha, "ce013625030ba8dba906f756967f9e9ca394464a");
        assert_eq!(file.text().unwrap(), "hello\n");
    }
}
