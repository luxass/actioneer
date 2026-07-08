use std::path::PathBuf;

use actioneer::cli::Cli;
use clap::Parser;

#[test]
fn parses_positional_workflow_path() {
    let cli = Cli::try_parse_from([
        "actioneer",
        "testdata/workflows/advanced.yml",
    ])
    .unwrap();
    assert_eq!(
        cli.workflow_paths(),
        &[PathBuf::from("testdata/workflows/advanced.yml")]
    );
    assert!(cli.command.is_none());
}

#[test]
fn parses_subcommand_with_workflow_path() {
    let cli = Cli::try_parse_from([
        "actioneer",
        "audit",
        "testdata/workflows",
    ])
    .unwrap();
    assert_eq!(
        cli.workflow_paths(),
        &[PathBuf::from("testdata/workflows")]
    );
    assert!(matches!(cli.command, Some(actioneer::cli::Command::Audit { .. })));
}

#[test]
fn parses_global_flags_before_path() {
    let cli = Cli::try_parse_from([
        "actioneer",
        "--pin",
        "sha",
        "testdata/workflows/advanced.yml",
    ])
    .unwrap();
    assert_eq!(
        cli.config.pin,
        Some(actioneer::config::PinMode::Sha)
    );
    assert_eq!(
        cli.workflow_paths(),
        &[PathBuf::from("testdata/workflows/advanced.yml")]
    );
}

#[test]
fn default_has_no_paths() {
    let cli = Cli::try_parse_from(["actioneer"]).unwrap();
    assert!(cli.workflow_paths().is_empty());
}
