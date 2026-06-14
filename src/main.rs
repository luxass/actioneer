use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("version") => match actioneer::cmd::version::run(std::io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("failed to print version: {error}");
                ExitCode::FAILURE
            }
        },
        Some(command) => {
            eprintln!("unknown command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!("update flow is not implemented yet");
            ExitCode::from(2)
        }
    }
}
