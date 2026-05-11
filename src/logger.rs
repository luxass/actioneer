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
        self.write(message.as_ref());
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.write(message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.write(message.as_ref());
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.write(message.as_ref());
    }

    fn write(&self, message: &str) {
        if self.mode == Mode::Plain {
            println!("{}", strip_ansi(message));
        } else {
            println!("{message}");
        }
    }
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
