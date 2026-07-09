//! Building blocks for auditing and updating GitHub Actions workflow pins.
//!
//! The typical pipeline is [`discovery`] → [`engine`] → [`github`] → [`scan`].
//! The command-line application uses the same scan report for plain text, JSON,
//! apply, and [`tui`] output.

mod ansi;
/// Cache-directory resolution.
pub mod cache;
/// Command-line argument types.
pub mod cli;
/// Plain command handlers used by the binary.
pub mod cmd;
/// Configuration loading, parsing, and validation.
pub mod config;
pub mod discovery;
pub mod engine;
pub mod github;
pub mod scan;
/// Interactive update terminal interface.
pub mod tui;

/// Version of the actioneer package that was compiled.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
