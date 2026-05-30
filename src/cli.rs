use clap::{Args, Parser, Subcommand, ValueEnum, builder::styling::{AnsiColor, Effects, Styles}};

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
    fn root_command_uses_scan_args() {
        let app = App::parse_from(["actioneer", ".github/workflows"]);
        assert!(app.command.is_none());
        assert_eq!(vec![".github/workflows".to_string()], app.update.inputs);
        assert!(!app.global.dry_run);
        assert_eq!(Mode::Beautiful, app.global.mode);
    }

    #[test]
    fn explicit_update_command() {
        let app = App::parse_from([
            "actioneer", "update", "--update", "patch", "--pin", "tag", ".github",
        ]);
        match app.command {
            Some(Command::Update(args)) => {
                assert_eq!(vec![".github".to_string()], args.inputs);
                assert_eq!(UpdateMode::Patch, args.update);
                assert_eq!(PinStyle::Tag, args.pin);
            }
            other => panic!("expected update command, got {other:?}"),
        }
    }

    #[test]
    fn audit_command() {
        let app = App::parse_from([
            "actioneer", "audit", "--recursive", "--update", "minor", ".",
        ]);
        match app.command {
            Some(Command::Audit(args)) => {
                assert!(args.recursive);
                assert_eq!(UpdateMode::Minor, args.update);
            }
            other => panic!("expected audit command, got {other:?}"),
        }
    }

    #[test]
    fn global_args_work_after_subcommand() {
        let app = App::parse_from([
            "actioneer", "audit", "--dry-run", "--no-cache", "--mode", "plain", ".",
        ]);
        assert!(app.global.dry_run);
        assert!(app.global.no_cache);
        assert_eq!(Mode::Plain, app.global.mode);
    }
}
