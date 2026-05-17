use std::io::{self, IsTerminal, Write};

use crate::cli::Mode;

pub struct Logger {
    mode: Mode,
}

impl Logger {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }

    pub fn is_json(&self) -> bool {
        self.mode == Mode::Json
    }

    pub fn debug(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref());
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref());
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref());
    }

    pub fn json(&self, message: impl AsRef<str>) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{}", message.as_ref());
    }

    fn write_human(&self, message: &str) {
        let formatted = if self.effective_human_mode() == Mode::Plain {
            strip_ansi(message)
        } else {
            message.to_string()
        };

        if self.is_json() {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "{formatted}");
        } else {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "{formatted}");
        }
    }

    fn effective_human_mode(&self) -> Mode {
        match self.mode {
            Mode::Plain => Mode::Plain,
            Mode::Json => {
                if supports_color_on_stderr() {
                    Mode::Json
                } else {
                    Mode::Plain
                }
            }
            Mode::Beautiful => {
                if supports_color_on_stdout() {
                    Mode::Beautiful
                } else {
                    Mode::Plain
                }
            }
        }
    }
}

fn supports_color_on_stdout() -> bool {
    color_enabled() && io::stdout().is_terminal()
}

fn supports_color_on_stderr() -> bool {
    color_enabled() && io::stderr().is_terminal()
}

fn color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
}

fn strip_ansi(input: &str) -> String {
    let mut filtered = String::with_capacity(input.len());
    let mut in_escape = false;

    for ch in input.chars() {
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        if ch == '\u{1b}' {
            in_escape = true;
            continue;
        }
        filtered.push(ch);
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn strip_ansi_removes_escape_sequences() {
        assert_eq!(
            "hello world",
            strip_ansi("\x1b[31mhello\x1b[0m \x1b[1mworld\x1b[0m")
        );
    }
}
