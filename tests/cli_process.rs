//! Process-level tests for update command routing.

use std::process::{Command, Output};

fn run_actioneer(args: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();

    Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .args(args)
        .current_dir(dir.path())
        .output()
        .unwrap()
}

fn assert_success(output: &Output) -> (String, String) {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    assert!(
        output.status.success(),
        "command failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    assert!(stderr.is_empty(), "unexpected stderr:\n{stderr}");

    (stdout, stderr)
}

#[test]
fn update_dry_run_uses_non_interactive_apply_path() {
    let output = run_actioneer(&["update", "--dry-run"]);
    let (stdout, _) = assert_success(&output);

    assert!(stdout.contains("Dry run — no files modified."));
    assert!(stdout.contains("No updates to apply."));
}

#[test]
fn update_apply_uses_non_interactive_apply_path() {
    let output = run_actioneer(&["update", "--apply"]);
    let (stdout, _) = assert_success(&output);

    assert!(stdout.contains("No updates to apply."));
    assert!(!stdout.contains("Dry run — no files modified."));
}

#[test]
fn update_plain_mode_remains_non_interactive() {
    let output = run_actioneer(&["update", "--mode", "plain"]);
    let (stdout, _) = assert_success(&output);

    assert!(stdout.contains("No updates planned."));
    assert!(stdout.contains("Scanned 0 workflow(s), 0 reference(s)."));
}
