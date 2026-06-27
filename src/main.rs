use std::{path::Path, process::ExitCode};

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
        Some(Command::Update) => match cfg.mode {
            Some(OutputMode::Plain) | Some(OutputMode::Json) => cmd::update::run(&cfg),
            None => run_tui_update(&cfg),
        },
        Some(Command::Audit) => cmd::audit::run(&cfg),
        Some(Command::Version) => {
            cmd::version::run();
            ExitCode::SUCCESS
        }
        None => match cfg.mode {
            Some(OutputMode::Plain) | Some(OutputMode::Json) => cmd::update::run(&cfg),
            None => run_tui_update(&cfg),
        },
    }
}

fn run_tui_update(cfg: &config::ActioneerConfig) -> ExitCode {
    match tui::run_app(cfg.clone()) {
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
