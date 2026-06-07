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
    match app.command {
        Some(Command::Version) => Ok(cmd::version::run()),
        Some(Command::Audit(args)) => {
            cmd::audit::run(global, args, gh)
        }
        Some(Command::Update(args)) => {
            cmd::update::run(global, args, gh)
        }
        None => {
            cmd::update::run(global, app.update, gh)
        }
    }
}
