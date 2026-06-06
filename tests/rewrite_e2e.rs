use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

use actioneer::actions::ActionReference;
use actioneer::workflows::{PatchError, apply_patches};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_dir() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("actioneer-test-{}-{}", std::process::id(), n));
    let _ = fs::create_dir_all(&path);
    path
}

fn mk_action(
    file: &str,
    current: &str,
    new_ref: &str,
    new_version: &str,
    vc: Option<&str>,
    mismatch: bool,
    ref_start: usize,
) -> ActionReference {
    ActionReference {
        owner: "a".into(),
        name: "b".into(),
        path: String::new(),
        current_ref: current.to_string(),
        version_comment: vc.map(|s| s.to_string()),
        file: file.to_string(),
        line: 4,
        ref_start,
        ref_end: ref_start + current.len(),
        new_ref: new_ref.to_string(),
        new_version: new_version.to_string(),
        expected_sha: String::new(),
        sha_mismatch: mismatch,
        is_branch: false,
        is_major: false,
        needs_update: true,
    }
}

#[test]
fn single_update_replaces_sha_and_writes_comment() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input = "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@oldsha # v4.1.0\n";
    fs::write(&file, input).unwrap();

    let ref_start = input.find("oldsha").unwrap();
    let a = mk_action(
        &file.to_string_lossy(),
        "oldsha",
        "newsha",
        "v4.2.0",
        Some("v4.1.0"),
        false,
        ref_start,
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("actions/checkout@newsha # v4.2.0"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn multiple_updates_in_one_file() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input = concat!(
        "jobs:\n",
        "  build:\n",
        "    steps:\n",
        "      - uses: actions/checkout@old1 # v4.1.0\n",
        "      - uses: actions/setup-node@old2 # v6.2.0\n",
    );
    fs::write(&file, input).unwrap();

    let a1 = mk_action(
        &file.to_string_lossy(),
        "old1",
        "new1",
        "v4.2.0",
        Some("v4.1.0"),
        false,
        input.find("old1").unwrap(),
    );
    let a2 = mk_action(
        &file.to_string_lossy(),
        "old2",
        "new2",
        "v6.4.0",
        Some("v6.2.0"),
        false,
        input.find("old2").unwrap(),
    );
    apply_patches(&[a1, a2], &[0, 1]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("actions/checkout@new1 # v4.2.0"));
    assert!(result.contains("actions/setup-node@new2 # v6.4.0"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn multiple_files() {
    let tmp = tmp_dir();
    let f1 = tmp.join("a.yml");
    let f2 = tmp.join("b.yml");
    let input1 = "jobs:\n  build:\n    steps:\n      - uses: a/b@old1\n";
    let input2 = "jobs:\n  build:\n    steps:\n      - uses: a/c@old2\n";
    fs::write(&f1, input1).unwrap();
    fs::write(&f2, input2).unwrap();

    let a1 = mk_action(
        &f1.to_string_lossy(),
        "old1",
        "new1",
        "v1.0.0",
        None,
        false,
        input1.find("old1").unwrap(),
    );
    let a2 = mk_action(
        &f2.to_string_lossy(),
        "old2",
        "new2",
        "v2.0.0",
        None,
        false,
        input2.find("old2").unwrap(),
    );
    let count = apply_patches(&[a1, a2], &[0, 1]).unwrap();
    assert_eq!(2, count);

    assert!(fs::read_to_string(&f1).unwrap().contains("@new1"));
    assert!(fs::read_to_string(&f2).unwrap().contains("@new2"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn preserves_quoted_ref() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input =
        "jobs:\n  build:\n    steps:\n      - uses: \"actions/setup-node@oldsha\" # v6.2.0\n";
    fs::write(&file, input).unwrap();

    let a = mk_action(
        &file.to_string_lossy(),
        "oldsha",
        "newsha",
        "v6.4.0",
        Some("v6.2.0"),
        false,
        input.find("oldsha").unwrap(),
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("\"actions/setup-node@newsha\" # v6.4.0"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn preserves_crlf() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input =
        "jobs:\r\n  build:\r\n    steps:\r\n      - uses: actions/checkout@oldsha # v4.1.0\r\n";
    fs::write(&file, input).unwrap();

    let a = mk_action(
        &file.to_string_lossy(),
        "oldsha",
        "newsha",
        "v4.2.0",
        Some("v4.1.0"),
        false,
        input.find("oldsha").unwrap(),
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("actions/checkout@newsha # v4.2.0\r\n"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn no_comment_when_ref_equals_version() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input = "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@oldsha\n";
    fs::write(&file, input).unwrap();

    let a = mk_action(
        &file.to_string_lossy(),
        "oldsha",
        "v4.2.0",
        "v4.2.0",
        None,
        false,
        input.find("oldsha").unwrap(),
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("actions/checkout@v4.2.0\n"));
    assert!(!result.contains(" # "));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn comment_written_on_sha_mismatch() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input = "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@oldsha\n";
    fs::write(&file, input).unwrap();

    let a = mk_action(
        &file.to_string_lossy(),
        "oldsha",
        "newsha",
        "v4.2.0",
        Some("v4.1.0"),
        true,
        input.find("oldsha").unwrap(),
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("actions/checkout@newsha # v4.2.0"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn target_not_found_errors() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    fs::write(&file, "jobs:\n  build:\n    steps:\n      - uses: a/b@v1\n").unwrap();

    let a = mk_action(
        &file.to_string_lossy(),
        "not-there",
        "new",
        "v2",
        None,
        false,
        999,
    );
    let err = apply_patches(&[a], &[0]).unwrap_err();
    assert!(matches!(err, PatchError::UpdateTargetNotFound));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn preserves_user_comment() {
    let tmp = tmp_dir();
    let file = tmp.join("ci.yml");
    let input = "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@oldsha  # do not remove this\n";
    fs::write(&file, input).unwrap();

    let a = mk_action(
        &file.to_string_lossy(),
        "oldsha",
        "newsha",
        "v4.2.0",
        None,
        false,
        input.find("oldsha").unwrap(),
    );
    apply_patches(&[a], &[0]).unwrap();

    let result = fs::read_to_string(&file).unwrap();
    assert!(result.contains("actions/checkout@newsha  # do not remove this # v4.2.0"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn sorts_interleaved_files() {
    let tmp = tmp_dir();
    let f1 = tmp.join("a.yml");
    let f2 = tmp.join("b.yml");
    let input1 = "jobs:\n  build:\n    steps:\n      - uses: x/y@old1\n      - uses: x/y@old1b\n";
    let input2 = "jobs:\n  build:\n    steps:\n      - uses: x/z@old2\n";
    fs::write(&f1, input1).unwrap();
    fs::write(&f2, input2).unwrap();

    let a1 = mk_action(
        &f1.to_string_lossy(),
        "old1",
        "new1",
        "v1.0.0",
        None,
        false,
        input1.find("old1").unwrap(),
    );
    let a2 = mk_action(
        &f2.to_string_lossy(),
        "old2",
        "new2",
        "v2.0.0",
        None,
        false,
        input2.find("old2").unwrap(),
    );
    let a3 = mk_action(
        &f1.to_string_lossy(),
        "old1b",
        "new1b",
        "v1.1.0",
        None,
        false,
        input1.rfind("old1b").unwrap(),
    );
    let count = apply_patches(&[a1, a2, a3], &[0, 1, 2]).unwrap();
    assert_eq!(3, count);

    let out1 = fs::read_to_string(&f1).unwrap();
    assert!(out1.contains("@new1"));
    assert!(out1.contains("@new1b"));
    assert!(fs::read_to_string(&f2).unwrap().contains("@new2"));
    let _ = fs::remove_dir_all(&tmp);
}
