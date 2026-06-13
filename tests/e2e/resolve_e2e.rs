use std::collections::HashMap;

use actioneer::actions::{ActionUpdate, PinStyle, ResolveConfig, Tag, UpdateMode, resolve};
use actioneer::github::GitHubClient;
use actioneer::workflows::find_action_references;
use wiremock::MockServer;

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
) -> Vec<ActionUpdate> {
    let actions = find_action_references(&[workspace.root()], false).unwrap();
    let gh = GitHubClient::new_for_test(false, base_url, None);
    let mut tags: HashMap<(String, String), Vec<Tag>> = HashMap::new();

    for (owner, name) in repos {
        tags.insert(
            ((*owner).into(), (*name).into()),
            gh.fetch_tags(owner, name).unwrap(),
        );
    }

    resolve(&actions, &tags, &config)
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
    crate::support::mock_tags(&server, "actions", "checkout", r#"[{"name":"v4.2.0","commit":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}},{"name":"v4.1.0","commit":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}]"#).await;

    let result = tokio::task::block_in_place(|| {
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert_eq!(1, result.len());
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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
    )
    .await;

    let result = tokio::task::block_in_place(|| {
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Tag, false),
        )
    });

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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.1.5","commit":{"sha":"p15"}},{"name":"v4.2.0","commit":{"sha":"p20"}}]"#,
    )
    .await;

    let result = tokio::task::block_in_place(|| {
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Patch, PinStyle::Sha, false),
        )
    });

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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
    )
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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
    )
    .await;

    let result = tokio::task::block_in_place(|| {
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, true),
        )
    });

    assert!(result.is_empty());
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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.2.0","commit":{"sha":"goodsha0goodsha0goodsha0goodsha0"}}]"#,
    )
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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.2.0","commit":{"sha":"goodsha"}}]"#,
    )
    .await;

    let result = tokio::task::block_in_place(|| {
        resolve_workspace(
            &workspace,
            server.uri(),
            &[("actions", "checkout")],
            resolve_config(UpdateMode::Major, PinStyle::Sha, false),
        )
    });

    assert!(result.is_empty());
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
    crate::support::mock_tags(
        &server,
        "actions",
        "checkout",
        r#"[{"name":"v4.2.0","commit":{"sha":"sha42"}}]"#,
    )
    .await;
    crate::support::mock_tags(
        &server,
        "actions",
        "setup-node",
        r#"[{"name":"v4.0.0","commit":{"sha":"node40"}}]"#,
    )
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
}
