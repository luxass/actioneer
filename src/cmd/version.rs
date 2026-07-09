//! Version command execution.

use crate::VERSION;

/// Print the compiled actioneer version to stdout.
pub fn run() {
    println!("actioneer {VERSION}");
}
