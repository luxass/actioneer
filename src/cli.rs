use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "actioneer", version, about = "GitHub Actions CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Audit,
    Update,
    Version,
}
