use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

use actioneer::github::GitHubClient;
use actioneer::model::{PinStyle, ResolveConfig, Tag, UpdateMode};
use actioneer::{resolve, scan};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_dir() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("actioneer-test-{}-{}", std::process::id(), n));
    let _ = fs::create_dir_all(&path);
    path
}

#[tokio::test(flavor = "multi_thread")]
async fn sha_pin_version_upgrade() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4 # v4.1.0\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .and(query_param("per_page", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}},{"name":"v4.1.0","commit":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        assert_eq!(1, actions.len());
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(result[0].needs_update);
    assert_eq!(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        result[0].new_ref
    );
    assert_eq!("v4.2.0", result[0].new_version);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn tag_pin_version_upgrade() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Tag,
            },
        );
        actions
    });

    assert!(result[0].needs_update);
    assert_eq!("v4.2.0", result[0].new_ref);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn patch_mode_upgrade() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@v4.1.0\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.1.5","commit":{"sha":"p15"}},{"name":"v4.2.0","commit":{"sha":"p20"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Patch,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(result[0].needs_update);
    assert_eq!("v4.1.5", result[0].new_version);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_detected() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@main\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(result[0].is_branch);
    assert!(result[0].needs_update);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn skip_branches_excluded() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@main\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: true,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(!result[0].needs_update);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn sha_mismatch_detected() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@badcafe0 # v4.2.0\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"goodsha0goodsha0goodsha0goodsha0"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        assert_eq!(1, actions.len());
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(result[0].sha_mismatch);
    assert!(result[0].needs_update);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn already_current_no_update() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@goodsha # v4.2.0\n",
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"goodsha"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        assert_eq!(1, actions.len());
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(!result[0].needs_update);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn multiple_repos_in_one_file() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        concat!(
            "jobs:\n",
            "  build:\n",
            "    steps:\n",
            "      - uses: actions/checkout@v4\n",
            "      - uses: actions/setup-node@v3\n",
        ),
    )
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/setup-node/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"[{"name":"v4.0.0","commit":{"sha":"node40"}}]"#,
            "application/json",
        ))
        .mount(&server)
        .await;

    let result = tokio::task::block_in_place(|| {
        let mut actions = scan::scan(&[tmp.display().to_string()], false).unwrap();
        assert_eq!(2, actions.len());
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();
        tags.insert(
            ("actions".into(), "checkout".into()),
            gh.fetch_tags("actions", "checkout").unwrap(),
        );
        tags.insert(
            ("actions".into(), "setup-node".into()),
            gh.fetch_tags("actions", "setup-node").unwrap(),
        );
        resolve::resolve(
            &mut actions,
            &tags,
            &ResolveConfig {
                excludes: vec![],
                skip_branches: false,
                mode: UpdateMode::Major,
                style: PinStyle::Sha,
            },
        );
        actions
    });

    assert!(result[0].needs_update);
    assert!(result[1].needs_update);
    let _ = fs::remove_dir_all(&tmp);
}
