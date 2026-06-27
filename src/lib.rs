pub mod ansi;
pub mod cache;
pub mod cli;
pub mod cmd;
pub mod config;
pub mod discovery;
pub mod engine;
pub mod github;
pub mod scan;
pub mod tui;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
