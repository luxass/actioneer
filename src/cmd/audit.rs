use std::process::ExitCode;

use thiserror::Error;

use crate::cli::{AuditArgs, GlobalArgs, ValidateArgs};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Validate(#[from] crate::cmd::validate::Error),
}

pub fn run(global: GlobalArgs, args: AuditArgs) -> Result<ExitCode, Error> {
    Ok(crate::cmd::validate::run(
        global,
        ValidateArgs {
            recursive: args.recursive,
            include_branches: args.include_branches,
            inputs: args.inputs,
        },
    )?)
}
