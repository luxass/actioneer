//! Lightweight ANSI styling when stdout/stderr is a TTY.

use std::io::{self, IsTerminal};

/// Terminal colour helpers — no-op when the target stream is not a TTY.
#[derive(Clone, Copy)]
pub struct Colors {
    enabled: bool,
}

impl Colors {
    pub fn stdout() -> Self {
        Self {
            enabled: io::stdout().is_terminal(),
        }
    }

    fn paint(self, code: &str, s: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn workflow(self, s: &str) -> String {
        self.paint("36", s)
    }

    pub fn action(self, s: &str) -> String {
        self.paint("33", s)
    }

    pub fn from(self, s: &str) -> String {
        self.paint("90", s)
    }

    pub fn to(self, s: &str) -> String {
        self.paint("32", s)
    }

    pub fn warn(self, s: &str) -> String {
        self.paint("33", s)
    }

    pub fn error(self, s: &str) -> String {
        self.paint("31", s)
    }

    pub fn dim(self, s: &str) -> String {
        self.paint("2", s)
    }

    pub fn bold(self, s: &str) -> String {
        self.paint("1", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_passthrough() {
        let c = Colors { enabled: false };
        assert_eq!(c.workflow("ci.yml"), "ci.yml");
        assert_eq!(c.to("v4"), "v4");
    }

    #[test]
    fn enabled_wraps_codes() {
        let c = Colors { enabled: true };
        assert_eq!(c.to("v4"), "\x1b[32mv4\x1b[0m");
        assert_eq!(c.from("v3"), "\x1b[90mv3\x1b[0m");
    }
}
