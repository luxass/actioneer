use std::{path::PathBuf, process::ExitCode};

use crate::{
    audit::{audit_references, output::print_report},
    cli::AuditArgs,
    config::Config,
    discovery::discover_action_refs,
};

pub fn run(args: &AuditArgs, config: &Config) -> Result<ExitCode, String> {
    let references = discover_action_refs(audit_inputs(args))?;
    let report = audit_references(&references, config);

    print_report(&report, args.shared.mode)?;

    if report.ok() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

fn audit_inputs(args: &AuditArgs) -> Vec<PathBuf> {
    if args.inputs.is_empty() {
        vec![PathBuf::from(".github")]
    } else {
        args.inputs.clone()
    }
}
