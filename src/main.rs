use std::process::ExitCode;

use actioneer::cli::{App, Command};
use actioneer::cmd;
use actioneer::github::GitHubClient;
use clap::Parser;

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
    let gh = GitHubClient::new(!global.no_cache);
    match app.command.unwrap_or(Command::Update(app.update)) {
        Command::Version => Ok(cmd::version::run()),
        Command::Audit(args) => cmd::audit::run(global, args, gh),
        Command::Update(args) => cmd::update::run(global, args, gh),
    }
}
