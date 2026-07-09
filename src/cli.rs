//! Clap argument types shared by the actioneer binary and tests.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::{OutputMode, PinMode, RelativeDuration, UpdateLevel};

#[derive(Debug, Args, Default)]
/// Positional workflow files or flat directories supplied to a command.
pub struct WorkflowPathArgs {
    /// Workflow file(s) or directory to scan (default: .github/workflows/)
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Parser)]
#[command(
    name = "actioneer",
    version,
    about = "GitHub Actions CLI",
    args_conflicts_with_subcommands = true
)]
/// Parsed command line for actioneer.
pub struct Cli {
    /// Explicit subcommand, or `None` for the default update command.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Workflow file(s) or directory to scan (default: .github/workflows/)
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Configuration overrides supplied on the command line.
    #[command(flatten)]
    pub config: ConfigArgs,
}

impl Cli {
    /// Resolved workflow targets for the active command.
    pub fn workflow_paths(&self) -> &[PathBuf] {
        match &self.command {
            Some(Command::Audit { workflow_paths }) => &workflow_paths.paths,
            Some(Command::Update { workflow_paths }) => &workflow_paths.paths,
            Some(Command::Version) => &[],
            None => &self.paths,
        }
    }
}

#[derive(Debug, Subcommand)]
/// Commands supported by the actioneer binary.
pub enum Command {
    /// Audit workflow references and return a failing status when issues exist.
    Audit {
        /// Explicit workflows or directories to audit.
        #[command(flatten)]
        workflow_paths: WorkflowPathArgs,
    },
    /// Plan, display, or apply workflow-reference updates.
    Update {
        /// Explicit workflows or directories to update.
        #[command(flatten)]
        workflow_paths: WorkflowPathArgs,
    },
    /// Print the compiled package version.
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

    /// Write planned updates to workflow files
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    pub apply: Option<bool>,

    /// Preview file writes without modifying workflows (implies --apply)
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    pub dry_run: Option<bool>,
}
