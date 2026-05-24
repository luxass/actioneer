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
        self.write_human(message.as_ref(), Level::Debug);
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref(), Level::Info);
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref(), Level::Warn);
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.write_human(message.as_ref(), Level::Error);
    }

    pub fn json(&self, message: impl AsRef<str>) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{}", message.as_ref());
    }

    fn write_human(&self, message: &str, level: Level) {
        let formatted = format_human(message, level, self.effective_human_mode());

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

fn format_human(message: &str, level: Level, mode: Mode) -> String {
    let prefixed = if mode == Mode::Beautiful {
        format!("{} {}", level.prefix(), message)
    } else {
        message.to_string()
    };

    if mode == Mode::Plain {
        strip_ansi(&prefixed)
    } else {
        prefixed
    }
}

#[derive(Clone, Copy)]
enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

use owo_colors::OwoColorize;

impl Level {
    fn prefix(&self) -> String {
        match self {
            Level::Debug => format!("{}", "·".bright_black()),
            Level::Info => format!("{}", "›".cyan()),
            Level::Warn => format!("{}", "!".yellow()),
            Level::Error => format!("{}", "✗".red()),
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
    use crate::cli::Mode;

    use super::{Level, format_human, strip_ansi};

    #[test]
    fn plain_mode_strips_ansi_and_omits_prefix() {
        assert_eq!(
            "hello world",
            format_human("\x1b[31mhello\x1b[0m world", Level::Info, Mode::Plain)
        );
    }

    #[test]
    fn beautiful_mode_adds_level_prefix() {
        let formatted = format_human("hello", Level::Info, Mode::Beautiful);

        assert!(formatted.contains("hello"));
        assert!(formatted.starts_with('\u{1b}') || formatted.starts_with('›'));
    }

    #[test]
    fn json_mode_keeps_human_diagnostics_unprefixed() {
        assert_eq!("hello", format_human("hello", Level::Info, Mode::Json));
    }

    #[test]
    fn strip_ansi_removes_escape_sequences() {
        assert_eq!(
            "hello world",
            strip_ansi("\x1b[31mhello\x1b[0m \x1b[1mworld\x1b[0m")
        );
    }
}
