use actioneer::actions::{PinStyle, UpdateMode};
use actioneer::cli::{App, Command, Mode};
use clap::Parser;

#[test]
fn root_no_args() {
    let app = App::parse_from(["actioneer"]);
    assert!(app.command.is_none());
    assert!(app.update.inputs.is_empty());
    assert!(!app.global.dry_run);
    assert_eq!(Mode::Beautiful, app.global.mode);
}

#[test]
fn root_with_inputs() {
    let app = App::parse_from(["actioneer", ".github", "ci.yml"]);
    assert_eq!(2, app.update.inputs.len());
}

#[test]
fn root_with_flags() {
    let app = App::parse_from([
        "actioneer",
        "-r",
        "--skip-branches",
        "--update",
        "patch",
        "--pin",
        "tag",
        "--yes",
    ]);
    assert!(app.update.recursive);
    assert!(app.update.skip_branches);
    assert_eq!(UpdateMode::Patch, app.update.update);
    assert_eq!(PinStyle::Tag, app.update.pin);
    assert!(app.update.yes);
}

#[test]
fn update_subcommand() {
    let app = App::parse_from([
        "actioneer",
        "update",
        "-r",
        "--update",
        "minor",
        "--pin",
        "sha",
        ".",
    ]);
    match app.command {
        Some(Command::Update(args)) => {
            assert!(args.recursive);
            assert_eq!(UpdateMode::Minor, args.update);
            assert_eq!(PinStyle::Sha, args.pin);
            assert_eq!(vec!["."], args.inputs);
        }
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn audit_subcommand() {
    let app = App::parse_from(["actioneer", "audit", "--recursive", "--skip-branches", "."]);
    match app.command {
        Some(Command::Audit(args)) => {
            assert!(args.recursive);
            assert!(args.skip_branches);
        }
        other => panic!("expected audit, got {other:?}"),
    }
}

#[test]
fn version_subcommand() {
    let app = App::parse_from(["actioneer", "version"]);
    assert!(matches!(app.command, Some(Command::Version)));
}

#[test]
fn global_flags() {
    let app = App::parse_from(["actioneer", "audit", "--dry-run", "--no-cache", "."]);
    assert!(app.global.dry_run);
    assert!(app.global.no_cache);
}

#[test]
fn global_mode() {
    assert_eq!(
        Mode::Json,
        App::parse_from(["actioneer", "--mode", "json"]).global.mode
    );
    assert_eq!(
        Mode::Plain,
        App::parse_from(["actioneer", "--mode", "plain"])
            .global
            .mode
    );
}

#[test]
fn global_excludes() {
    let app = App::parse_from(["actioneer", "--exclude", "a", "--exclude", "b"]);
    assert_eq!(vec!["a", "b"], app.global.excludes);
}

#[test]
fn mode_is_json() {
    assert!(Mode::Json.is_json());
    assert!(!Mode::Plain.is_json());
    assert!(!Mode::Beautiful.is_json());
}
