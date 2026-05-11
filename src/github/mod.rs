use reqwest::blocking::Client as HttpClient;
use serde::Deserialize;
use thiserror::Error;

use crate::engine::git::{parse_version, Version};
use crate::model::Repository;

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
}

impl Default for Client {
    fn default() -> Self {
        Self {
            http: HttpClient::builder().build().expect("reqwest client"),
            token: std::env::var("GITHUB_TOKEN")
                .ok()
                .filter(|token| !token.is_empty()),
        }
    }
}

impl Client {
    pub fn fetch_tags(&self, repository: &Repository) -> Result<Vec<Tag>, Error> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/tags?per_page=100",
            repository.owner, repository.name
        );
        let mut request = self
            .http
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "actioneer")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let response = request.send()?;
        if !response.status().is_success() {
            return Err(Error::HttpStatus(response.status().as_u16()));
        }

        let body: Vec<ApiTag> = response.json()?;
        Ok(body
            .into_iter()
            .filter_map(|tag| {
                let version = parse_version(&tag.name)?;
                Some(Tag {
                    name: tag.name,
                    sha: tag.commit.sha,
                    version,
                })
            })
            .collect())
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
