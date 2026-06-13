use clap::{
    Args, Parser, Subcommand, ValueEnum,
    builder::styling::{AnsiColor, Effects, Styles},
};

use crate::actions::{PinStyle, UpdateMode};

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

    #[arg(long = "filter", value_name = "OWNER/NAME")]
    pub filters: Vec<String>,

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
