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
    match app.command {
        Some(Command::Update(args)) => {
            let gh = GitHubClient::new(!global.no_cache);
            cmd::update::run(global, args, gh)
        }
        Some(Command::Audit(args)) => {
            let gh = GitHubClient::new(!global.no_cache);
            cmd::audit::run(global, args, gh)
        }
        Some(Command::Version) => Ok(cmd::version::run()),
        None => {
            let gh = GitHubClient::new(!global.no_cache);
            cmd::update::run(global, app.update, gh)
        }
    }
}
