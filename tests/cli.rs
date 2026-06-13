use std::time::Duration;

use actioneer::actions::{PinStyle, UpdateMode};
use actioneer::cli::{App, Command, Mode};
use clap::Parser;

#[test]
fn root_no_args() {
    let app = App::parse_from(["actioneer"]);
    assert!(app.command.is_none());
    assert!(app.update.scan.inputs.is_empty());
    assert!(!app.global.dry_run);
    assert_eq!(Mode::Beautiful, app.global.mode);
}

#[test]
fn root_with_inputs() {
    let app = App::parse_from(["actioneer", ".github", "ci.yml"]);
    assert_eq!(2, app.update.scan.inputs.len());
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
    assert!(app.update.scan.recursive);
    assert!(app.update.scan.skip_branches);
    assert_eq!(UpdateMode::Patch, app.update.scan.update);
    assert_eq!(PinStyle::Tag, app.update.scan.pin);
    assert!(app.update.scan.yes);
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
            assert!(args.scan.recursive);
            assert_eq!(UpdateMode::Minor, args.scan.update);
            assert_eq!(PinStyle::Sha, args.scan.pin);
            assert_eq!(vec!["."], args.scan.inputs);
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

#[test]
fn filter_single() {
    let app = App::parse_from(["actioneer", "update", "--filter", "actions/checkout"]);
    match app.command {
        Some(Command::Update(args)) => {
            assert_eq!(vec!["actions/checkout"], args.scan.filters);
        }
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn filter_multiple() {
    let app = App::parse_from([
        "actioneer",
        "update",
        "--filter",
        "actions/checkout",
        "--filter",
        "actions/setup-node",
    ]);
    match app.command {
        Some(Command::Update(args)) => {
            assert_eq!(
                vec!["actions/checkout", "actions/setup-node"],
                args.scan.filters
            );
        }
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn filter_empty_by_default() {
    let app = App::parse_from(["actioneer", "update"]);
    match app.command {
        Some(Command::Update(args)) => {
            assert!(args.scan.filters.is_empty());
        }
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn min_release_age_root_update_flag() {
    let app = App::parse_from(["actioneer", "--min-release-age", "30m"]);
    assert_eq!(
        Duration::from_secs(30 * 60),
        app.update.min_release_age.unwrap().as_duration()
    );
}

#[test]
fn min_release_age_update_subcommand() {
    let app = App::parse_from(["actioneer", "update", "--min-release-age", "12h"]);
    match app.command {
        Some(Command::Update(args)) => {
            assert_eq!(
                Duration::from_secs(12 * 60 * 60),
                args.min_release_age.unwrap().as_duration()
            );
        }
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn min_release_age_days() {
    let app = App::parse_from(["actioneer", "update", "--min-release-age", "90d"]);
    match app.command {
        Some(Command::Update(args)) => {
            assert_eq!(
                Duration::from_secs(90 * 24 * 60 * 60),
                args.min_release_age.unwrap().as_duration()
            );
        }
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn min_release_age_rejects_weeks_and_months() {
    assert!(App::try_parse_from(["actioneer", "update", "--min-release-age", "2w"]).is_err());
    assert!(App::try_parse_from(["actioneer", "update", "--min-release-age", "1mo"]).is_err());
}

#[test]
fn min_release_age_is_update_only() {
    assert!(App::try_parse_from(["actioneer", "audit", "--min-release-age", "30m"]).is_err());
}

#[test]
fn filter_audit_subcommand() {
    let app = App::parse_from([
        "actioneer",
        "audit",
        "--filter",
        "actions/checkout",
        "--mode",
        "json",
    ]);
    match app.command {
        Some(Command::Audit(args)) => {
            assert_eq!(vec!["actions/checkout"], args.filters);
        }
        other => panic!("expected audit, got {other:?}"),
    }
}
