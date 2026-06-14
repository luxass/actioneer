use std::io::{self, Write};

pub fn run(mut out: impl Write) -> io::Result<()> {
    writeln!(out, "actioneer {}", crate::VERSION)
}
