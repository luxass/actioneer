mod support;

use std::process::Command;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

#[tokio::test(flavor = "multi_thread")]
async fn audit_fix_pins_mutable_tag_to_sha_without_prompting() {
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
    let cache_dir = temp_dir("actioneer-audit-fix-cache");

    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir(workspace.path())
        .env("ACTIONEER_GITHUB_API_BASE_URL", server.uri())
        .env("ACTIONEER_CACHE_DIR", &cache_dir)
        .args(["audit", "--fix", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit --fix");

    assert!(
        output.status.success(),
        "audit --fix should succeed when all findings are fixed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let workflow = std::fs::read_to_string(workspace.path().join(".github/workflows/ci.yml"))
        .expect("read patched workflow");
    assert!(
        workflow
            .contains("- uses: actions/checkout@2222222222222222222222222222222222222222 # v4.2.2"),
        "workflow should be patched with SHA and version comment:\n{workflow}"
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit --fix stdout is JSON");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "audit");
    assert_eq!(json["ok"], true);
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["findings"], 0);
    assert_eq!(json["summary"]["fixable"], 0);
    assert_eq!(
        json["findings"].as_array().expect("findings array").len(),
        0
    );
    assert_eq!(json["fixes"][0]["finding_id"], "finding-1");
    assert_eq!(json["fixes"][0]["file"], ".github/workflows/ci.yml");
    assert_eq!(json["fixes"][0]["line"], 10);
    assert_eq!(json["fixes"][0]["applied"], true);
    assert_eq!(
        json["fixes"][0]["new_ref"],
        "2222222222222222222222222222222222222222"
    );
    assert_eq!(json["fixes"][0]["new_version_comment"], "v4.2.2");
}

#[test]
fn audit_json_succeeds_for_secure_full_sha_ref() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/audit/secure-full-sha")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(
        output.status.success(),
        "audit should succeed for secure refs; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "expected empty stderr in JSON mode, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "audit");
    assert_eq!(json["ok"], true);
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["findings"], 0);
    assert_eq!(json["summary"]["fixable"], 0);
    assert_eq!(
        json["findings"].as_array().expect("findings array").len(),
        0
    );
}

#[test]
fn audit_json_applies_config_globals_and_ordered_rules() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/config/rules-order")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(
        !output.status.success(),
        "audit should fail for setup-node only"
    );
    assert!(
        output.stderr.is_empty(),
        "expected empty stderr in JSON mode, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["references"], 2);
    assert_eq!(json["summary"]["findings"], 1);

    let findings = json["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["action"]["repo"], "actions/setup-node");
    assert_eq!(findings[0]["action"]["ref"], "v4");
}

#[test]
fn audit_json_uses_root_config_global_pin() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/config/root-config")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(
        output.status.success(),
        "root config pin=tag should allow branch refs as compliant; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["findings"], 0);
}

#[test]
fn audit_json_uses_github_config_global_pin() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/config/github-config")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(
        output.status.success(),
        ".github/actioneer.toml pin=tag should allow branch refs as compliant; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["findings"], 0);
}

#[test]
fn audit_json_applies_owner_specific_rule() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/config/owner-specific-rule")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "audit should fail for docker/login-action");
    assert!(output.stderr.is_empty());

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["references"], 2);
    assert_eq!(json["summary"]["findings"], 1);

    let findings = json["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["action"]["repo"], "docker/login-action");
    assert_eq!(findings[0]["action"]["ref"], "main");
}

#[test]
fn audit_json_reports_mutable_branch_ref() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/audit/mutable-branch")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "audit should fail with findings");
    assert!(output.stderr.is_empty());

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["findings"], 1);
    assert_eq!(json["findings"][0]["kind"], "mutable_ref");
    assert_eq!(json["findings"][0]["action"]["ref"], "main");
}

#[test]
fn audit_json_reports_short_sha_ref() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/audit/short-sha")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "audit should fail with findings");
    assert!(output.stderr.is_empty());

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["findings"], 1);
    assert_eq!(json["findings"][0]["kind"], "mutable_ref");
}

#[test]
fn audit_json_reports_sha_comment_mismatch() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/audit/sha-comment-mismatch")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "audit should fail with findings");
    assert!(output.stderr.is_empty());

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");
    assert_eq!(json["summary"]["findings"], 1);
    assert_eq!(json["findings"][0]["kind"], "sha_comment_mismatch");
    assert!(
        json["findings"][0]["expected_sha"].is_string(),
        "expected_sha should be set"
    );
}

#[test]
fn audit_json_reports_mutable_ref_finding() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/audit/mutable-tag")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "audit should fail with findings");
    assert!(
        output.stderr.is_empty(),
        "expected empty stderr in JSON mode, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).expect("audit stdout is JSON");

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "audit");
    assert_eq!(json["ok"], false);
    assert_eq!(json["summary"]["references"], 1);
    assert_eq!(json["summary"]["findings"], 1);
    assert_eq!(json["summary"]["fixable"], 1);

    let finding = &json["findings"][0];
    assert_eq!(finding["id"], "finding-1");
    assert_eq!(finding["kind"], "mutable_ref");
    assert_eq!(finding["severity"], "error");
    assert_eq!(finding["file"], ".github/workflows/ci.yml");
    assert_eq!(finding["line"], 10);
    assert_eq!(finding["action"]["owner"], "actions");
    assert_eq!(finding["action"]["name"], "checkout");
    assert_eq!(finding["action"]["repo"], "actions/checkout");
    assert_eq!(finding["action"]["path"], "");
    assert_eq!(finding["action"]["ref"], "v4");
    assert_eq!(finding["message"], "Action is pinned to a mutable tag");
    assert_eq!(finding["recommendation"], "Pin to a full SHA");
    assert_eq!(finding["fixable"], true);
    assert!(finding["expected_sha"].is_null());
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
