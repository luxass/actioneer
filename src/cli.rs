use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

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

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct UpdateArgs {
    #[command(flatten)]
    pub shared: SharedArgs,

    #[arg(value_name = "INPUT")]
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    Tui,
    Plain,
    Json,
}
