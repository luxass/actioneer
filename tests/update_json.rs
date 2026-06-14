mod support;

use std::process::Command;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

#[tokio::test(flavor = "multi_thread")]
async fn update_json_dry_run_reports_sha_pin_candidate_without_writing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "name": "v4.2.2", "commit": { "sha": "2222222222222222222222222222222222222222" } }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let workspace = "testdata/workflows/update/tag-to-sha";
    let workflow = format!("{workspace}/.github/workflows/ci.yml");
    let before = std::fs::read_to_string(&workflow).expect("read fixture before update");
    let cache_dir = temp_dir("actioneer-update-dry-run-cache");

    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir(workspace)
        .env("ACTIONEER_GITHUB_API_BASE_URL", server.uri())
        .env("ACTIONEER_CACHE_DIR", &cache_dir)
        .args(["update", "--dry-run", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer update");

    assert!(
        output.status.success(),
        "update dry-run should succeed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "expected empty stderr in JSON mode, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&workflow).expect("read fixture after update"),
        before,
        "dry-run must not write workflow files"
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("update stdout is JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "update");
    assert_eq!(json["ok"], true);
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["candidates"], 1);
    assert_eq!(json["summary"]["selected"], 1);
    assert_eq!(json["summary"]["applied"], 0);

    let candidate = &json["candidates"][0];
    assert_eq!(candidate["id"], "update-1");
    assert_eq!(candidate["kind"], "version_update");
    assert_eq!(candidate["file"], ".github/workflows/ci.yml");
    assert_eq!(candidate["line"], 10);
    assert_eq!(candidate["action"]["repo"], "actions/checkout");
    assert_eq!(candidate["action"]["current_ref"], "v4");
    assert_eq!(
        candidate["target"]["ref"],
        "2222222222222222222222222222222222222222"
    );
    assert_eq!(candidate["target"]["version"], "v4.2.2");
    assert_eq!(
        candidate["target"]["sha"],
        "2222222222222222222222222222222222222222"
    );
    assert_eq!(candidate["target"]["pin"], "sha");
    assert_eq!(candidate["reason"], "newer_version_available");
    assert_eq!(candidate["notes"], json!(["mutable_ref"]));
    assert_eq!(candidate["selected"], true);
    assert_eq!(candidate["applied"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn update_yes_patches_sha_ref_and_writes_version_comment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/actions/checkout/tags"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "name": "v4.2.2", "commit": { "sha": "2222222222222222222222222222222222222222" } }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let workspace = workflow_workspace! {
        ".github/workflows/ci.yml" => r#"
            name: ci

            on:
              push:

            jobs:
              test:
                runs-on: ubuntu-latest
                steps:
                  - uses: actions/checkout@v4
        "#,
    };
    let cache_dir = temp_dir("actioneer-update-apply-cache");

    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir(workspace.path())
        .env("ACTIONEER_GITHUB_API_BASE_URL", server.uri())
        .env("ACTIONEER_CACHE_DIR", &cache_dir)
        .args(["update", "--yes", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer update");

    assert!(
        output.status.success(),
        "update --yes should succeed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = std::fs::read_to_string(workspace.path().join(".github/workflows/ci.yml"))
        .expect("read patched workflow");
    assert!(
        workflow
            .contains("- uses: actions/checkout@2222222222222222222222222222222222222222 # v4.2.2"),
        "workflow should be patched with SHA and version comment:\n{workflow}"
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("update stdout is JSON");
    assert_eq!(json["summary"]["selected"], 1);
    assert_eq!(json["summary"]["applied"], 1);
    assert_eq!(json["candidates"][0]["selected"], true);
    assert_eq!(json["candidates"][0]["applied"], true);
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
