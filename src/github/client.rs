use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::{Client as HttpClient, Response};
use serde::{Deserialize, de::DeserializeOwned};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::actions::Tag;
use crate::github::cache::{cache_path, no_cache_from_env, read_cache, write_cache};

const MAX_PAGES: usize = 10;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("github request failed with status {0}")]
    HttpStatus(u16),
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("github response did not contain a release date")]
    MissingReleaseDate,
    #[error("github response contained an invalid release date: {0}")]
    InvalidReleaseDate(String),
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

    pub fn fetch_tag_release_time(
        &self,
        owner: &str,
        name: &str,
        tag: &str,
    ) -> Result<SystemTime, Error> {
        let ref_url = format!("{}/repos/{owner}/{name}/git/ref/tags/{tag}", self.base_url);
        let tag_ref: ApiGitRef = self.get_json(&ref_url)?;

        match tag_ref.object.kind.as_str() {
            "tag" => {
                let tag_url = format!(
                    "{}/repos/{owner}/{name}/git/tags/{}",
                    self.base_url, tag_ref.object.sha
                );
                let tag: ApiGitTag = self.get_json(&tag_url)?;
                parse_github_time(
                    tag.tagger
                        .and_then(|tagger| tagger.date)
                        .ok_or(Error::MissingReleaseDate)?,
                )
            }
            "commit" => {
                let commit_url = format!(
                    "{}/repos/{owner}/{name}/git/commits/{}",
                    self.base_url, tag_ref.object.sha
                );
                let commit: ApiGitCommit = self.get_json(&commit_url)?;
                parse_github_time(
                    commit
                        .committer
                        .and_then(|committer| committer.date)
                        .or_else(|| commit.author.and_then(|author| author.date))
                        .ok_or(Error::MissingReleaseDate)?,
                )
            }
            _ => Err(Error::MissingReleaseDate),
        }
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

            let response = self.get(&page_url)?;
            let next = next_link(response.headers().get("link"));
            let body: Vec<ApiTag> = response.json()?;
            tags.extend(
                body.into_iter()
                    .filter_map(|t| Tag::from_name_sha(t.name, t.commit.sha)),
            );

            url = next;
        }

        Ok(tags)
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        Ok(self.get(url)?.json()?)
    }

    fn get(&self, url: &str) -> Result<Response, Error> {
        let mut req = self
            .http
            .get(url)
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
        Ok(response)
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

#[derive(Deserialize)]
struct ApiGitRef {
    object: ApiGitObject,
}

#[derive(Deserialize)]
struct ApiGitObject {
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Deserialize)]
struct ApiGitTag {
    tagger: Option<ApiGitActor>,
}

#[derive(Deserialize)]
struct ApiGitCommit {
    author: Option<ApiGitActor>,
    committer: Option<ApiGitActor>,
}

#[derive(Deserialize)]
struct ApiGitActor {
    date: Option<String>,
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

fn parse_github_time(value: String) -> Result<SystemTime, Error> {
    let parsed = OffsetDateTime::parse(&value, &Rfc3339)
        .map_err(|_| Error::InvalidReleaseDate(value.clone()))?;
    let seconds = parsed.unix_timestamp();
    if seconds < 0 {
        return Err(Error::InvalidReleaseDate(value));
    }
    Ok(UNIX_EPOCH + Duration::new(seconds as u64, parsed.nanosecond()))
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
