pub mod audit;
pub mod cli;
pub mod cmd;
pub mod config;
pub mod discovery;
pub mod github;
pub mod patch;
pub mod update;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
