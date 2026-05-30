use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

use owo_colors::OwoColorize;

use crate::cli::Mode;
use crate::model::Action;

pub struct Printer {
    mode: Mode,
}

impl Printer {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }

    pub fn info(&self, msg: &str) {
        self.write(msg, "›".cyan().to_string());
    }

    pub fn warn(&self, msg: &str) {
        self.write(msg, "!".yellow().to_string());
    }

    pub fn error(&self, msg: &str) {
        self.write(msg, "✗".red().to_string());
    }

    pub fn debug(&self, msg: &str) {
        self.write(msg, "·".bright_black().to_string());
    }

    fn write(&self, msg: &str, prefix: String) {
        let formatted = match self.effective_mode() {
            Mode::Plain => msg.to_string(),
            Mode::Beautiful => format!("{prefix} {msg}"),
            Mode::Json => msg.to_string(),
        };

        if self.mode.is_json() {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "{formatted}");
        } else {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "{formatted}");
        }
    }

    fn effective_mode(&self) -> Mode {
        match self.mode {
            Mode::Plain => Mode::Plain,
            Mode::Json => {
                if color_enabled() && io::stderr().is_terminal() {
                    Mode::Beautiful
                } else {
                    Mode::Plain
                }
            }
            Mode::Beautiful => {
                if color_enabled() && io::stdout().is_terminal() {
                    Mode::Beautiful
                } else {
                    Mode::Plain
                }
            }
        }
    }
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

pub fn print_json(actions: &[Action]) {
    let json = serde_json::to_string(&serde_json::json!({ "updates": actions }))
        .expect("serializing updates");
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", json);
}

pub fn update_file_count(actions: &[Action]) -> usize {
    actions
        .iter()
        .map(|a| a.file.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

pub fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}
