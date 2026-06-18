use actioneer::cli::{Cli, Command, Mode, PinStyle, UpdateLevel};
use clap::Parser;

#[test]
fn parses_shared_filter_and_exclude_flags() {
    let audit = Cli::try_parse_from([
        "actioneer",
        "audit",
        "--recursive",
        "--filter",
        "actions/checkout",
        "--filter",
        "actions/setup-node",
        "--exclude",
        "internal",
    ])
    .expect("parse audit filter flags");
    assert!(matches!(
        audit.command,
        Some(Command::Audit(args))
        if args.shared.recursive
        && args.shared.filter == vec!["actions/checkout", "actions/setup-node"]
        && args.shared.exclude == vec!["internal"]
    ));
}

#[test]
fn parses_update_specific_flags() {
    let update = Cli::try_parse_from([
        "actioneer",
        "update",
        "--yes",
        "--pin",
        "tag",
        "--update",
        "minor",
        "--skip-branches",
        "--min-release-age",
        "12h",
        "--mode",
        "json",
    ])
    .expect("parse update flags");
    assert!(matches!(
        update.command,
        Some(Command::Update(args))
        if args.yes
        && args.pin == Some(PinStyle::Tag)
        && args.update == Some(UpdateLevel::Minor)
        && args.skip_branches
        && args.min_release_age == Some("12h".to_string())
        && args.shared.mode == Some(Mode::Json)
    ));
}

#[test]
fn parses_target_commands_and_mode_overrides() {
    let audit = Cli::try_parse_from(["actioneer", "audit", "--mode", "json"])
        .expect("parse audit json mode");
    assert!(matches!(
        audit.command,
        Some(Command::Audit(args)) if args.shared.mode == Some(Mode::Json)
    ));

    let update = Cli::try_parse_from(["actioneer", "update", "--mode", "plain"])
        .expect("parse update plain mode");
    assert!(matches!(
        update.command,
        Some(Command::Update(args)) if args.shared.mode == Some(Mode::Plain)
    ));

    let default_update =
        Cli::try_parse_from(["actioneer", "--mode", "tui"]).expect("parse default update tui mode");
    assert!(default_update.command.is_none());
    assert_eq!(default_update.default_update.shared.mode, Some(Mode::Tui));
}
