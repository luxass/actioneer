mod support;

use std::path::Path;

use actioneer::discovery::discover_action_refs;

#[test]
fn discovers_external_workflow_uses_and_ignores_local_and_docker_refs() {
    let refs = discover_action_refs([Path::new(
        "testdata/workflows/discovery/ignores-local-and-docker",
    )])
    .expect("discover action refs");

    let actions = refs
        .iter()
        .map(|action| {
            (
                action.repo.as_str(),
                action.path.as_str(),
                action.ref_name.as_str(),
                action.line,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actions,
        vec![
            ("luxass/reusable", ".github/workflows/ci.yml", "v1", 8),
            ("actions/checkout", "", "v4", 13),
            ("owner/tool", "path", "main", 14),
        ]
    );

    assert!(refs.iter().all(|action| action.file.ends_with(
        ".github/workflows/ci.yml"
    )));
}

#[test]
fn workflow_workspace_macro_writes_dedented_multiline_files_for_discovery() {
    let workspace = workflow_workspace! {
        ".github/workflows/ci.yml" => r#"
            name: ci

            on:
              push:

            jobs:
              test:
                runs-on: ubuntu-latest
                steps:
                  - uses: actions/setup-node@v4
                  - uses: 'owner/single-quoted@v1.2.3'
                  - uses: "owner/tool/path@main" # keep this user comment
                  - uses: ./local-action
                  - run: echo "uses: ignored/text@v1"
        "#,
        "notes.txt" => r#"
            uses: ignored/non-yaml@v1
        "#,
    };

    let refs = discover_action_refs([workspace.path()]).expect("discover action refs");

    let actions = refs
        .iter()
        .map(|action| {
            (
                action.repo.as_str(),
                action.path.as_str(),
                action.ref_name.as_str(),
                action.line,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        actions,
        vec![
            ("actions/setup-node", "", "v4", 10),
            ("owner/single-quoted", "", "v1.2.3", 11),
            ("owner/tool", "path", "main", 12),
        ]
    );
}

#[test]
fn discovers_external_uses_in_composite_action_yaml() {
    let workspace = workflow_workspace! {
        "action.yml" => r#"
            name: composite
            runs:
              using: composite
              steps:
                - uses: docker/login-action@v3
                - uses: ./local-helper
        "#,
    };

    let refs = discover_action_refs([workspace.path()]).expect("discover action refs");

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].repo, "docker/login-action");
    assert_eq!(refs[0].ref_name, "v3");
    assert_eq!(refs[0].line, 5);
    assert!(refs[0].file.ends_with("action.yml"));
}
