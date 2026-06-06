use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use reqwest::blocking::Client as HttpClient;
use serde::Deserialize;

use crate::model::{Tag, parse_version};

const MAX_PAGES: usize = 10;
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 6);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("github request failed with status {0}")]
    HttpStatus(u16),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

pub struct GitHubClient {
    http: HttpClient,
    base_url: String,
    token: Option<String>,
    cache_enabled: bool,
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new(true)
    }
}

impl GitHubClient {
    pub fn new(cache_enabled: bool) -> Self {
        let cache_enabled = cache_enabled && !no_cache_from_env();
        Self {
            http: HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            base_url: "https://api.github.com".into(),
            token: resolve_token(),
            cache_enabled,
        }
    }

    #[allow(dead_code)]
    pub fn new_for_test(cache_enabled: bool, base_url: String, token: Option<String>) -> Self {
        Self {
            http: HttpClient::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            base_url,
            token,
            cache_enabled,
        }
    }

    pub fn fetch_tags(&self, owner: &str, name: &str) -> Result<Vec<Tag>, Error> {
        let cache_path = cache_path(owner, name);

        if self.cache_enabled
            && let Some(tags) = read_cache(&cache_path)
        {
            return Ok(tags);
        }

        let url = format!("{}/repos/{owner}/{name}/tags?per_page=100", self.base_url);
        let tags = self.fetch_all_pages(&url)?;

        if self.cache_enabled {
            if let Some(parent) = cache_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let cached: Vec<CachedTag> = tags
                .iter()
                .map(|t| CachedTag {
                    name: t.name.clone(),
                    sha: t.sha.clone(),
                })
                .collect();
            if let Ok(json) = serde_json::to_string(&cached) {
                let _ = fs::write(&cache_path, json);
            }
        }

        Ok(tags)
    }

    fn fetch_all_pages(&self, start_url: &str) -> Result<Vec<Tag>, Error> {
        let mut tags = Vec::new();
        let mut url = Some(start_url.to_string());
        let mut pages = 0;

        while let Some(page_url) = url {
            if pages >= MAX_PAGES {
                break;
            }
            pages += 1;

            let mut req = self
                .http
                .get(&page_url)
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "actioneer")
                .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(token) = &self.token {
                req = req.bearer_auth(token);
            }

            let response = req.send()?;
            if !response.status().is_success() {
                return Err(Error::HttpStatus(response.status().as_u16()));
            }

            let next = next_link(response.headers().get("link"));
            let body: Vec<ApiTag> = response.json()?;
            tags.extend(body.into_iter().filter_map(|t| {
                Some(Tag {
                    name: t.name.clone(),
                    sha: t.commit.sha,
                    version: parse_version(&t.name)?,
                })
            }));

            url = next;
        }

        Ok(tags)
    }
}

#[derive(Deserialize)]
struct ApiTag {
    name: String,
    commit: ApiCommit,
}
#[derive(Deserialize)]
struct ApiCommit {
    sha: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct CachedTag {
    name: String,
    sha: String,
}

fn next_link(header: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    let value = header?.to_str().ok()?;
    for part in value.split(',') {
        let t = part.trim();
        if t.contains("rel=\"next\"") {
            let start = t.find('<')? + 1;
            let end = t.find('>')?;
            return Some(t[start..end].into());
        }
    }
    None
}

pub fn cache_path(owner: &str, name: &str) -> PathBuf {
    std::env::temp_dir()
        .join("actioneer-cache")
        .join("tags")
        .join(format!("{owner}__{name}.json"))
}

fn read_cache(path: &Path) -> Option<Vec<Tag>> {
    let meta = fs::metadata(path).ok()?;
    let age = SystemTime::now()
        .duration_since(meta.modified().ok()?)
        .ok()?;
    if age > CACHE_TTL {
        return None;
    }
    let contents = fs::read_to_string(path).ok()?;
    let cached: Vec<CachedTag> = serde_json::from_str(&contents).ok()?;
    Some(
        cached
            .into_iter()
            .filter_map(|t| {
                Some(Tag {
                    name: t.name.clone(),
                    sha: t.sha,
                    version: parse_version(&t.name)?,
                })
            })
            .collect(),
    )
}

fn resolve_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .and_then(|t| {
            let t = t.trim();
            (!t.is_empty()).then(|| t.to_string())
        })
        .or_else(|| {
            let out = std::process::Command::new("gh")
                .args(["auth", "token"])
                .stdin(std::process::Stdio::null())
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let token = String::from_utf8(out.stdout).ok()?;
            let t = token.trim();
            (!t.is_empty()).then(|| t.to_string())
        })
}

pub fn no_cache_from_env() -> bool {
    matches!(std::env::var("ACTIONEER_NO_CACHE"), Ok(v) if matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}
