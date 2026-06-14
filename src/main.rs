use std::process::ExitCode;

use actioneer::cli::{Cli, Command};
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match &cli.command {
        Some(Command::Audit(args)) => match actioneer::config::load_for_command(&args.shared) {
            Ok(_config) => report_result(actioneer::cmd::audit::run(args)),
            Err(error) => report_result(Err(error)),
        },
        Some(Command::Update(args)) => match actioneer::config::load_for_command(&args.shared) {
            Ok(_config) => report_result(actioneer::cmd::update::run(args)),
            Err(error) => report_result(Err(error)),
        },
        Some(Command::Version) => match actioneer::cmd::version::run(std::io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("failed to print version: {error}");
                ExitCode::FAILURE
            }
        },
        None => match actioneer::config::load_for_command(&cli.default_update.shared) {
            Ok(_config) => report_result(actioneer::cmd::update::run(&cli.default_update)),
            Err(error) => report_result(Err(error)),
        },
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
