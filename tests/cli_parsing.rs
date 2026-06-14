use actioneer::cli::{Cli, Command, Mode};
use clap::Parser;

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

    let default_update = Cli::try_parse_from(["actioneer", "--mode", "tui"])
        .expect("parse default update tui mode");
    assert!(default_update.command.is_none());
    assert_eq!(default_update.default_update.shared.mode, Some(Mode::Tui));
}
