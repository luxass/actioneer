//! Command-line entry point for actioneer.

use std::{path::Path, path::PathBuf, process::ExitCode};

use actioneer::cli::{Cli, Command};
use actioneer::cmd;
use actioneer::config::{self, OutputMode};
use actioneer::tui;
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let root = Path::new(".");
    let mut cfg = match config::load(root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    cfg.apply_overrides(&cli.config);
    if let Err(e) = cfg.validate() {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    match cli.command {
        Some(Command::Update { .. }) => match cfg.mode {
            Some(OutputMode::Plain) | Some(OutputMode::Json) => {
                cmd::update::run(&cfg, cli.workflow_paths())
            }
            None => run_tui_update(&cfg, cli.workflow_paths()),
        },
        Some(Command::Audit { .. }) => cmd::audit::run(&cfg, cli.workflow_paths()),
        Some(Command::Version) => {
            cmd::version::run();
            ExitCode::SUCCESS
        }
        None => match cfg.mode {
            Some(OutputMode::Plain) | Some(OutputMode::Json) => {
                cmd::update::run(&cfg, cli.workflow_paths())
            }
            None => run_tui_update(&cfg, cli.workflow_paths()),
        },
    }
}

fn run_tui_update(cfg: &config::ActioneerConfig, workflow_paths: &[PathBuf]) -> ExitCode {
    match tui::run_app(cfg.clone(), workflow_paths.to_vec()) {
        Ok(outcome) => {
            if let Some(error) = outcome.apply_error {
                eprintln!("error: {error}");
                return ExitCode::FAILURE;
            }
            if let Some(report) = outcome.apply_report {
                cmd::update::print_apply_plain(&report, false);
                if report.failures.is_empty() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
