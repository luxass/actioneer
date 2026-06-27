use std::fs;
use std::path::PathBuf;

use actioneer::config::{ActioneerConfig, PinMode};
use actioneer::engine::{parse_workflow, CommentMatch};
use actioneer::github::{RefKind, ResolvedRef};
use actioneer::scan::{
    apply,
    types::{
        ApplyTarget, LocatedReference, PlannedChange, PlanReason, ReferenceReport,
        ResolvedReference, ScanReport, ScanStats, WorkflowReport,
    },
};
use tempfile::TempDir;

fn workflow_report(path: &PathBuf, content: &str, planned: PlannedChange) -> ScanReport {
    let document = parse_workflow(content).unwrap();
    let reference = document.references[0].clone();

    ScanReport {
        workflows: vec![WorkflowReport {
            path: path.clone(),
            name: Some("CI".into()),
            references: vec![ReferenceReport {
                resolved: ResolvedReference {
                    located: LocatedReference {
                        workflow_path: path.clone(),
                        reference,
                    },
                    current: ResolvedRef {
                        sha: "a".repeat(40),
                        ref_kind: RefKind::Tag,
                        published_at: None,
                    },
                    comment_match: CommentMatch::NoComment,
                },
                issues: vec![],
                planned: Some(planned),
            }],
        }],
        stats: ScanStats {
            workflows: 1,
            references: 1,
            planned: 1,
            ..Default::default()
        },
    }
}

#[test]
fn apply_tag_bump_rewrites_uses_line() {
    let dir = TempDir::new().unwrap();
    let workflow_path = PathBuf::from(".github/workflows/ci.yml");
    let file = dir.path().join(&workflow_path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let content = "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4.1.0\n";
    fs::write(&file, content).unwrap();

    let report = workflow_report(
        &workflow_path,
        content,
        PlannedChange {
            from_ref: "v4.1.0".into(),
            to_ref: "v4.2.0".into(),
            from_version: Some("v4.1.0".into()),
            to_sha: "b".repeat(40),
            to_comment: None,
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        },
    );

    let config = ActioneerConfig {
        pin: PinMode::Tag,
        ..Default::default()
    };
    let targets = vec![ApplyTarget {
        workflow_path: workflow_path.clone(),
        line: 7,
    }];

    let result = apply(dir.path(), &report, &targets, &config, false).unwrap();
    assert_eq!(result.applied.len(), 1);
    assert!(result.failures.is_empty());

    let updated = fs::read_to_string(&file).unwrap();
    assert!(updated.contains("uses: actions/checkout@v4.2.0"));
}

#[test]
fn apply_sha_bump_updates_comment() {
    let dir = TempDir::new().unwrap();
    let workflow_path = PathBuf::from(".github/workflows/ci.yml");
    let file = dir.path().join(&workflow_path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let old_sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let new_sha = "df4cb1c069e1874edd31b4311f1884172cec0e10";
    let content = format!(
        "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{old_sha} # v4.2.0\n"
    );
    fs::write(&file, &content).unwrap();

    let report = workflow_report(
        &workflow_path,
        &content,
        PlannedChange {
            from_ref: old_sha.into(),
            to_ref: new_sha.into(),
            from_version: Some("v4.2.0".into()),
            to_sha: new_sha.into(),
            to_comment: Some("v4.3.0".into()),
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        },
    );

    let config = ActioneerConfig::default();
    let targets = vec![ApplyTarget {
        workflow_path,
        line: 7,
    }];

    let result = apply(dir.path(), &report, &targets, &config, false).unwrap();
    assert_eq!(result.applied.len(), 1);

    let updated = fs::read_to_string(&file).unwrap();
    assert!(updated.contains(&format!("uses: actions/checkout@{new_sha} # v4.3.0")));
}

#[test]
fn apply_dry_run_does_not_write() {
    let dir = TempDir::new().unwrap();
    let workflow_path = PathBuf::from(".github/workflows/ci.yml");
    let file = dir.path().join(&workflow_path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let content = "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4.1.0\n";
    fs::write(&file, content).unwrap();

    let report = workflow_report(
        &workflow_path,
        content,
        PlannedChange {
            from_ref: "v4.1.0".into(),
            to_ref: "v4.2.0".into(),
            from_version: Some("v4.1.0".into()),
            to_sha: "b".repeat(40),
            to_comment: None,
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        },
    );

    let config = ActioneerConfig {
        pin: PinMode::Tag,
        ..Default::default()
    };
    let targets = vec![ApplyTarget {
        workflow_path: workflow_path.clone(),
        line: 7,
    }];

    let result = apply(dir.path(), &report, &targets, &config, true).unwrap();
    assert_eq!(result.applied.len(), 1);
    assert_eq!(fs::read_to_string(&file).unwrap(), content);
}

#[test]
fn apply_fails_when_line_changed_since_scan() {
    let dir = TempDir::new().unwrap();
    let workflow_path = PathBuf::from(".github/workflows/ci.yml");
    let file = dir.path().join(&workflow_path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let scanned = "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4.1.0\n";
    let on_disk = "name: CI\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v9.9.9\n";
    fs::write(&file, on_disk).unwrap();

    let report = workflow_report(
        &workflow_path,
        scanned,
        PlannedChange {
            from_ref: "v4.1.0".into(),
            to_ref: "v4.2.0".into(),
            from_version: Some("v4.1.0".into()),
            to_sha: "b".repeat(40),
            to_comment: None,
            reason: PlanReason::SemverBump {
                level: "minor".into(),
            },
        },
    );

    let config = ActioneerConfig {
        pin: PinMode::Tag,
        ..Default::default()
    };
    let targets = vec![ApplyTarget {
        workflow_path,
        line: 7,
    }];

    let result = apply(dir.path(), &report, &targets, &config, false).unwrap();
    assert!(result.applied.is_empty());
    assert_eq!(result.failures.len(), 1);
}
