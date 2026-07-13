//! Integration tests for the shared workspace scan pipeline.

use std::fs;

use actioneer::{
    cache::resolve_cache_dir_with,
    config::ActioneerConfig,
    github::{CacheEntry, GitHubClient, Release, ReleasesIndex},
    scan::{ScanReport, scan_workspace},
};
use tempfile::TempDir;

fn seed_ref_cache(
    dir: &TempDir,
    owner: &str,
    repo: &str,
    kind: &str,
    git_ref: &str,
    entry: &CacheEntry,
) {
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap())).unwrap();
    let encoded = git_ref.replace('/', "%2F");
    let path = cache
        .path()
        .join("github")
        .join(owner)
        .join(repo)
        .join("refs")
        .join(kind)
        .join(format!("{encoded}.json"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(entry).unwrap()).unwrap();
}

fn seed_releases_cache(dir: &TempDir, owner: &str, repo: &str, releases: &ReleasesIndex) {
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap())).unwrap();
    let path = cache
        .path()
        .join("github")
        .join(owner)
        .join(repo)
        .join("releases")
        .join("index.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec_pretty(releases).unwrap()).unwrap();
}

fn setup_repo_with_basic_workflow(dir: &TempDir) {
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    let src = format!(
        "{}/testdata/workflows/basic.yml",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::copy(src, workflows.join("ci.yml")).unwrap();
}

fn scan_sha_workflow(
    dir: &TempDir,
    sha: &str,
    comment: Option<&str>,
    cached_tags: &[(&str, &str)],
) -> ScanReport {
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    let comment = comment
        .map(|value| format!(" # {value}"))
        .unwrap_or_default();
    fs::write(
        workflows.join("ci.yml"),
        format!(
            "jobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@{sha}{comment}\n"
        ),
    )
    .unwrap();

    for (tag, tag_sha) in cached_tags {
        let entry = CacheEntry {
            sha: (*tag_sha).into(),
            ref_kind: "tag".into(),
            published_at: Some("2020-01-01T00:00:00Z".into()),
            fetched_at: 1_700_000_000,
        };
        seed_ref_cache(dir, "actions", "checkout", "tags", tag, &entry);
    }

    let releases: Vec<Release> =
        serde_json::from_str(include_str!("../testdata/github/releases_checkout.json")).unwrap();
    seed_releases_cache(
        dir,
        "actions",
        "checkout",
        &ReleasesIndex {
            releases,
            fetched_at: 1_700_000_000,
        },
    );

    let config = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);
    scan_workspace(dir.path(), &[], &config, &client).unwrap()
}

fn issue_json(report: &ScanReport) -> Vec<serde_json::Value> {
    report.workflows[0].references[0]
        .issues
        .iter()
        .map(|issue| serde_json::to_value(issue).unwrap())
        .collect()
}

#[test]
fn scan_offline_produces_reference_reports() {
    let dir = TempDir::new().unwrap();
    setup_repo_with_basic_workflow(&dir);

    let entry_v4 = CacheEntry {
        sha: "a81bbbf8298c0fa03ea29cdc473d45769f953675".into(),
        ref_kind: "tag".into(),
        published_at: Some("2020-01-01T00:00:00Z".into()),
        fetched_at: 1_700_000_000,
    };
    seed_ref_cache(&dir, "actions", "checkout", "tags", "v4", &entry_v4);
    seed_ref_cache(&dir, "actions", "setup-node", "tags", "v4", &entry_v4);
    seed_ref_cache(&dir, "actions", "setup-node", "tags", "v3", &entry_v4);

    let release_list: Vec<Release> =
        serde_json::from_str(include_str!("../testdata/github/releases_checkout.json")).unwrap();
    let releases_index = ReleasesIndex {
        releases: release_list,
        fetched_at: 1_700_000_000,
    };
    seed_releases_cache(&dir, "actions", "checkout", &releases_index);
    seed_releases_cache(&dir, "actions", "setup-node", &releases_index);

    let config = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);

    let report = scan_workspace(dir.path(), &[], &config, &client).unwrap();

    assert_eq!(report.stats.workflows, 1);
    assert!(report.stats.references >= 3);
    assert!(report.stats.primary >= 3);
}

#[test]
fn scan_empty_repo_has_no_workflows() {
    let dir = TempDir::new().unwrap();
    let config = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);
    let report = scan_workspace(dir.path(), &[], &config, &client).unwrap();
    assert_eq!(report.stats.workflows, 0);
}

