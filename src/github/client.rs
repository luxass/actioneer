use std::time::Duration;

use reqwest::blocking::Client as HttpClient;
use serde::Deserialize;

use crate::actions::{Tag, parse_version};
use crate::github::cache::{cache_path, no_cache_from_env, read_cache, write_cache};

const MAX_PAGES: usize = 10;

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
            write_cache(&cache_path, &tags);
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
