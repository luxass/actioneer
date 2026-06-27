use clap::{Args, Parser, Subcommand};

use crate::config::{OutputMode, PinMode, RelativeDuration, UpdateLevel};

#[derive(Debug, Parser)]
#[command(name = "actioneer", version, about = "GitHub Actions CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub config: ConfigArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Audit,
    Update,
    Version,
}

/// CLI overrides for config-file settings.
///
/// Any value provided here takes precedence over the loaded `actioneer.toml`.
#[derive(Debug, Default, Args)]
pub struct ConfigArgs {
    /// Pin mode: "sha" or "tag"
    #[arg(long, global = true, value_name = "MODE")]
    pub pin: Option<PinMode>,

    /// Update level: "major", "minor", or "patch"
    #[arg(long, global = true, value_name = "LEVEL")]
    pub update: Option<UpdateLevel>,

    /// Skip processing branches (pass --skip-branches=false to explicitly disable)
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    pub skip_branches: Option<bool>,

    /// Minimum release age before considering an update (e.g. 7d, 4h, 30m)
    #[arg(long = "min-release-age", global = true, value_name = "DURATION")]
    pub min_release_age: Option<RelativeDuration>,

    /// Never perform network requests; use the local cache only
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    pub offline: Option<bool>,

    /// Bypass cache reads/writes; always fetch fresh data from the network
    #[arg(long = "no-cache", global = true, num_args = 0..=1, default_missing_value = "true")]
    pub no_cache: Option<bool>,

    /// Output mode: "plain" or "json" (update uses TUI unless overridden)
    #[arg(long, global = true, value_name = "MODE")]
    pub mode: Option<OutputMode>,
}