#[test]
fn sha_matching_semver_comment_is_verified() {
    let dir = TempDir::new().unwrap();
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let report = scan_sha_workflow(&dir, sha, Some("v4.2.0"), &[("v4.2.0", sha)]);
    let reference = &report.workflows[0].references[0];

    assert!(reference.issues.is_empty());
    assert!(reference.planned.is_none());
}

#[test]
fn sha_mismatching_semver_comment_reports_expected_tag_sha() {
    let dir = TempDir::new().unwrap();
    let pinned_sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let expected_sha = "b81bbbf8298c0fa03ea29cdc473d45769f953675";
    let report = scan_sha_workflow(
        &dir,
        pinned_sha,
        Some("v4.2.0"),
        &[("v4.2.0", expected_sha)],
    );
    let reference = &report.workflows[0].references[0];

    assert!(issue_json(&report).contains(&serde_json::json!({
        "kind": "sha-comment-mismatch",
        "comment": "v4.2.0",
        "expected_sha": expected_sha,
    })));
    let planned = reference.planned.as_ref().unwrap();
    assert_eq!(planned.to_sha, expected_sha);
    assert_eq!(planned.to_comment.as_deref(), Some("v4.2.0"));
}

#[test]
fn sha_semver_comment_offline_cache_miss_blocks_planning() {
    let dir = TempDir::new().unwrap();
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let report = scan_sha_workflow(&dir, sha, Some("v4.2.0"), &[]);
    let reference = &report.workflows[0].references[0];

    let resolution_failure = issue_json(&report)
        .into_iter()
        .find(|issue| issue["kind"] == "resolution-failed")
        .unwrap();
    assert!(
        resolution_failure["message"]
            .as_str()
            .unwrap()
            .contains("v4.2.0")
    );
    assert!(reference.planned.is_none());
    assert_eq!(report.stats.blocked, 1);
}

#[test]
fn sha_short_pin_preserves_comment_resolution_failure() {
    let dir = TempDir::new().unwrap();
    let report = scan_sha_workflow(&dir, "a81bbbf", Some("v4.2.0"), &[]);
    let reference = &report.workflows[0].references[0];

    assert!(reference.issues.iter().any(|issue| matches!(
        issue,
        actioneer::scan::AuditIssue::ResolutionFailed { message }
            if message.contains("v4.2.0")
    )));
    assert!(reference.planned.is_none());
    assert_eq!(report.stats.blocked, 1);
}

#[test]
fn sha_without_comment_reports_unverifiable_provenance_without_remediation() {
    let dir = TempDir::new().unwrap();
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let report = scan_sha_workflow(&dir, sha, None, &[("v4.2.0", sha)]);
    let reference = &report.workflows[0].references[0];

    assert!(issue_json(&report).contains(&serde_json::json!({
        "kind": "sha-provenance-unverifiable",
        "sha": sha,
    })));
    assert!(reference.planned.is_none());
}

#[test]
fn sha_with_non_semver_comment_reports_unverifiable_provenance() {
    let dir = TempDir::new().unwrap();
    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let report = scan_sha_workflow(&dir, sha, Some("trusted commit"), &[]);
    let reference = &report.workflows[0].references[0];

    assert!(issue_json(&report).contains(&serde_json::json!({
        "kind": "sha-provenance-unverifiable",
        "sha": sha,
    })));
    assert!(reference.planned.is_none());
}

#[test]
fn scan_local_reusable_workflow_skips_remote_audit_and_resolution() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        workflows.join("ci.yml"),
        "jobs:\n  build:\n    uses: ./.github/workflows/build.yml\n",
    )
    .unwrap();

    let config = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);

    let report = scan_workspace(dir.path(), &[], &config, &client).unwrap();
    let reference = &report.workflows[0].references[0];

    assert_eq!(
        reference.resolved.located.reference.raw,
        "./.github/workflows/build.yml"
    );
    assert!(!reference.issues.iter().any(|issue| matches!(
        issue,
        actioneer::scan::AuditIssue::NotShaPinned
            | actioneer::scan::AuditIssue::ResolutionFailed { .. }
    )));
    assert!(reference.issues.is_empty());
    assert!(reference.planned.is_none());
    assert_eq!(report.stats.blocked, 0);
    assert_eq!(report.stats.primary, 0);
    assert_eq!(report.stats.secondary, 1);
}

