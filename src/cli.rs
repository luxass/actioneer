use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

pub use crate::config::{PinStyle, UpdateLevel};

#[derive(Debug, Parser)]
#[command(name = "actioneer", version, about)]
pub struct Cli {
    #[command(flatten)]
    pub default_update: UpdateArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Audit(AuditArgs),
    Update(UpdateArgs),
    Version,
}

#[derive(Debug, Clone, Args, Default)]
pub struct SharedArgs {
    #[arg(short = 'r', long)]
    pub recursive: bool,

    #[arg(long, value_name = "OWNER/NAME")]
    pub filter: Vec<String>,

    #[arg(long, value_name = "PATTERN")]
    pub exclude: Vec<String>,

    #[arg(long)]
    pub offline: bool,

    #[arg(long)]
    pub no_cache: bool,

    #[arg(long, value_enum)]
    pub mode: Option<Mode>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct AuditArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    #[arg(long)]
    pub fix: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct UpdateArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(short = 'y', long)]
    pub yes: bool,

    #[arg(long, value_enum)]
    pub pin: Option<PinStyle>,

    #[arg(long, value_enum)]
    pub update: Option<UpdateLevel>,

    #[arg(long)]
    pub skip_branches: bool,

    #[arg(long, value_name = "DURATION")]
    pub min_release_age: Option<String>,

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Tui,
    Plain,
    Json,
}
