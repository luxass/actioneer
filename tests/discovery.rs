use std::fs;
use std::path::PathBuf;

use actioneer::discovery::{DiscoveryError, discover_workflows, resolve_workflow_paths};
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
fn ignores_subdirectories_in_default_workflows_dir() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join(".github/workflows");
    fs::create_dir_all(workflows.join("nested")).unwrap();
    fs::write(workflows.join("ci.yml"), "name: CI\n").unwrap();

    let found = discover_workflows(dir.path()).unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn resolve_single_workflow_file() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join("testdata/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(workflows.join("ci.yml"), "name: CI\n").unwrap();

    let found = resolve_workflow_paths(dir.path(), &[workflows.join("ci.yml")]).unwrap();
    assert_eq!(found, vec![PathBuf::from("testdata/workflows/ci.yml")]);
}

#[test]
fn resolve_flat_workflow_directory() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join("testdata/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(workflows.join("a.yml"), "name: A\n").unwrap();
    fs::write(workflows.join("b.yaml"), "name: B\n").unwrap();
    fs::create_dir_all(workflows.join("nested")).unwrap();
    fs::write(workflows.join("nested/c.yml"), "name: C\n").unwrap();

    let found = resolve_workflow_paths(dir.path(), &[PathBuf::from("testdata/workflows")]).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0], PathBuf::from("testdata/workflows/a.yml"));
    assert_eq!(found[1], PathBuf::from("testdata/workflows/b.yaml"));
}

#[test]
fn resolve_empty_directory_returns_no_files() {
    let dir = TempDir::new().unwrap();
    let workflows = dir.path().join("empty");
    fs::create_dir_all(&workflows).unwrap();

    let found = resolve_workflow_paths(dir.path(), &[PathBuf::from("empty")]).unwrap();
    assert!(found.is_empty());
}

#[test]
fn resolve_rejects_non_yaml_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("readme.md"), "nope\n").unwrap();

    let err = resolve_workflow_paths(dir.path(), &[PathBuf::from("readme.md")]).unwrap_err();
    assert!(matches!(err, DiscoveryError::NotWorkflowFile { .. }));
}

#[test]
fn resolve_rejects_missing_path() {
    let dir = TempDir::new().unwrap();
    let err = resolve_workflow_paths(dir.path(), &[PathBuf::from("missing.yml")]).unwrap_err();
    assert!(matches!(err, DiscoveryError::NotFound { .. }));
}
