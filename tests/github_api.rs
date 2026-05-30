use std::fs;

use actioneer::github::{self, GitHubClient};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread")]
async fn fetch_tags_single_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/owner/name/tags"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v2.0.0","commit":{"sha":"abc123"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let tags = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), None)
            .fetch_tags("owner", "name")
            .unwrap()
    });
    assert_eq!(1, tags.len());
    assert_eq!("v2.0.0", tags[0].name);
    assert_eq!("abc123", tags[0].sha);
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_tags_multi_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/o/n/tags"))
        .and(query_param("per_page", "100"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header(
                    "Link",
                    format!("<{}/repos/o/n/tags?page=2>; rel=\"next\"", server.uri()),
                )
                .set_body_raw(
                    r#"[{"name":"v1.0.0","commit":{"sha":"a"}}]"#,
                    "application/json",
                ),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/o/n/tags"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v2.0.0","commit":{"sha":"b"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let tags = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), None)
            .fetch_tags("o", "n")
            .unwrap()
    });
    assert_eq!(2, tags.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_tags_max_pages() {
    let server = MockServer::start().await;
    for page in 1..=12 {
        let mut resp = ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v1.0.0","commit":{"sha":"a"}}]"#,
            "application/json",
        );
        if page <= 10 {
            resp = resp.append_header(
                "Link",
                format!(
                    "<{}/repos/o/n/tags?page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                ),
            );
        }
        let mock = if page == 1 {
            Mock::given(method("GET"))
                .and(path("/repos/o/n/tags"))
                .and(query_param("per_page", "100"))
        } else {
            Mock::given(method("GET"))
                .and(path("/repos/o/n/tags"))
                .and(query_param("page", page.to_string()))
        };
        mock.respond_with(resp).mount(&server).await;
    }

    let tags = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), None)
            .fetch_tags("o", "n")
            .unwrap()
    });
    assert_eq!(10, tags.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_tags_applies_auth() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/o/n/tags"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v1.0.0","commit":{"sha":"a"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let tags = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), Some("tok".into()))
            .fetch_tags("o", "n")
            .unwrap()
    });
    assert_eq!(1, tags.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_tags_http_404_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/o/n/tags"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let err = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), None)
            .fetch_tags("o", "n")
            .unwrap_err()
    });
    assert!(matches!(err, github::Error::HttpStatus(404)));
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_tags_rate_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/o/n/tags"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let err = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), None)
            .fetch_tags("o", "n")
            .unwrap_err()
    });
    assert!(matches!(err, github::Error::HttpStatus(429)));
}

#[tokio::test(flavor = "multi_thread")]
async fn cache_returns_stored_data() {
    let path = github::cache_path("t", "fresh");
    let _ = fs::remove_file(&path);
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(
        &path,
        r#"[{"name":"v1.0.0","sha":"abc"}]"#,
    )
    .unwrap();

    let server = MockServer::start().await;

    let tags = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(true, server.uri(), None)
            .fetch_tags("t", "fresh")
            .unwrap()
    });
    assert_eq!(1, tags.len());
    assert_eq!("v1.0.0", tags[0].name);
    assert_eq!("abc", tags[0].sha);
    let _ = fs::remove_file(&path);
}

#[tokio::test(flavor = "multi_thread")]
async fn cache_disabled_hits_api() {
    let cache_file = github::cache_path("t", "nocache");
    if let Some(p) = cache_file.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(&cache_file, r#"[{"name":"v1.0.0","sha":"stale"}]"#).unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/t/nocache/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v2.0.0","commit":{"sha":"fresh"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let tags = tokio::task::block_in_place(|| {
        GitHubClient::new_for_test(false, server.uri(), None)
            .fetch_tags("t", "nocache")
            .unwrap()
    });
    assert_eq!(1, tags.len());
    assert_eq!("v2.0.0", tags[0].name);
    let _ = fs::remove_file(&cache_file);
}
