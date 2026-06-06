use clap::{
    Args, Parser, Subcommand, ValueEnum,
    builder::styling::{AnsiColor, Effects, Styles},
};

use crate::model::{PinStyle, UpdateMode};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Debug, Parser)]
#[command(name = "actioneer", about, version, styles = STYLES)]
pub struct App {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(flatten)]
    pub update: ScanArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Update(ScanArgs),
    Audit(ScanArgs),
    Version,
}

#[derive(Clone, Debug, Args)]
pub struct GlobalArgs {
    #[arg(long, global = true, default_value_t = false)]
    pub dry_run: bool,

    #[arg(long = "no-cache", global = true, default_value_t = false)]
    pub no_cache: bool,

    #[arg(long = "exclude", global = true)]
    pub excludes: Vec<String>,

    #[arg(long, global = true, value_enum, default_value_t = Mode::Beautiful)]
    pub mode: Mode,
}

#[derive(Clone, Debug, Args)]
pub struct ScanArgs {
    #[arg(long, short = 'r', default_value_t = false)]
    pub recursive: bool,

    #[arg(long = "skip-branches", default_value_t = false)]
    pub skip_branches: bool,

    #[arg(long = "update", value_enum, default_value_t = UpdateMode::Major)]
    pub update: UpdateMode,

    #[arg(long = "pin", value_enum, default_value = "sha")]
    pub pin: PinStyle,

    #[arg(long, short = 'y', default_value_t = false)]
    pub yes: bool,

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Mode {
    Plain,
    Json,
    Beautiful,
}

impl Mode {
    pub fn is_json(self) -> bool {
        self == Mode::Json
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

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
    fn global_dry_run() {
        let app = App::parse_from(["actioneer", "audit", "--dry-run", "."]);
        assert!(app.global.dry_run);
    }

    #[test]
    fn global_no_cache() {
        let app = App::parse_from(["actioneer", "--no-cache"]);
        assert!(app.global.no_cache);
    }

    #[test]
    fn global_mode_json() {
        let app = App::parse_from(["actioneer", "--mode", "json"]);
        assert_eq!(Mode::Json, app.global.mode);
    }

    #[test]
    fn global_mode_plain() {
        let app = App::parse_from(["actioneer", "--mode", "plain"]);
        assert_eq!(Mode::Plain, app.global.mode);
    }

    #[test]
    fn global_exclude_single() {
        let app = App::parse_from(["actioneer", "--exclude", "actions/checkout"]);
        assert_eq!(vec!["actions/checkout"], app.global.excludes);
    }

    #[test]
    fn global_exclude_multiple() {
        let app = App::parse_from(["actioneer", "--exclude", "a", "--exclude", "b"]);
        assert_eq!(vec!["a", "b"], app.global.excludes);
    }

    #[test]
    fn global_after_subcommand() {
        let app = App::parse_from(["actioneer", "audit", "--dry-run", "--no-cache", "."]);
        assert!(app.global.dry_run);
        assert!(app.global.no_cache);
    }

    #[test]
    fn mode_is_json() {
        assert!(Mode::Json.is_json());
        assert!(!Mode::Plain.is_json());
        assert!(!Mode::Beautiful.is_json());
    }
}
