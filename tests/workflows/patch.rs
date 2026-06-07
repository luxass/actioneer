use actioneer::actions::{ActionReference, ActionUpdate};
use actioneer::workflows::{PatchError, apply_patches};

use crate::support;

fn action_reference_at(
    file: &str,
    current: &str,
    new_ref: &str,
    new_version: &str,
    vc: Option<&str>,
    mismatch: bool,
    ref_start: usize,
) -> ActionUpdate {
    ActionUpdate {
        action: ActionReference::from_discovery(
            "a".into(),
            "b".into(),
            String::new(),
            current.to_string(),
            vc.map(|s| s.to_string()),
            file.to_string(),
            4,
            ref_start,
            ref_start + current.len(),
        ),
        new_ref: new_ref.to_string(),
        new_version: new_version.to_string(),
        expected_sha: String::new(),
        sha_mismatch: mismatch,
        is_branch: false,
        is_major: false,
    }
}

fn patch_reference(
    workspace: &support::TestWorkspace,
    path: &str,
    current: &str,
    new_ref: &str,
    new_version: &str,
    vc: Option<&str>,
    mismatch: bool,
) -> ActionUpdate {
    let input = workspace.read(path);
    action_reference_at(
        &workspace.path(path),
        current,
        new_ref,
        new_version,
        vc,
        mismatch,
        input.find(current).unwrap(),
    )
}

#[test]
fn single_update_replaces_sha_and_writes_comment() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@oldsha # v4.1.0
        "#,
    };
    let a = patch_reference(
        &workspace,
        "ci.yml",
        "oldsha",
        "newsha",
        "v4.2.0",
        Some("v4.1.0"),
        false,
    );
    apply_patches(&[a], &[0]).unwrap();

    assert!(
        workspace
            .read("ci.yml")
            .contains("actions/checkout@newsha # v4.2.0")
    );
}

#[test]
fn multiple_updates_in_one_file() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@old1 # v4.1.0
                  - uses: actions/setup-node@old2 # v6.2.0
        "#,
    };
    let a1 = patch_reference(
        &workspace,
        "ci.yml",
        "old1",
        "new1",
        "v4.2.0",
        Some("v4.1.0"),
        false,
    );
    let a2 = patch_reference(
        &workspace,
        "ci.yml",
        "old2",
        "new2",
        "v6.4.0",
        Some("v6.2.0"),
        false,
    );
    apply_patches(&[a1, a2], &[0, 1]).unwrap();

    let result = workspace.read("ci.yml");
    assert!(result.contains("actions/checkout@new1 # v4.2.0"));
    assert!(result.contains("actions/setup-node@new2 # v6.4.0"));
}

#[test]
fn multiple_files() {
    let workspace = test_workspace! {
        "a.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: a/b@old1
        "#,
        "b.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: a/c@old2
        "#,
    };
    let a1 = patch_reference(&workspace, "a.yml", "old1", "new1", "v1.0.0", None, false);
    let a2 = patch_reference(&workspace, "b.yml", "old2", "new2", "v2.0.0", None, false);
    let count = apply_patches(&[a1, a2], &[0, 1]).unwrap();
    assert_eq!(2, count);

    assert!(workspace.read("a.yml").contains("@new1"));
    assert!(workspace.read("b.yml").contains("@new2"));
}

#[test]
fn preserves_quoted_ref() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: "actions/setup-node@oldsha" # v6.2.0
        "#,
    };
    let a = patch_reference(
        &workspace,
        "ci.yml",
        "oldsha",
        "newsha",
        "v6.4.0",
        Some("v6.2.0"),
        false,
    );
    apply_patches(&[a], &[0]).unwrap();

    assert!(
        workspace
            .read("ci.yml")
            .contains("\"actions/setup-node@newsha\" # v6.4.0")
    );
}

#[test]
fn preserves_crlf() {
    let workspace = test_workspace! {
        "placeholder" => "",
    };
    let input =
        "jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@oldsha # v4.1.0\r\n";
    workspace.write_raw("ci.yml", input);
    let a = action_reference_at(
        &workspace.path("ci.yml"),
        "oldsha",
        "newsha",
        "v4.2.0",
        Some("v4.1.0"),
        false,
        input.find("oldsha").unwrap(),
    );
    apply_patches(&[a], &[0]).unwrap();

    assert!(
        workspace
            .read("ci.yml")
            .contains("actions/checkout@newsha # v4.2.0\r\n")
    );
}

#[test]
fn no_comment_when_ref_equals_version() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@oldsha
        "#,
    };
    let a = patch_reference(
        &workspace, "ci.yml", "oldsha", "v4.2.0", "v4.2.0", None, false,
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = workspace.read("ci.yml");
    assert!(result.contains("actions/checkout@v4.2.0\n"));
    assert!(!result.contains(" # "));
}

#[test]
fn comment_written_on_sha_mismatch() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@oldsha
        "#,
    };
    let a = patch_reference(
        &workspace,
        "ci.yml",
        "oldsha",
        "newsha",
        "v4.2.0",
        Some("v4.1.0"),
        true,
    );
    apply_patches(&[a], &[0]).unwrap();

    assert!(
        workspace
            .read("ci.yml")
            .contains("actions/checkout@newsha # v4.2.0")
    );
}

#[test]
fn target_not_found_errors() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: a/b@v1
        "#,
    };
    let a = action_reference_at(
        &workspace.path("ci.yml"),
        "not-there",
        "new",
        "v2",
        None,
        false,
        999,
    );
    let err = apply_patches(&[a], &[0]).unwrap_err();
    assert!(matches!(err, PatchError::UpdateTargetNotFound));
}

#[test]
fn preserves_user_comment() {
    let workspace = test_workspace! {
        "ci.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: actions/checkout@oldsha  # do not remove this
        "#,
    };
    let a = patch_reference(
        &workspace, "ci.yml", "oldsha", "newsha", "v4.2.0", None, false,
    );
    apply_patches(&[a], &[0]).unwrap();

    assert!(
        workspace
            .read("ci.yml")
            .contains("actions/checkout@newsha  # do not remove this # v4.2.0")
    );
}

#[test]
fn sorts_interleaved_files() {
    let workspace = test_workspace! {
        "a.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: x/y@old1
                  - uses: x/y@old1b
        "#,
        "b.yml" => r#"
            jobs:
              build:
                steps:
                  - uses: x/z@old2
        "#,
    };
    let a1 = patch_reference(&workspace, "a.yml", "old1", "new1", "v1.0.0", None, false);
    let a2 = patch_reference(&workspace, "b.yml", "old2", "new2", "v2.0.0", None, false);
    let a3 = patch_reference(&workspace, "a.yml", "old1b", "new1b", "v1.1.0", None, false);
    let count = apply_patches(&[a1, a2, a3], &[0, 1, 2]).unwrap();
    assert_eq!(3, count);

    let out1 = workspace.read("a.yml");
    assert!(out1.contains("@new1"));
    assert!(out1.contains("@new1b"));
    assert!(workspace.read("b.yml").contains("@new2"));
}
