use std::{path::Path, process::ExitCode};

use actioneer::cli::{Cli, Command};
use actioneer::cmd;
use actioneer::config;
use clap::{CommandFactory, Parser};

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
        Some(Command::Audit) => cmd::audit::run(),
        Some(Command::Update) => cmd::update::run(),
        Some(Command::Version) => cmd::version::run(),
        None => {
            let mut help_cmd = Cli::command();
            let _ = help_cmd.print_help();
            println!();
        }
    }

    ExitCode::SUCCESS
}
