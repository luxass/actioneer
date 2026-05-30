use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use reqwest::blocking::Client as HttpClient;
use serde::Deserialize;

use crate::model::{Tag, parse_version};

const MAX_PAGES: usize = 10;
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 6);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("github request failed")]
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
    fn default() -> Self { Self::new(true) }
}

impl GitHubClient {
    pub fn new(cache_enabled: bool) -> Self {
        let cache_enabled = cache_enabled && !no_cache_from_env();
        Self {
            http: HttpClient::builder().build().expect("reqwest client"),
            base_url: "https://api.github.com".into(),
            token: resolve_token(),
            cache_enabled,
        }
    }

    #[cfg(test)]
    pub fn new_for_test(cache_enabled: bool, base_url: String, token: Option<String>) -> Self {
        Self {
            http: HttpClient::builder().build().expect("reqwest client"),
            base_url,
            token,
            cache_enabled,
        }
    }

    pub fn fetch_tags(&self, owner: &str, name: &str) -> Result<Vec<Tag>, Error> {
        let cache_path = cache_path(owner, name);

        if self.cache_enabled {
            if let Some(tags) = read_cache(&cache_path) {
                return Ok(tags);
            }
        }

        let url = format!("{}/repos/{owner}/{name}/tags?per_page=100", self.base_url);
        let tags = self.fetch_all_pages(&url)?;

        if self.cache_enabled {
            if let Some(parent) = cache_path.parent() { let _ = fs::create_dir_all(parent); }
            let cached: Vec<CachedTag> = tags.iter().map(|t| CachedTag { name: t.name.clone(), sha: t.sha.clone() }).collect();
            if let Ok(json) = serde_json::to_string(&cached) { let _ = fs::write(&cache_path, json); }
        }

        Ok(tags)
    }

    fn fetch_all_pages(&self, start_url: &str) -> Result<Vec<Tag>, Error> {
        let mut tags = Vec::new();
        let mut url = Some(start_url.to_string());
        let mut pages = 0;

        while let Some(page_url) = url {
            if pages >= MAX_PAGES { break; }
            pages += 1;

            let mut req = self.http.get(&page_url)
                .header("Accept", "application/vnd.github+json")
                .header("User-Agent", "actioneer")
                .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(token) = &self.token { req = req.bearer_auth(token); }

            let response = req.send()?;
            if !response.status().is_success() {
                return Err(Error::HttpStatus(response.status().as_u16()));
            }

            let next = next_link(response.headers().get("link"));
            let body: Vec<ApiTag> = response.json()?;
            tags.extend(body.into_iter().filter_map(|t| {
                Some(Tag { name: t.name.clone(), sha: t.commit.sha, version: parse_version(&t.name)? })
            }));

            url = next;
        }

        Ok(tags)
    }
}

#[derive(Deserialize)]
struct ApiTag { name: String, commit: ApiCommit }
#[derive(Deserialize)]
struct ApiCommit { sha: String }
#[derive(serde::Serialize, serde::Deserialize)]
struct CachedTag { name: String, sha: String }

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

fn cache_path(owner: &str, name: &str) -> PathBuf {
    std::env::temp_dir().join("actioneer-cache").join("tags").join(format!("{owner}__{name}.json"))
}

fn read_cache(path: &PathBuf) -> Option<Vec<Tag>> {
    let meta = fs::metadata(path).ok()?;
    let age = SystemTime::now().duration_since(meta.modified().ok()?).ok()?;
    if age > CACHE_TTL { return None; }
    let contents = fs::read_to_string(path).ok()?;
    let cached: Vec<CachedTag> = serde_json::from_str(&contents).ok()?;
    Some(cached.into_iter().filter_map(|t| {
        Some(Tag { name: t.name.clone(), sha: t.sha, version: parse_version(&t.name)? })
    }).collect())
}

fn resolve_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN").ok()
        .and_then(|t| { let t = t.trim(); (!t.is_empty()).then(|| t.to_string()) })
        .or_else(|| {
            let out = std::process::Command::new("gh").args(["auth", "token"])
                .stdin(std::process::Stdio::null()).output().ok()?;
            if !out.status.success() { return None; }
            let token = String::from_utf8(out.stdout).ok()?;
            let t = token.trim(); (!t.is_empty()).then(|| t.to_string())
        })
}

fn no_cache_from_env() -> bool {
    matches!(std::env::var("ACTIONEER_NO_CACHE"), Ok(v) if matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[test]
    fn no_cache_env_true() {
        unsafe { std::env::set_var("ACTIONEER_NO_CACHE", "true"); }
        assert!(no_cache_from_env());
        unsafe { std::env::remove_var("ACTIONEER_NO_CACHE"); }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetches_tags_from_api() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/t/t/tags"))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{"name":"v2.0.0","commit":{"sha":"newsha"}}]"#, "application/json",
            ))
            .mount(&server).await;

        let tags = tokio::task::block_in_place(|| {
            let gh = GitHubClient::new_for_test(false, server.uri(), None);
            gh.fetch_tags("t", "t").unwrap()
        });
        assert_eq!(1, tags.len());
        assert_eq!("v2.0.0", tags[0].name);
        assert_eq!("newsha", tags[0].sha);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auth_applied_to_all_pages() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/t/auth/tags"))
            .and(query_param("per_page", "100"))
            .and(header("authorization", "Bearer token-1"))
            .respond_with(ResponseTemplate::new(200)
                .append_header("Link", format!("<{}/repos/t/auth/tags?page=2>; rel=\"next\"", server.uri()))
                .set_body_raw(r#"[{"name":"v1.0.0","commit":{"sha":"a"}}]"#, "application/json"))
            .mount(&server).await;
        Mock::given(method("GET"))
            .and(path("/repos/t/auth/tags"))
            .and(query_param("page", "2"))
            .and(header("authorization", "Bearer token-1"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"[{"name":"v2.0.0","commit":{"sha":"b"}}]"#, "application/json",
            ))
            .mount(&server).await;

        let tags = tokio::task::block_in_place(|| {
            let gh = GitHubClient::new_for_test(false, server.uri(), Some("token-1".into()));
            gh.fetch_tags("t", "auth").unwrap()
        });
        assert_eq!(2, tags.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cache_returns_fresh_data_without_api_call() {
        let path = cache_path("t", "cached");
        let _ = fs::remove_file(&path);
        if let Some(p) = path.parent() { fs::create_dir_all(p).unwrap(); }
        let cached = vec![CachedTag { name: "v1.0.0".into(), sha: "abc".into() }];
        fs::write(&path, serde_json::to_string(&cached).unwrap()).unwrap();

        let server = MockServer::start().await;
        // No mock mounts — if we hit the API, it'll fail

        let tags = tokio::task::block_in_place(|| {
            let gh = GitHubClient::new_for_test(true, server.uri(), None);
            gh.fetch_tags("t", "cached").unwrap()
        });
        assert_eq!(1, tags.len());
        assert_eq!("v1.0.0", tags[0].name);
        let _ = fs::remove_file(&path);
    }
}