#[test]
fn scan_remote_reusable_workflow_still_receives_primary_pin_audit() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        workflows.join("ci.yml"),
        "jobs:\n  build:\n    uses: octo/example/.github/workflows/build.yml@v1\n",
    )
    .unwrap();

    let entry = CacheEntry {
        sha: "a81bbbf8298c0fa03ea29cdc473d45769f953675".into(),
        ref_kind: "tag".into(),
        published_at: Some("2020-01-01T00:00:00Z".into()),
        fetched_at: 1_700_000_000,
    };
    seed_ref_cache(&dir, "octo", "example", "tags", "v1", &entry);
    seed_releases_cache(
        &dir,
        "octo",
        "example",
        &ReleasesIndex {
            releases: Vec::new(),
            fetched_at: 1_700_000_000,
        },
    );

    let config = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);

    let report = scan_workspace(dir.path(), &[], &config, &client).unwrap();
    let reference = &report.workflows[0].references[0];

    assert!(
        reference
            .issues
            .iter()
            .any(|issue| matches!(issue, actioneer::scan::AuditIssue::NotShaPinned))
    );
    assert!(reference.planned.is_none());
    assert_eq!(report.stats.primary, 1);
}

#[test]
fn scan_major_only_tag_plans_normalization_and_flags_floating() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(
        workflows.join("ci.yml"),
        "jobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n",
    )
    .unwrap();

    let sha = "a81bbbf8298c0fa03ea29cdc473d45769f953675";
    let entry = CacheEntry {
        sha: sha.into(),
        ref_kind: "tag".into(),
        published_at: Some("2020-01-01T00:00:00Z".into()),
        fetched_at: 1_700_000_000,
    };
    seed_ref_cache(&dir, "actions", "checkout", "tags", "v4", &entry);
    seed_ref_cache(&dir, "actions", "checkout", "tags", "v4.2.0", &entry);

    let release_list: Vec<Release> =
        serde_json::from_str(include_str!("../testdata/github/releases_checkout.json")).unwrap();
    let releases_index = ReleasesIndex {
        releases: release_list,
        fetched_at: 1_700_000_000,
    };
    seed_releases_cache(&dir, "actions", "checkout", &releases_index);

    let config = ActioneerConfig {
        offline: true,
        pin: actioneer::config::PinMode::Tag,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);

    let report = scan_workspace(dir.path(), &[], &config, &client).unwrap();
    let reference = &report.workflows[0].references[0];

    assert!(
        reference
            .issues
            .iter()
            .any(|i| matches!(i, actioneer::scan::AuditIssue::FloatingMajorPin { .. }))
    );
    let planned = reference.planned.as_ref().unwrap();
    assert_eq!(planned.from_version.as_deref(), Some("v4"));
    assert_eq!(planned.to_ref, "v4.2.0");
}

#[test]
fn scan_explicit_workflow_file_path() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join("custom");
    fs::create_dir_all(&workflows).unwrap();
    let src = format!(
        "{}/testdata/workflows/basic.yml",
        env!("CARGO_MANIFEST_DIR")
    );
    fs::copy(src, workflows.join("ci.yml")).unwrap();

    let entry_v4 = CacheEntry {
        sha: "a81bbbf8298c0fa03ea29cdc473d45769f953675".into(),
        ref_kind: "tag".into(),
        published_at: Some("2020-01-01T00:00:00Z".into()),
        fetched_at: 1_700_000_000,
    };
    seed_ref_cache(&dir, "actions", "checkout", "tags", "v4", &entry_v4);
    seed_ref_cache(&dir, "actions", "setup-node", "tags", "v4", &entry_v4);
    seed_ref_cache(&dir, "actions", "setup-node", "tags", "v3", &entry_v4);

    let release_list: Vec<Release> =
        serde_json::from_str(include_str!("../testdata/github/releases_checkout.json")).unwrap();
    let releases_index = ReleasesIndex {
        releases: release_list,
        fetched_at: 1_700_000_000,
    };
    seed_releases_cache(&dir, "actions", "checkout", &releases_index);
    seed_releases_cache(&dir, "actions", "setup-node", &releases_index);

    let config = ActioneerConfig {
        offline: true,
        ..Default::default()
    };
    let cache = resolve_cache_dir_with(Some(dir.path().to_str().unwrap()));
    let client = GitHubClient::new(&config, cache);

    let report = scan_workspace(
        dir.path(),
        &[std::path::PathBuf::from("custom/ci.yml")],
        &config,
        &client,
    )
    .unwrap();

    assert_eq!(report.stats.workflows, 1);
    assert!(report.stats.references >= 3);
}
