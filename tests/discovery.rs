use std::fs;

use actioneer::discovery::discover_workflows;
use tempfile::TempDir;

#[test]
fn discovers_yml_and_yaml_under_github_workflows() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(workflows.join("ci.yml"), "name: CI\n").unwrap();
    fs::write(workflows.join("release.yaml"), "name: Release\n").unwrap();
    fs::write(workflows.join("README.md"), "nope\n").unwrap();

    let found = discover_workflows(dir.path()).unwrap();
    assert_eq!(found.len(), 2);
    assert!(found[0].to_str().unwrap().ends_with("ci.yml"));
    assert!(found[1].to_str().unwrap().ends_with("release.yaml"));
}

#[test]
fn returns_empty_when_workflows_dir_missing() {
    let dir = TempDir::new().unwrap();
    let found = discover_workflows(dir.path()).unwrap();
    assert!(found.is_empty());
}

#[test]
fn ignores_subdirectories() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(workflows.join("nested")).unwrap();
    fs::write(workflows.join("ci.yml"), "name: CI\n").unwrap();

    let found = discover_workflows(dir.path()).unwrap();
    assert_eq!(found.len(), 1);
}
