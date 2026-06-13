use std::time::Duration;

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
    pub update: UpdateArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Update(UpdateArgs),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MinReleaseAge(Duration);

impl MinReleaseAge {
    pub fn from_duration(duration: Duration) -> Self {
        Self(duration)
    }

    pub fn as_duration(self) -> Duration {
        self.0
    }
}

#[derive(Clone, Debug, Args)]
pub struct UpdateArgs {
    #[command(flatten)]
    pub scan: ScanArgs,

    #[arg(
        long = "min-release-age",
        value_name = "DURATION",
        value_parser = parse_min_release_age,
        help = "Skip update tags newer than this age (e.g. 30m, 12h, 7d)"
    )]
    pub min_release_age: Option<MinReleaseAge>,
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

fn parse_min_release_age(value: &str) -> Result<MinReleaseAge, String> {
    let Some(unit) = value.chars().last() else {
        return Err("duration must use m, h, or d units".into());
    };
    let amount = &value[..value.len() - unit.len_utf8()];
    if amount.is_empty() || !amount.bytes().all(|b| b.is_ascii_digit()) {
        return Err("duration must be a positive integer followed by m, h, or d".into());
    }

    let amount: u64 = amount
        .parse()
        .map_err(|_| "duration amount is too large".to_string())?;
    if amount == 0 {
        return Err("duration amount must be greater than zero".into());
    }

    let seconds = match unit {
        'm' => amount.checked_mul(60),
        'h' => amount.checked_mul(60 * 60),
        'd' => amount.checked_mul(60 * 60 * 24),
        _ => return Err("duration must use m, h, or d units".into()),
    }
    .ok_or_else(|| "duration amount is too large".to_string())?;

    Ok(MinReleaseAge(Duration::from_secs(seconds)))
}
