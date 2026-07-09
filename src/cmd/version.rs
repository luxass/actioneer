//! Version command execution.

use crate::VERSION;

/// Print the compiled actioneer version.
pub fn run() {
    println!("actioneer {VERSION}");
}
