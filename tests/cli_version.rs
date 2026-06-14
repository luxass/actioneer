use std::process::Command;

#[test]
fn version_prints_actioneer_version_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .arg("version")
        .output()
        .expect("run actioneer version");

    assert!(
        output.status.success(),
        "expected success, got status {:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("actioneer {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        output.stderr.is_empty(),
        "expected empty stderr, got:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
