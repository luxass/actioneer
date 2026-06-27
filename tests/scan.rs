use std::fs;

use actioneer::{
    cache::resolve_cache_dir_with,
    config::ActioneerConfig,
    github::{CacheEntry, GitHubClient, Release, ReleasesIndex},
    scan::scan_workspace,
};
use tempfile::TempDir;

fn seed_ref_cache(dir: &TempDir, owner: &str, repo: &str, kind: &str, git_ref: &str, entry: &CacheEntry) {
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

    let release_list: Vec<Release> = serde_json::from_str(include_str!(
        "../testdata/github/releases_checkout.json"
    ))
    .unwrap();
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

    let report = scan_workspace(dir.path(), &config, &client).unwrap();

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
    let report = scan_workspace(dir.path(), &config, &client).unwrap();
    assert_eq!(report.stats.workflows, 0);
}
