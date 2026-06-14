use std::process::Command;

use serde_json::Value;

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
    assert_eq!(json["findings"].as_array().expect("findings array").len(), 0);
}

#[test]
fn audit_json_applies_config_globals_and_ordered_rules() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir("testdata/workflows/config/rules-order")
        .args(["audit", "--mode", "json", ".github"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "audit should fail for setup-node only");
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
