//! GitHub API client with disk caching.
//!
//! Resolves `owner/repo@git_ref` to a commit SHA. Also fetches the release
//! publication date for future `min-release-age` filtering.
//!
//! # Cache policy
//!
//! | `offline` | `no_cache` | Behaviour |
//! |-----------|-----------|-----------|
//! | `false`   | `false`   | Read cache first; fetch on miss, write back |
//! | `true`    | `false`   | Cache read only; [`GitHubError::Offline`] on miss |
//! | `false`   | `true`    | Network only; no cache reads or writes |
//!
//! # Usage
//!
//! ```rust,no_run
//! use actioneer::config::ActioneerConfig;
//! use actioneer::cache::cache_dir;
//! use actioneer::github::GitHubClient;
//!
//! let config = ActioneerConfig::default();
//! let resolved = GitHubClient::new(&config, cache_dir())
//!     .resolve_ref("actions", "checkout", "v4")?;
//! println!("{}", resolved.sha);
//! # Ok::<(), actioneer::github::GitHubError>(())
//! ```

mod cache;

use std::{fmt, io};

use serde::{Deserialize, Serialize};

use crate::cache::CacheDir;
use crate::config::ActioneerConfig;

pub use cache::CacheEntry;

/// The kind of git reference that was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum RefKind {
    /// A version tag (e.g. `v4`, `v1.2.3`).
    Tag,
    /// A branch name (e.g. `main`, `feature/foo`).
    Branch,
    /// The input was already a full 40-character SHA — no lookup was performed.
    Sha,
}

impl fmt::Display for RefKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tag => write!(f, "tag"),
            Self::Branch => write!(f, "branch"),
            Self::Sha => write!(f, "sha"),
        }
    }
}

/// A GitHub release entry used for update planning.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub published_at: String,
    pub prerelease: bool,
}

/// Cached list of releases for a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasesIndex {
    pub releases: Vec<Release>,
    pub fetched_at: u64,
}

/// A resolved git reference.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedRef {
    /// Full 40-character commit SHA.
    pub sha: String,
    /// How the input ref was classified.
    pub ref_kind: RefKind,
    /// ISO 8601 publication timestamp from the GitHub Releases API, if available.
    ///
    /// `None` when the ref is a branch, a direct SHA, or the tag has no GitHub
    /// Release entry.
    pub published_at: Option<String>,
}

/// Errors produced by the GitHub client.
#[derive(Debug)]
pub enum GitHubError {
    /// Offline mode is active and there is no cached entry for this request.
    Offline,
    /// The server returned an unexpected HTTP status.
    Http {
        /// HTTP status code.
        status: u16,
        /// Brief description (may be empty).
        message: String,
    },
    /// GitHub API rate limit exceeded (HTTP 403 or 429).
    RateLimited,
    /// The requested ref does not exist on GitHub (HTTP 404).
    NotFound {
        owner: String,
        repo: String,
        git_ref: String,
    },
    /// Failed to read a cache entry from disk.
    CacheRead(io::Error),
    /// Failed to write a cache entry to disk.
    CacheWrite(io::Error),
    /// JSON (de)serialisation error.
    Json(serde_json::Error),
    /// Low-level transport / TLS error from the HTTP client.
    Transport(String),
}

impl fmt::Display for GitHubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline => write!(f, "offline mode: no cached response available"),
            Self::Http { status, message } if message.is_empty() => {
                write!(f, "GitHub API error (HTTP {status})")
            }
            Self::Http { status, message } => {
                write!(f, "GitHub API error (HTTP {status}): {message}")
            }
            Self::RateLimited => write!(f, "GitHub API rate limit exceeded"),
            Self::NotFound {
                owner,
                repo,
                git_ref,
            } => write!(f, "{owner}/{repo}@{git_ref}: ref not found on GitHub"),
            Self::CacheRead(e) => write!(f, "cache read error: {e}"),
            Self::CacheWrite(e) => write!(f, "cache write error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Transport(s) => write!(f, "transport error: {s}"),
        }
    }
}

