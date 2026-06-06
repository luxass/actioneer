use std::fs;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU32, Ordering};

use actioneer::cli::{GlobalArgs, Mode, ScanArgs};
use actioneer::cmd::audit;
use actioneer::github::GitHubClient;
use actioneer::model::{PinStyle, UpdateMode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_dir() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("actioneer-test-{}-{}", std::process::id(), n));
    let _ = fs::create_dir_all(&path);
    path
}

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
        inputs,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn all_secure_returns_success() {
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

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(
            global_args(),
            scan_args(vec![tmp.display().to_string()]),
            gh,
        )
        .unwrap()
    });
    assert_eq!(ExitCode::SUCCESS, code);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn branch_ref_returns_failure() {
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

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(
            global_args(),
            scan_args(vec![tmp.display().to_string()]),
            gh,
        )
        .unwrap()
    });
    assert_eq!(ExitCode::FAILURE, code);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn sha_mismatch_returns_failure() {
    let tmp = tmp_dir();
    fs::write(
        tmp.join("ci.yml"),
        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@badf00d # v4.2.0\n",
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

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(
            global_args(),
            scan_args(vec![tmp.display().to_string()]),
            gh,
        )
        .unwrap()
    });
    assert_eq!(ExitCode::FAILURE, code);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_scan_returns_success() {
    let tmp = tmp_dir();
    fs::write(tmp.join("ci.yml"), "jobs:\n  build:\n    steps: []\n").unwrap();

    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, "http://localhost:1".into(), None);
        audit::run(
            global_args(),
            scan_args(vec![tmp.display().to_string()]),
            gh,
        )
        .unwrap()
    });
    assert_eq!(ExitCode::SUCCESS, code);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn json_mode_clean_returns_success() {
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

    let mut global = global_args();
    global.mode = Mode::Json;
    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global, scan_args(vec![tmp.display().to_string()]), gh).unwrap()
    });
    assert_eq!(ExitCode::SUCCESS, code);
    let _ = fs::remove_dir_all(&tmp);
}

#[tokio::test(flavor = "multi_thread")]
async fn json_mode_with_issue_returns_failure() {
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

    let mut global = global_args();
    global.mode = Mode::Json;
    let code = tokio::task::block_in_place(|| {
        let gh = GitHubClient::new_for_test(false, server.uri(), None);
        audit::run(global, scan_args(vec![tmp.display().to_string()]), gh).unwrap()
    });
    assert_eq!(ExitCode::FAILURE, code);
    let _ = fs::remove_dir_all(&tmp);
}
