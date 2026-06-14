use actioneer::github::{GitHubTag, GitHubTags};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

#[tokio::test(flavor = "multi_thread")]
async fn fetches_paginated_version_tags_and_reuses_cache_unless_disabled() {
    let server = MockServer::start().await;
    let cache_dir = temp_dir("actioneer-github-tags-cache");

    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        "<{}{}>; rel=\"next\"",
                        server.uri(),
                        "/repos/actions/checkout/tags?per_page=100&page=2"
                    ),
                )
                .set_body_json(json!([
                    { "name": "v4.2.2", "commit": { "sha": "2222222222222222222222222222222222222222" } },
                    { "name": "main", "commit": { "sha": "mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm" } }
                ])),
        )
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "name": "v4.1.0", "commit": { "sha": "1111111111111111111111111111111111111111" } }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let fresh_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "name": "v4.3.0", "commit": { "sha": "3333333333333333333333333333333333333333" } }
        ])))
        .expect(1)
        .mount(&fresh_server)
        .await;

    let api_base_url = server.uri();
    let fresh_api_base_url = fresh_server.uri();
    let (tags, cached, fresh) = tokio::task::spawn_blocking({
        let cache_dir = cache_dir.clone();
        move || {
            let tags = GitHubTags::new(&cache_dir)
                .with_api_base_url(api_base_url.clone())
                .tags_for_repo("actions", "checkout")
                .expect("fetch tags from GitHub API");

            let cached = GitHubTags::new(&cache_dir)
                .with_api_base_url(api_base_url.clone())
                .tags_for_repo("actions", "checkout")
                .expect("load tags from cache");

            let fresh = GitHubTags::new(&cache_dir)
                .with_api_base_url(fresh_api_base_url)
                .with_no_cache(true)
                .tags_for_repo("actions", "checkout")
                .expect("--no-cache should bypass cached tags and fetch fresh data");

            (tags, cached, fresh)
        }
    })
    .await
    .expect("GitHub tag fetch task should not panic");

    assert_eq!(
        tags,
        vec![
            GitHubTag {
                name: "v4.2.2".to_string(),
                sha: "2222222222222222222222222222222222222222".to_string(),
            },
            GitHubTag {
                name: "v4.1.0".to_string(),
                sha: "1111111111111111111111111111111111111111".to_string(),
            },
        ]
    );
    assert_eq!(cached, tags);
    assert_eq!(
        fresh,
        vec![GitHubTag {
            name: "v4.3.0".to_string(),
            sha: "3333333333333333333333333333333333333333".to_string(),
        }]
    );
}

fn temp_dir(prefix: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}
