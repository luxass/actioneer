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
        let plain = self.effective_mode() == Mode::Plain;
        let formatted = if plain {
            strip_ansi(msg)
        } else {
            format!("{prefix} {msg}")
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

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_sha_long() {
        assert_eq!(&"abcdef0123456789"[..12], short_sha("abcdef0123456789"));
    }

    #[test]
    fn short_sha_exact_12() {
        assert_eq!("abcdef012345", short_sha("abcdef012345"));
    }

    #[test]
    fn short_sha_short() {
        assert_eq!("abc", short_sha("abc"));
    }

    #[test]
    fn update_file_count_empty() {
        assert_eq!(0, update_file_count(&[]));
    }

    #[test]
    fn update_file_count_single_file() {
        let a = Action::from_scan(
            "o".into(),
            "n".into(),
            String::new(),
            "v1".into(),
            None,
            "ci.yml".into(),
            1,
            0,
            2,
        );
        let b = Action::from_scan(
            "o".into(),
            "n2".into(),
            String::new(),
            "v2".into(),
            None,
            "ci.yml".into(),
            2,
            0,
            2,
        );
        assert_eq!(1, update_file_count(&[a, b]));
    }

    #[test]
    fn update_file_count_multiple_files() {
        let a = Action::from_scan(
            "o".into(),
            "n".into(),
            String::new(),
            "v1".into(),
            None,
            "a.yml".into(),
            1,
            0,
            2,
        );
        let b = Action::from_scan(
            "o".into(),
            "n2".into(),
            String::new(),
            "v2".into(),
            None,
            "b.yml".into(),
            2,
            0,
            2,
        );
        assert_eq!(2, update_file_count(&[a, b]));
    }
}
