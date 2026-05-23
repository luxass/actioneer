mod cli;
mod cmd;
mod engine;
mod errors;
mod github;
mod logger;
mod model;
mod syntax;
mod ui;

use std::process::ExitCode;

use clap::Parser;

use crate::cli::{App, Command};

fn main() -> ExitCode {
    let app = App::parse();
    match run(app) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run(app: App) -> Result<ExitCode, errors::Error> {
    let global = app.global.clone();
    match app.command {
        Some(Command::Update(args)) => Ok(cmd::update::run(global, args)?),
        Some(Command::Audit(args)) => Ok(cmd::audit::run(global, args)?),
        Some(Command::Version) => Ok(cmd::version::run()),
        None => Ok(cmd::update::run(global, app.update)?),
    }
}
