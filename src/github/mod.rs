use std::time::SystemTime;

mod cache;

use cache::{cache_file_path, no_cache_from_env, read_cached_tags, write_cached_tags};
use reqwest::StatusCode;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::{ETAG, IF_NONE_MATCH, LINK};
use serde::Deserialize;
use thiserror::Error;

use crate::engine::git::{Version, parse_version};
use crate::model::Repository;

const MAX_PAGES: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tag {
    pub name: String,
    pub sha: String,
    pub version: Version,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("github request failed")]
    HttpStatus(u16),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

pub struct Client {
    http: HttpClient,
    token: Option<String>,
    no_cache: bool,
}

impl Default for Client {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Client {
    pub fn new(no_cache: bool) -> Self {
        Self {
            http: HttpClient::builder().build().expect("reqwest client"),
            token: resolve_token(),
            no_cache: no_cache || no_cache_from_env(),
        }
    }

    pub fn fetch_tags(&self, repository: &Repository) -> Result<Vec<Tag>, Error> {
        let cache_path = cache_file_path(repository);
        let cached = (!self.no_cache)
            .then(|| read_cached_tags(&cache_path))
            .flatten();

        let base_url = format!(
            "https://api.github.com/repos/{}/{}/tags?per_page=100",
            repository.owner, repository.name
        );
        let mut request = self
            .http
            .get(&base_url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "actioneer")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(entry) = &cached
            && let Some(etag) = &entry.etag
        {
            request = request.header(IF_NONE_MATCH, etag);
        }

        let response = request.send()?;
        if response.status() == StatusCode::NOT_MODIFIED {
            if let Some(entry) = cached {
                return Ok(entry.into_tags());
            }
            return Err(Error::HttpStatus(StatusCode::NOT_MODIFIED.as_u16()));
        }
        if !response.status().is_success() {
            return Err(Error::HttpStatus(response.status().as_u16()));
        }

        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let next_link = response.headers().get(LINK).cloned();
        let body: Vec<ApiTag> = response.json()?;
        let mut tags = parse_api_tags(body);

        let mut next_url = parse_next_link(next_link.as_ref());
        let mut page_count = 1;

        while let Some(page_url) = next_url {
            if page_count >= MAX_PAGES {
                break;
            }

            let page_response = self
                .http
                .get(&page_url)
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "actioneer")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()?;

            if !page_response.status().is_success() {
                return Err(Error::HttpStatus(page_response.status().as_u16()));
            }

            let page_next_link = page_response.headers().get(LINK).cloned();
            let page_body: Vec<ApiTag> = page_response.json()?;
            tags.extend(parse_api_tags(page_body));

            next_url = parse_next_link(page_next_link.as_ref());
            page_count += 1;
        }

        if !self.no_cache {
            let _ = write_cached_tags(&cache_path, &tags, etag, SystemTime::now());
        }
        Ok(tags)
    }
}

#[derive(Deserialize)]
struct ApiTagCommit {
    sha: String,
}

#[derive(Deserialize)]
struct ApiTag {
    name: String,
    commit: ApiTagCommit,
}

fn resolve_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .and_then(|token| normalize_token(&token))
        .or_else(resolve_gh_auth_token)
}

fn resolve_gh_auth_token() -> Option<String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let token = String::from_utf8(output.stdout).ok()?;
    normalize_token(&token)
}

fn normalize_token(token: &str) -> Option<String> {
    let trimmed = token.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_api_tags(body: Vec<ApiTag>) -> Vec<Tag> {
    body.into_iter()
        .filter_map(|tag| {
            let version = parse_version(&tag.name)?;
            Some(Tag {
                name: tag.name,
                sha: tag.commit.sha,
                version,
            })
        })
        .collect()
}

fn parse_next_link(header: Option<&reqwest::header::HeaderValue>) -> Option<String> {
    let value = header?.to_str().ok()?;
    for part in value.split(',') {
        let trimmed = part.trim();
        if trimmed.contains("rel=\"next\"") {
            let start = trimmed.find('<')? + 1;
            let end = trimmed.find('>')?;
            return Some(trimmed[start..end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::normalize_token;

    #[test]
    fn normalize_token_trims_and_rejects_empty_values() {
        assert_eq!(Some(String::from("abc123")), normalize_token("  abc123 \n"));
        assert_eq!(None, normalize_token(""));
        assert_eq!(None, normalize_token("   \n\t"));
    }
}
