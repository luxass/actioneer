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

pub fn sha_mismatch_line(action: &ActionUpdate) -> String {
    let mut line = format!(
        "{} at {}:{} uses {}",
        action.action_name().bold(),
        action.action.file.cyan(),
        action.action.line,
        action.action.current_ref.red()
    );
    if let Some(vc) = &action.action.version_comment {
        line.push_str(&format!(" but says {}", vc.yellow()));
    }
    if !action.expected_sha.is_empty() {
        line.push_str(&format!(
            "; expected {}",
            short_sha(&action.expected_sha).green()
        ));
    }
    line
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
    use super::*;
    use crate::actions::{ActionReference, WorkflowEdit};

    #[test]
    fn sha_mismatch_line_includes_comment_and_expected_sha() {
        let action = action_update(
            "de0fac2e4500dabe0009e67214ff5f5447ce83dd",
            Some("v6.0.2"),
            "df4cb1c069e1874edd31b4311f1884172cec0e10",
        );

        assert_eq!(
            "actions/checkout at .github/workflows/ci.yaml:37 uses de0fac2e4500dabe0009e67214ff5f5447ce83dd but says v6.0.2; expected df4cb1c069e1",
            strip_ansi(&sha_mismatch_line(&action))
        );
    }

    fn action_update(
        current_ref: &str,
        version_comment: Option<&str>,
        expected_sha: &str,
    ) -> ActionUpdate {
        ActionUpdate {
            action: ActionReference {
                owner: "actions".into(),
                name: "checkout".into(),
                path: String::new(),
                current_ref: current_ref.into(),
                version_comment: version_comment.map(str::to_string),
                file: ".github/workflows/ci.yaml".into(),
                line: 37,
                edit: WorkflowEdit::new(0, 0),
            },
            new_ref: expected_sha.into(),
            new_version: "v6.0.3".into(),
            expected_sha: expected_sha.into(),
            sha_mismatch: true,
            is_branch: false,
            is_major: false,
        }
    }
}