impl std::error::Error for GitHubError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CacheRead(e) | Self::CacheWrite(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for GitHubError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

// --- Internal GitHub API response types ---

/// `GET /repos/{owner}/{repo}/git/ref/{ref}` response.
#[derive(Deserialize)]
struct GitRefResponse {
    object: GitRefObject,
}

#[derive(Deserialize)]
struct GitRefObject {
    sha: String,
    /// `"commit"` for lightweight tags and branches; `"tag"` for annotated tags.
    #[serde(rename = "type")]
    kind: String,
}

/// `GET /repos/{owner}/{repo}/git/tags/{sha}` response (annotated-tag dereference).
#[derive(Deserialize)]
struct GitTagResponse {
    object: GitTagInner,
}

#[derive(Deserialize)]
struct GitTagInner {
    /// The commit SHA that the annotated tag points to.
    sha: String,
}

/// `GET /repos/{owner}/{repo}/releases` response (partial).
#[derive(Deserialize)]
struct ReleaseListItem {
    tag_name: String,
    published_at: Option<String>,
    prerelease: bool,
}

/// `GET /repos/{owner}/{repo}/releases/tags/{tag}` response (partial).
#[derive(Deserialize)]
struct ReleaseResponse {
    published_at: Option<String>,
}

// --- Client ---

/// A GitHub API client with configurable caching behaviour.
///
/// Construct via [`GitHubClient::new`]. The underlying HTTP agent pools
/// connections across calls and can be cheaply cloned.
///
/// Authentication is read from the `GITHUB_TOKEN` environment variable when
/// the client is constructed; the token is sent as a `Bearer` credential.
#[derive(Clone)]
pub struct GitHubClient {
    agent: ureq::Agent,
    offline: bool,
    no_cache: bool,
    cache: Option<CacheDir>,
    token: Option<String>,
    base_url: String,
}

impl GitHubClient {
    /// Create a new client from `config` and an optional cache directory.
    ///
    /// `GITHUB_TOKEN` is read from the process environment.
    pub fn new(config: &ActioneerConfig, cache: Option<CacheDir>) -> Self {
        Self::build(
            config.offline,
            config.no_cache,
            cache,
            "https://api.github.com".to_string(),
        )
    }

    /// Override the API base URL (useful for testing against a local fixture server).
    ///
    /// The URL must not have a trailing slash.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn build(offline: bool, no_cache: bool, cache: Option<CacheDir>, base_url: String) -> Self {
        Self {
            agent: ureq::Agent::new_with_defaults(),
            offline,
            no_cache,
            cache,
            token: std::env::var("GITHUB_TOKEN").ok(),
            base_url,
        }
    }

    /// Resolve `git_ref` for `owner/repo` to a commit SHA.
    ///
    /// Returns immediately for full 40-hex-char SHAs (no I/O). For tags the
    /// `GET /repos/{owner}/{repo}/git/ref/tags/{tag}` endpoint is used, with
    /// automatic dereferencing of annotated tags. Release dates are fetched
    /// best-effort and cached independently — their absence does not fail the
    /// call.
    ///
    /// # Cache policy
    ///
    /// | `offline` | `no_cache` | Behaviour |
    /// |-----------|-----------|-----------|
    /// | `false`   | `false`   | Read cache; fetch + write on miss |
    /// | `true`    | `false`   | Cache read only; [`GitHubError::Offline`] on miss |
    /// | `false`   | `true`    | Network only; no cache reads or writes |
    pub fn resolve_ref(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
    ) -> Result<ResolvedRef, GitHubError> {
        if is_full_sha(git_ref) {
            return Ok(ResolvedRef {
                sha: git_ref.to_string(),
                ref_kind: RefKind::Sha,
                published_at: None,
            });
        }

        let ref_kind = classify_ref(git_ref);

        if !self.no_cache
            && let Some(entry) = self.read_cached(owner, repo, git_ref, ref_kind)?
        {
            let resolved_kind = entry.ref_kind();
            return Ok(ResolvedRef {
                sha: entry.sha,
                ref_kind: resolved_kind,
                published_at: entry.published_at,
            });
        }

        if self.offline {
            return Err(GitHubError::Offline);
        }

        let sha = self.api_resolve_sha(owner, repo, git_ref, ref_kind)?;

        let published_at = if ref_kind == RefKind::Tag {
            self.api_release_date(owner, repo, git_ref).unwrap_or(None)
        } else {
            None
        };

        if !self.no_cache {
            self.write_cached(owner, repo, git_ref, ref_kind, &sha, published_at.as_deref())?;
        }

        Ok(ResolvedRef {
            sha,
            ref_kind,
            published_at,
        })
    }

    /// List GitHub releases for `owner/repo`, newest first.
    ///
    /// Results are cached at `{cache}/github/{owner}/{repo}/releases/index.json`
    /// using the same offline/no_cache policy as [`Self::resolve_ref`].
    pub fn list_releases(&self, owner: &str, repo: &str) -> Result<Vec<Release>, GitHubError> {
        if !self.no_cache
            && let Some(index) = self.read_releases_cache(owner, repo)?
        {
            return Ok(index.releases);
        }

        if self.offline {
            return Err(GitHubError::Offline);
        }

        let releases = self.api_list_releases(owner, repo)?;

        if !self.no_cache {
            self.write_releases_cache(owner, repo, &releases)?;
        }

        Ok(releases)
    }

    fn read_releases_cache(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Option<ReleasesIndex>, GitHubError> {
        let Some(cache) = &self.cache else {
            return Ok(None);
        };
        let path = cache::releases_index_path(cache, owner, repo);
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(GitHubError::CacheRead(e)),
        };
        let index: ReleasesIndex = serde_json::from_slice(&data).map_err(GitHubError::Json)?;
        Ok(Some(index))
    }

    fn write_releases_cache(
        &self,
        owner: &str,
        repo: &str,
        releases: &[Release],
    ) -> Result<(), GitHubError> {
        let Some(cache) = &self.cache else {
            return Ok(());
        };
        let path = cache::releases_index_path(cache, owner, repo);
        let index = ReleasesIndex {
            releases: releases.to_vec(),
            fetched_at: cache::now_secs(),
        };
        cache::write_releases_index(&path, &index)
    }

    fn api_list_releases(&self, owner: &str, repo: &str) -> Result<Vec<Release>, GitHubError> {
        let mut all = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/repos/{owner}/{repo}/releases?per_page=100&page={page}",
                self.base_url
            );
            let batch: Vec<ReleaseListItem> = self.get_json(&url, || GitHubError::NotFound {
                owner: owner.to_string(),
                repo: repo.to_string(),
                git_ref: String::new(),
            })?;

            if batch.is_empty() {
                break;
            }

            let batch_len = batch.len();
            for item in batch {
                if let Some(published_at) = item.published_at {
                    all.push(Release {
                        tag_name: item.tag_name,
                        published_at,
                        prerelease: item.prerelease,
                    });
                }
            }

            if batch_len < 100 {
                break;
            }
            page += 1;
            if page > 10 {
                break;
            }
        }

        Ok(all)
    }

    // --- Private: cache helpers ---

    fn read_cached(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
        ref_kind: RefKind,
    ) -> Result<Option<CacheEntry>, GitHubError> {
        let Some(cache) = &self.cache else {
            return Ok(None);
        };
        let kind_dir = ref_kind_dir(ref_kind);
        let path = cache::ref_path(cache, owner, repo, kind_dir, git_ref);
        cache::read_entry(&path)
    }

    fn write_cached(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
        ref_kind: RefKind,
        sha: &str,
        published_at: Option<&str>,
    ) -> Result<(), GitHubError> {
        let Some(cache) = &self.cache else {
            return Ok(());
        };
        let kind_dir = ref_kind_dir(ref_kind);
        let path = cache::ref_path(cache, owner, repo, kind_dir, git_ref);
        let entry = CacheEntry {
            sha: sha.to_string(),
            ref_kind: ref_kind.to_string(),
            published_at: published_at.map(str::to_string),
            fetched_at: cache::now_secs(),
        };
        cache::write_entry(&path, &entry)
    }

    // --- Private: API calls ---

    fn api_resolve_sha(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
        ref_kind: RefKind,
    ) -> Result<String, GitHubError> {
        let ref_path = match ref_kind {
            RefKind::Tag => format!("tags/{git_ref}"),
            RefKind::Branch => format!("heads/{git_ref}"),
            RefKind::Sha => unreachable!("full SHA refs are short-circuited in resolve_ref"),
        };
        let url = format!("{}/repos/{owner}/{repo}/git/ref/{ref_path}", self.base_url);
        let resp: GitRefResponse = self.get_json(&url, || GitHubError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref: git_ref.to_string(),
        })?;

        // Annotated tag: dereference the tag object to reach the commit SHA.
        if resp.object.kind == "tag" {
            let tag_url = format!(
                "{}/repos/{owner}/{repo}/git/tags/{}",
                self.base_url, resp.object.sha
            );
            let tag_resp: GitTagResponse = self.get_json(&tag_url, || GitHubError::NotFound {
                owner: owner.to_string(),
                repo: repo.to_string(),
                git_ref: git_ref.to_string(),
            })?;
            return Ok(tag_resp.object.sha);
        }

        Ok(resp.object.sha)
    }

    /// Fetch the release `published_at` for `tag`.
    ///
    /// Returns `Ok(None)` when the tag has no GitHub Release entry (HTTP 404).
    fn api_release_date(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
    ) -> Result<Option<String>, GitHubError> {
        let url = format!(
            "{}/repos/{owner}/{repo}/releases/tags/{tag}",
            self.base_url
        );
        match self.get_json::<ReleaseResponse>(&url, || GitHubError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref: tag.to_string(),
        }) {
            Ok(r) => Ok(r.published_at),
            Err(GitHubError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Perform a `GET` request to `url` and deserialize the JSON response body.
    ///
    /// `not_found_err` is called to construct the error when the server responds
    /// with HTTP 404. Rate-limit responses (403/429) map to
    /// [`GitHubError::RateLimited`].
    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: &str,
        not_found_err: impl FnOnce() -> GitHubError,
    ) -> Result<T, GitHubError> {
        let req = self
            .agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header(
                "User-Agent",
                &format!("actioneer/{}", crate::VERSION),
            )
            .header("X-GitHub-Api-Version", "2022-11-28");

        let req = if let Some(token) = &self.token {
            req.header("Authorization", &format!("Bearer {token}"))
        } else {
            req
        };

        match req.call() {
            Ok(mut resp) => resp.body_mut().read_json().map_err(map_body_error),
            Err(ureq::Error::StatusCode(404)) => Err(not_found_err()),
            Err(ureq::Error::StatusCode(403) | ureq::Error::StatusCode(429)) => {
                Err(GitHubError::RateLimited)
            }
            Err(ureq::Error::StatusCode(code)) => Err(GitHubError::Http {
                status: code,
                message: String::new(),
            }),
            Err(e) => Err(GitHubError::Transport(e.to_string())),
        }
    }
}

/// Map a body-read [`ureq::Error`] to a [`GitHubError`].
fn map_body_error(e: ureq::Error) -> GitHubError {
    match e {
        ureq::Error::Json(je) => GitHubError::Json(je),
        e => GitHubError::Transport(e.to_string()),
    }
}

/// Returns `true` if `s` looks like a full 40-character SHA-1 hash.
fn is_full_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Classify a git ref string as [`RefKind::Tag`] or [`RefKind::Branch`].
///
/// Mirrors the heuristic in `engine/reference.rs` but returns the GitHub
/// client's own [`RefKind`]:
/// - Starts with `v` followed by an ASCII digit → [`RefKind::Tag`]
/// - Everything else → [`RefKind::Branch`]
///
/// Full SHAs are handled before this function is called (see [`is_full_sha`]).
fn classify_ref(s: &str) -> RefKind {
    if s.starts_with('v') && s.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
        RefKind::Tag
    } else {
        RefKind::Branch
    }
}

/// Map a [`RefKind`] to its cache subdirectory name (`"tags"` or `"heads"`).
fn ref_kind_dir(kind: RefKind) -> &'static str {
    match kind {
        RefKind::Tag => "tags",
        RefKind::Branch | RefKind::Sha => "heads",
    }
}
