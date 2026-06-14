use std::{fs, process::Command, time::SystemTime};

#[test]
fn config_offline_conflicts_with_cli_no_cache_before_running_command() {
    let workspace = temp_workspace("actioneer-config-conflict");
    fs::write(workspace.join(".actioneer.toml"), "offline = true\n").expect("write config");

    let output = Command::new(env!("CARGO_BIN_EXE_actioneer"))
        .current_dir(&workspace)
        .args(["audit", "--no-cache"])
        .output()
        .expect("run actioneer audit");

    assert!(!output.status.success(), "expected conflict to fail");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("offline"),
        "stderr should mention offline: {stderr}"
    );
    assert!(
        stderr.contains("no-cache"),
        "stderr should mention no-cache: {stderr}"
    );
    assert!(
        !stderr.contains("audit flow is not implemented yet"),
        "conflict should be reported before audit runs: {stderr}"
    );
}

fn temp_workspace(prefix: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&path).expect("create temp workspace");
    path
}
