mod cli;
mod cmd;
mod display;
mod github;
mod model;
mod prompt;
mod resolve;
mod rewrite;
mod scan;

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

fn run(app: App) -> anyhow::Result<ExitCode> {
    let global = app.global.clone();
    match app.command {
        Some(Command::Update(args)) => cmd::update::run(global, args),
        Some(Command::Audit(args)) => cmd::audit::run(global, args),
        Some(Command::Version) => Ok(cmd::version::run()),
        None => cmd::update::run(global, app.update),
    }
}
