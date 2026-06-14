use std::process::ExitCode;

use actioneer::cli::{Cli, Command};
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Audit(args)) => report_result(actioneer::cmd::audit::run(args)),
        Some(Command::Update(args)) => report_result(actioneer::cmd::update::run(args)),
        Some(Command::Version) => match actioneer::cmd::version::run(std::io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("failed to print version: {error}");
                ExitCode::FAILURE
            }
        },
        None => report_result(actioneer::cmd::update::run(&cli.default_update)),
    }
}

fn report_result(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
