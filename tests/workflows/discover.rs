use actioneer::actions::ActionReference;
use actioneer::workflows::{DiscoveryError, find_action_references};

use crate::support;

fn references_in(workspace: &support::TestWorkspace, path: &str) -> Vec<ActionReference> {
    find_action_references(&[workspace.path(path)], false).unwrap()
}

fn references_under(workspace: &support::TestWorkspace, recursive: bool) -> Vec<ActionReference> {
    find_action_references(&[workspace.root()], recursive).unwrap()
}

fn action_names(actions: &[ActionReference]) -> Vec<String> {
    actions.iter().map(ActionReference::action_name).collect()
}

#[test]
fn finds_step_uses_in_workflow() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4 # v4.1.0
        "#,
    };

    let actions = references_in(&workspace, "ci.yml");

    assert_eq!(1, actions.len());
    assert_eq!("actions/checkout", actions[0].action_name());
    assert_eq!("v4", actions[0].current_ref);
    assert_eq!(Some("v4.1.0".to_string()), actions[0].version_comment);
}

#[test]
fn finds_reusable_workflow_job_uses() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                uses: myorg/repo/.github/workflows/ci.yml@v1
        "#,
    };

    let actions = references_in(&workspace, "ci.yml");

    assert_eq!(1, actions.len());
    assert_eq!(
        "myorg/repo/.github/workflows/ci.yml",
        actions[0].action_name()
    );
    assert_eq!("v1", actions[0].current_ref);
}

#[test]
fn skips_uses_inside_non_uses_strings() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                name: "deploy: uses: actions/fake@v1"
                steps:
                  - uses: actions/checkout@v4
        "#,
    };

    let actions = references_in(&workspace, "ci.yml");

    assert_eq!(vec!["actions/checkout"], action_names(&actions));
}

#[test]
fn finds_composite_action_step_uses() {
    let workspace = test_workspace! {
        "action.yml" => r#"
            runs:
              using: composite
              steps:
                - uses: actions/setup-node@v4 # v4.0.0
        "#,
    };

    let actions = references_in(&workspace, "action.yml");

    assert_eq!(1, actions.len());
    assert_eq!("actions/setup-node", actions[0].action_name());
    assert_eq!(Some("v4.0.0".to_string()), actions[0].version_comment);
}

#[test]
fn discovers_quoted_values() {
    let input = r#"
        jobs:
          build:
            steps:
              - uses: "actions/setup-node@v4"
    "#;
    let workspace = test_workspace! {
        "ci.yml" => input,
    };

    let actions = references_in(&workspace, "ci.yml");

    assert_eq!(1, actions.len());
    assert_eq!("v4", actions[0].current_ref);
}

#[test]
fn ignores_local_and_docker_uses() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: ./local-action
                  - uses: ../shared-action
                  - uses: docker://alpine:3.20
                  - uses: actions/checkout@v4
        "#,
    };

    let actions = references_in(&workspace, "ci.yml");

    assert_eq!(vec!["actions/checkout"], action_names(&actions));
}

#[test]
fn scans_directory_non_recursively() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4
        "#,
        "nested/ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/setup-node@v4
        "#,
    };

    let actions = references_under(&workspace, false);

    assert_eq!(vec!["actions/checkout"], action_names(&actions));
}

#[test]
fn scans_directory_recursively() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4
        "#,
        "nested/ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/setup-node@v4
        "#,
    };

    let actions = references_under(&workspace, true);

    let mut names = action_names(&actions);
    names.sort();
    assert_eq!(vec!["actions/checkout", "actions/setup-node"], names);
}

#[test]
fn ignores_non_yaml_files_in_directory() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4
        "#,
        "notes.txt" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/setup-node@v4
        "#,
    };

    let actions = references_under(&workspace, false);

    assert_eq!(vec!["actions/checkout"], action_names(&actions));
}

#[test]
fn ignores_missing_input_paths() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@v4
        "#,
    };

    let actions = find_action_references(
        &[workspace.path("missing.yml"), workspace.path("ci.yml")],
        false,
    )
    .unwrap();

    assert_eq!(vec!["actions/checkout"], action_names(&actions));
}

#[test]
fn reports_invalid_yaml_with_file_path() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build: [
        "#,
    };

    let err = find_action_references(&[workspace.path("ci.yml")], false).unwrap_err();

    assert!(matches!(err, DiscoveryError::InvalidYaml { file } if file.ends_with("ci.yml")));
}

#[test]
fn ignores_non_composite_action_yaml() {
    let workspace = test_workspace! {
        "action.yml" => r#"
            runs:
              using: node20
              main: dist/index.js
            inputs:
              fake:
                default: actions/checkout@v4
        "#,
    };

    let actions = references_in(&workspace, "action.yml");

    assert!(actions.is_empty());
}
