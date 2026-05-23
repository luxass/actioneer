use crate::model::UpdateMode;
use clap::{
    builder::styling::{AnsiColor, Effects, Styles},
    Args, Parser, Subcommand, ValueEnum,
};

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
    pub update: UpdateArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Update(UpdateArgs),
    Audit(AuditArgs),
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
pub struct UpdateArgs {
    #[arg(long, short = 'r', default_value_t = false)]
    pub recursive: bool,

    #[arg(long = "skip-branches", default_value_t = false)]
    pub skip_branches: bool,

    #[arg(long = "update", value_enum, default_value_t = UpdateMode::Major)]
    pub update: UpdateMode,

    #[arg(long, default_value_t = false)]
    pub tag: bool,

    #[arg(long, short = 'y', default_value_t = false)]
    pub yes: bool,

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<String>,
}

#[derive(Clone, Debug, Args)]
pub struct AuditArgs {
    #[arg(long, short = 'r', default_value_t = false)]
    pub recursive: bool,

    #[arg(long = "skip-branches", default_value_t = false)]
    pub skip_branches: bool,

    #[arg(long = "update", value_enum, default_value_t = UpdateMode::Major)]
    pub update: UpdateMode,

    #[arg(long, default_value_t = false)]
    pub tag: bool,

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Mode {
    Plain,
    Json,
    Beautiful,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::model::UpdateMode;

    use super::{App, Command, Mode};

    #[test]
    fn root_command_uses_update_args() {
        let app = App::parse_from(["actioneer", ".github/workflows"]);

        assert!(app.command.is_none());
        assert_eq!(vec![String::from(".github/workflows")], app.update.inputs);
        assert!(!app.global.dry_run);
        assert!(!app.global.no_cache);
        assert_eq!(Mode::Beautiful, app.global.mode);
    }

    #[test]
    fn explicit_update_command_uses_update_args() {
        let app = App::parse_from([
            "actioneer",
            "update",
            "--update",
            "patch",
            "--tag",
            ".github",
        ]);

        match app.command {
            Some(Command::Update(args)) => {
                assert_eq!(vec![String::from(".github")], args.inputs);
                assert_eq!(UpdateMode::Patch, args.update);
                assert!(args.tag);
            }
            other => panic!("expected update command, got {other:?}"),
        }
    }

    #[test]
    fn audit_command_uses_audit_args() {
        let app = App::parse_from([
            "actioneer",
            "audit",
            "--recursive",
            "--update",
            "minor",
            ".",
        ]);

        match app.command {
            Some(Command::Audit(args)) => {
                assert!(args.recursive);
                assert_eq!(UpdateMode::Minor, args.update);
                assert!(!args.tag);
                assert_eq!(vec![String::from(".")], args.inputs);
            }
            other => panic!("expected audit command, got {other:?}"),
        }
    }

    #[test]
    fn root_command_defaults_update_mode_to_major() {
        let app = App::parse_from(["actioneer", ".github/workflows"]);

        assert_eq!(UpdateMode::Major, app.update.update);
        assert!(!app.update.tag);
    }

    #[test]
    fn global_args_work_for_root_update() {
        let app = App::parse_from([
            "actioneer",
            "--dry-run",
            "--exclude",
            "actions/cache",
            "--no-cache",
            "--mode",
            "json",
            ".github",
        ]);

        assert!(app.command.is_none());
        assert!(app.global.dry_run);
        assert!(app.global.no_cache);
        assert_eq!(vec![String::from("actions/cache")], app.global.excludes);
        assert_eq!(Mode::Json, app.global.mode);
        assert_eq!(vec![String::from(".github")], app.update.inputs);
    }

    #[test]
    fn global_args_work_after_subcommand() {
        let app = App::parse_from([
            "actioneer",
            "audit",
            "--dry-run",
            "--no-cache",
            "--exclude",
            "actions/cache",
            "--mode",
            "plain",
            ".",
        ]);

        assert!(app.global.dry_run);
        assert!(app.global.no_cache);
        assert_eq!(vec![String::from("actions/cache")], app.global.excludes);
        assert_eq!(Mode::Plain, app.global.mode);
        match app.command {
            Some(Command::Audit(args)) => {
                assert_eq!(vec![String::from(".")], args.inputs);
            }
            other => panic!("expected audit command, got {other:?}"),
        }
    }
}
