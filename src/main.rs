use std::process::ExitCode;

use actioneer::cli::{Cli, Command};
use actioneer::cmd;
use clap::{CommandFactory, Parser};

fn main() -> ExitCode {
    let cli = Cli::parse();

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
