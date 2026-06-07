use std::collections::HashMap;

use actioneer::actions::{ActionReference, PinStyle, ResolveConfig, Tag, UpdateMode, resolve};
use actioneer::github::GitHubClient;
use actioneer::workflows::find_action_references;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support;

fn resolve_config(mode: UpdateMode, style: PinStyle, skip_branches: bool) -> ResolveConfig {
    ResolveConfig {
        excludes: vec![],
        skip_branches,
        mode,
        style,
    }
}

fn resolve_workspace(
    workspace: &support::TestWorkspace,
    base_url: String,
    repos: &[(&str, &str)],
    config: ResolveConfig,
) -> Vec<ActionReference> {
    let mut actions = find_action_references(&[workspace.root()], false).unwrap();
    let gh = GitHubClient::new_for_test(false, base_url, None);
    let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();

    for (owner, name) in repos {
        tags.insert(
            ((*owner).into(), (*name).into()),
            gh.fetch_tags(owner, name).unwrap(),
        );
    }

    resolve(&mut actions, &tags, &config);
    actions
}

#[tokio::test(flavor = "multi_thread")]
async fn sha_pin_version_upgrade() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4 # v4.1.0
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert_eq!(1, result.len());
    assert!(result[0].needs_update);
    assert_eq!(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        result[0].new_ref
    );
    assert_eq!("v4.2.0", result[0].new_version);
}

#[tokio::test(flavor = "multi_thread")]
async fn tag_pin_version_upgrade() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Tag, false),
        )
    });

    assert!(result[0].needs_update);
    assert_eq!("v4.2.0", result[0].new_ref);
}

#[tokio::test(flavor = "multi_thread")]
async fn patch_mode_upgrade() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4.1.0
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Patch, PinStyle::Sha, false),
        )
    });

    assert!(result[0].needs_update);
    assert_eq!("v4.1.5", result[0].new_version);
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_detected() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@main
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert!(result[0].is_branch);
    assert!(result[0].needs_update);
}

#[tokio::test(flavor = "multi_thread")]
async fn skip_branches_excluded() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@main
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, true),
        )
    });

    assert!(!result[0].needs_update);
}

#[tokio::test(flavor = "multi_thread")]
async fn sha_mismatch_detected() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@badcafe0 # v4.2.0
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert_eq!(1, result.len());
    assert!(result[0].sha_mismatch);
    assert!(result[0].needs_update);
}

#[tokio::test(flavor = "multi_thread")]
async fn already_current_no_update() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@goodsha # v4.2.0
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert_eq!(1, result.len());
    assert!(!result[0].needs_update);
}

#[tokio::test(flavor = "multi_thread")]
async fn multiple_repos_in_one_file() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4
                  - uses: actions/setup-node@v3
        "#,
    };

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
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout"), ("actions", "setup-node")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert_eq!(2, result.len());
    assert!(result[0].needs_update);
    assert!(result[1].needs_update);
}
