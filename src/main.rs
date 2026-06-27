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
            None => {
                if let Err(e) = tui::run_app(cfg) {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            }
        },
        Some(Command::Audit) => cmd::audit::run(&cfg),
        Some(Command::Version) => {
            cmd::version::run();
            ExitCode::SUCCESS
        }
        None => match cfg.mode {
            Some(OutputMode::Plain) | Some(OutputMode::Json) => cmd::update::run(&cfg),
            None => {
                if let Err(e) = tui::run_app(cfg) {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            }
        },
    }
}
