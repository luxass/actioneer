use std::{fs, path::PathBuf};

use reqwest::{blocking::Client, header::LINK};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubTag {
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Clone)]
pub struct GitHubTags {
    cache_dir: PathBuf,
    api_base_url: String,
    no_cache: bool,
    offline: bool,
    http: Client,
}

impl GitHubTags {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            api_base_url: "https://api.github.com".to_string(),
            no_cache: false,
            offline: false,
            http: Client::new(),
        }
    }

    pub fn with_api_base_url(mut self, api_base_url: impl Into<String>) -> Self {
        self.api_base_url = api_base_url.into();
        self
    }

    pub fn with_no_cache(mut self, no_cache: bool) -> Self {
        self.no_cache = no_cache;
        self
    }

    pub fn with_offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    pub fn tags_for_repo(&self, owner: &str, name: &str) -> Result<Vec<GitHubTag>, String> {
        if !self.no_cache {
            if let Some(tags) = self.read_cache(owner, name)? {
                return Ok(tags);
            }
        }

        if self.offline {
            return Err(format!(
                "offline mode requires cached GitHub tag data for {owner}/{name}, but no cache entry was found"
            ));
        }

        let tags = self.fetch_tags(owner, name)?;

        if !self.no_cache {
            self.write_cache(owner, name, &tags)?;
        }

        Ok(tags)
    }

    fn fetch_tags(&self, owner: &str, name: &str) -> Result<Vec<GitHubTag>, String> {
        let mut next_url = Some(format!(
            "{}/repos/{owner}/{name}/tags?per_page=100&page=1",
            self.api_base_url.trim_end_matches('/')
        ));
        let mut tags = Vec::new();

        while let Some(url) = next_url.take() {
            let response = self
                .http
                .get(&url)
                .header(
                    "user-agent",
                    concat!("actioneer/", env!("CARGO_PKG_VERSION")),
                )
                .bearer_auth_token_from_env()
                .send()
                .map_err(|error| {
                    format!("GitHub tags request failed for {owner}/{name}: {error}")
                })?;

            if !response.status().is_success() {
                return Err(format!(
                    "GitHub tags request failed for {owner}/{name}: HTTP {}",
                    response.status()
                ));
            }

            next_url = response
                .headers()
                .get(LINK)
                .and_then(|value| value.to_str().ok())
                .and_then(next_link_url);

            let page = response.json::<Vec<GitHubTagResponse>>().map_err(|error| {
                format!("failed to parse GitHub tags for {owner}/{name}: {error}")
            })?;
            tags.extend(
                page.into_iter()
                    .filter_map(GitHubTagResponse::into_version_tag),
            );
        }

        Ok(tags)
    }

    fn read_cache(&self, owner: &str, name: &str) -> Result<Option<Vec<GitHubTag>>, String> {
        let path = self.cache_path(owner, name);
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path).map_err(|error| {
            format!(
                "failed to read GitHub tag cache {}: {error}",
                path.display()
            )
        })?;
        let cache = serde_json::from_str::<CachedTags>(&contents).map_err(|error| {
            format!(
                "failed to parse GitHub tag cache {}: {error}",
                path.display()
            )
        })?;

        Ok(Some(cache.tags))
    }

    fn write_cache(&self, owner: &str, name: &str, tags: &[GitHubTag]) -> Result<(), String> {
        let path = self.cache_path(owner, name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create GitHub tag cache {}: {error}",
                    parent.display()
                )
            })?;
        }

        let contents = serde_json::to_string_pretty(&CachedTags {
            schema_version: 1,
            tags: tags.to_vec(),
        })
        .map_err(|error| format!("failed to serialize GitHub tag cache: {error}"))?;
        fs::write(&path, contents).map_err(|error| {
            format!(
                "failed to write GitHub tag cache {}: {error}",
                path.display()
            )
        })
    }

    fn cache_path(&self, owner: &str, name: &str) -> PathBuf {
        self.cache_dir.join("github-tags").join(format!(
            "{}--{}.json",
            sanitize(owner),
            sanitize(name)
        ))
    }
}

trait RequestBuilderExt {
    fn bearer_auth_token_from_env(self) -> Self;
}

impl RequestBuilderExt for reqwest::blocking::RequestBuilder {
    fn bearer_auth_token_from_env(self) -> Self {
        match std::env::var("GITHUB_TOKEN") {
            Ok(token) if !token.trim().is_empty() => self.bearer_auth(token),
            _ => self,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedTags {
    schema_version: u8,
    tags: Vec<GitHubTag>,
}

#[derive(Debug, Deserialize)]
struct GitHubTagResponse {
    name: String,
    commit: GitHubCommitResponse,
}

impl GitHubTagResponse {
    fn into_version_tag(self) -> Option<GitHubTag> {
        if is_version_tag(&self.name) {
            Some(GitHubTag {
                name: self.name,
                sha: self.commit.sha,
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubCommitResponse {
    sha: String,
}

fn next_link_url(link: &str) -> Option<String> {
    link.split(',').find_map(|part| {
        let part = part.trim();
        if !part.contains("rel=\"next\"") {
            return None;
        }
        let start = part.find('<')? + 1;
        let end = part[start..].find('>')? + start;
        Some(part[start..end].to_string())
    })
}

fn is_version_tag(name: &str) -> bool {
    let Some(version) = name.strip_prefix('v') else {
        return false;
    };

    !version.is_empty()
        && version.chars().any(|character| character.is_ascii_digit())
        && version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}
