use std::process::ExitCode;

use actioneer::actions::{PinStyle, UpdateMode};
use actioneer::cli::{GlobalArgs, Mode, ScanArgs};
use actioneer::cmd::audit;
use actioneer::github::GitHubClient;
use wiremock::MockServer;

fn global_args() -> GlobalArgs {
    GlobalArgs {
        dry_run: false,
        no_cache: false,
        excludes: vec![],
        mode: Mode::Beautiful,
    }
}

fn scan_args(inputs: Vec<String>) -> ScanArgs {
    ScanArgs {
        recursive: false,
        skip_branches: false,
        update: UpdateMode::Major,
        pin: PinStyle::Sha,
        yes: false,
        filters: vec![],
        inputs,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn all_secure_returns_success() {
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

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global_args(), scan_args(vec![workspace.root()]), gh)
    });
    assert_eq!(ExitCode::SUCCESS, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_ref_returns_failure() {
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

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global_args(), scan_args(vec![workspace.root()]), gh)
    });
    assert_eq!(ExitCode::FAILURE, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn sha_mismatch_returns_failure() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@badf00d # v4.2.0
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

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global_args(), scan_args(vec![workspace.root()]), gh)
    });
    assert_eq!(ExitCode::FAILURE, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_scan_returns_success() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps: []
        "#,
    };

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, "http://localhost:1".into(), None);
        audit::run(global_args(), scan_args(vec![workspace.root()]), gh)
    });
    assert_eq!(ExitCode::SUCCESS, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn json_mode_clean_returns_success() {
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

    let mut global = global_args();
    global.mode = Mode::Json;
    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global, scan_args(vec![workspace.root()]), gh)
    });
    assert_eq!(ExitCode::SUCCESS, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn filter_excludes_unmatched_action() {
    // checkout is a branch ref (insecure), but filter targets setup-node only
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@main
                  - uses: actions/setup-node@goodsha # v4.0.0
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
        r#"[{"name":"v4.0.0","commit":{"sha":"goodsha"}}]"#,
    )
    .await;

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut args = scan_args(vec![workspace.root()]);
        args.filters = vec!["actions/setup-node".into()];
        audit::run(global_args(), args, gh)
    });
    // checkout's branch ref is excluded by the filter, so audit sees only the clean setup-node
    assert_eq!(ExitCode::SUCCESS, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn filter_matches_insecure_action_returns_failure() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@main
                  - uses: actions/setup-node@goodsha # v4.0.0
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
        r#"[{"name":"v4.0.0","commit":{"sha":"goodsha"}}]"#,
    )
    .await;

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        let mut args = scan_args(vec![workspace.root()]);
        args.filters = vec!["actions/checkout".into()];
        audit::run(global_args(), args, gh)
    });
    assert_eq!(ExitCode::FAILURE, code);
}

#[tokio::test(flavor = "multi_thread")]
async fn json_mode_with_issue_returns_failure() {
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

    let mut global = global_args();
    global.mode = Mode::Json;
    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global, scan_args(vec![workspace.root()]), gh)
    });
    assert_eq!(ExitCode::FAILURE, code);
}
