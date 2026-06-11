use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};

use owo_colors::OwoColorize;

use crate::actions::ActionUpdate;
use crate::cli::Mode;

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

pub fn print_json(actions: &[ActionUpdate]) {
    let json = serde_json::to_string(&serde_json::json!({ "updates": actions }))
        .expect("serializing updates");
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", json);
}

pub fn update_file_count(actions: &[ActionUpdate]) -> usize {
    actions
        .iter()
        .map(|a| a.action.file.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

pub fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

pub(crate) fn strip_ansi(input: &str) -> String {
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
    use owo_colors::OwoColorize;

    use super::*;

    #[test]
    fn strip_ansi_passes_plain_text_through() {
        assert_eq!("hello world", strip_ansi("hello world"));
        assert_eq!("", strip_ansi(""));
    }

    #[test]
    fn strip_ansi_removes_color_codes() {
        let colored = format!("{} and {}", "red".red(), "bold".bold());
        assert_eq!("red and bold", strip_ansi(&colored));
    }

    #[test]
    fn strip_ansi_keeps_escape_without_bracket() {
        assert_eq!("a\x1bb", strip_ansi("a\x1bb"));
    }

    #[test]
    fn short_sha_is_at_most_twelve_chars() {
        assert_eq!("abcdef012345", short_sha("abcdef0123456789abcdef"));
        assert_eq!("abc", short_sha("abc"));
        assert_eq!("", short_sha(""));
    }
}
