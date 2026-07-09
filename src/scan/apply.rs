//! Apply planned updates to workflow files on disk.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{ActioneerConfig, PinMode};
use crate::engine::{UsesLine, join_uses_line as join, split_uses_line as split};

use super::ScanError;
use super::display::{plan_from_label, plan_to_label};
use super::types::{
    AppliedChange, ApplyFailure, ApplyReport, ApplyTarget, PlannedChange, ScanReport,
};

/// Apply selected planned updates to workflow files.
///
/// When `dry_run` is true, files are not modified but [`ApplyReport`] still lists
/// the changes that would have been written in [`ApplyReport::applied`].
///
/// Callers must inspect [`ApplyReport::failures`] even when this function returns
/// `Ok`: stale or invalid individual targets are reported there. Valid targets
/// in the same workflow are still written. File replacement uses a temporary
/// file and rename, but applying multiple workflows is not one atomic operation
/// and earlier files are not rolled back after a later failure.
///
/// # Errors
///
/// Operation-wide failures may be returned as [`ScanError`]. File and target
/// failures that can be associated with requested targets are collected in the
/// returned report instead.
///
/// # Side effects
///
/// Unless `dry_run` is set, this function rewrites workflow files below `root`.
/// It verifies that each source line still matches the scanned reference before
/// replacing it.
pub fn apply(
    root: &Path,
    report: &ScanReport,
    targets: &[ApplyTarget],
    config: &ActioneerConfig,
    dry_run: bool,
) -> Result<ApplyReport, ScanError> {
    let mut result = ApplyReport::default();

    if targets.is_empty() {
        return Ok(result);
    }

    let mut by_file: BTreeMap<PathBuf, Vec<&ApplyTarget>> = BTreeMap::new();
    for target in targets {
        by_file
            .entry(target.workflow_path.clone())
            .or_default()
            .push(target);
    }

    for (workflow_path, file_targets) in by_file {
        let path = root.join(&workflow_path);
        match apply_file(
            report,
            &path,
            &workflow_path,
            &file_targets,
            config,
            dry_run,
        ) {
            Ok((applied, failures)) => {
                result.applied.extend(applied);
                result.failures.extend(failures);
            }
            Err(e) => {
                for target in &file_targets {
                    result.failures.push(ApplyFailure {
                        workflow_path: target.workflow_path.clone(),
                        line: target.line,
                        message: e.to_string(),
                    });
                }
            }
        }
    }

    Ok(result)
}

/// All planned rows as apply targets.
pub fn all_planned_targets(report: &ScanReport) -> Vec<ApplyTarget> {
    report
        .planned_changes()
        .filter_map(|(path, reference)| {
            let line = reference.resolved.located.reference.line?;
            Some(ApplyTarget {
                workflow_path: path.clone(),
                line,
            })
        })
        .collect()
}

fn apply_file(
    report: &ScanReport,
    path: &Path,
    workflow_path: &Path,
    targets: &[&ApplyTarget],
    config: &ActioneerConfig,
    dry_run: bool,
) -> Result<(Vec<AppliedChange>, Vec<ApplyFailure>), ScanError> {
    let content = std::fs::read_to_string(path)?;
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();

    let mut applied = Vec::new();
    let mut failures = Vec::new();

    for target in targets {
        match apply_one_line(report, workflow_path, target, &mut lines, config) {
            Ok(change) => applied.push(change),
            Err(message) => failures.push(ApplyFailure {
                workflow_path: target.workflow_path.clone(),
                line: target.line,
                message,
            }),
        }
    }

    if !dry_run && !applied.is_empty() {
        write_lines(path, &lines, had_trailing_newline)?;
    }

    Ok((applied, failures))
}

fn apply_one_line(
    report: &ScanReport,
    workflow_path: &Path,
    target: &ApplyTarget,
    lines: &mut [String],
    config: &ActioneerConfig,
) -> Result<AppliedChange, String> {
    let reference = find_reference(report, workflow_path, target.line)
        .ok_or_else(|| format!("no planned update for line {}", target.line))?;

    let planned = reference
        .planned
        .as_ref()
        .ok_or_else(|| "reference has no planned change".to_string())?;

    let action_ref = &reference.resolved.located.reference;
    let line_no = action_ref
        .line
        .ok_or_else(|| "reference is missing a source line number".to_string())?;

    if line_no != target.line {
        return Err(format!(
            "line number mismatch: expected {}, found {line_no}",
            target.line
        ));
    }

    let line_index = usize::try_from(line_no)
        .ok()
        .and_then(|i| i.checked_sub(1))
        .ok_or_else(|| format!("invalid line number {line_no}"))?;

    let line = lines
        .get_mut(line_index)
        .ok_or_else(|| format!("line {line_no} no longer exists"))?;

    let uses = split(line).ok_or_else(|| format!("line {line_no} is not a uses: line"))?;

    if uses.value != action_ref.raw {
        return Err(format!(
            "line {line_no} changed since scan (expected {:?})",
            action_ref.raw
        ));
    }

    let new_value = target_value(action_ref.raw.as_str(), planned);
    let new_comment = target_comment(config.pin, planned, &uses);
    let new_line = join(&uses, &new_value, new_comment.as_deref());
    let from_label = plan_from_label(&reference.resolved, planned);
    let to_label = plan_to_label(planned, config.pin);

    *line = new_line;

    Ok(AppliedChange {
        workflow_path: workflow_path.to_path_buf(),
        line: line_no,
        action: action_ref.raw.clone(),
        from: from_label,
        to: to_label,
    })
}

fn find_reference<'a>(
    report: &'a ScanReport,
    workflow_path: &Path,
    line: u32,
) -> Option<&'a super::types::ReferenceReport> {
    report.workflows.iter().find_map(|workflow| {
        if workflow.path != workflow_path {
            return None;
        }
        workflow
            .references
            .iter()
            .find(|reference| reference.resolved.located.reference.line == Some(line))
    })
}

fn target_value(raw: &str, planned: &PlannedChange) -> String {
    let base = raw
        .rsplit_once('@')
        .map(|(prefix, _)| prefix)
        .unwrap_or(raw);
    format!("{base}@{}", planned.to_ref)
}

fn target_comment(pin: PinMode, planned: &PlannedChange, existing: &UsesLine) -> Option<String> {
    match pin {
        PinMode::Sha => planned.to_comment.clone(),
        PinMode::Tag => {
            if existing.comment.is_some() {
                Some(planned.to_ref.clone())
            } else {
                None
            }
        }
    }
}

fn write_lines(path: &Path, lines: &[String], had_trailing_newline: bool) -> Result<(), ScanError> {
    let mut content = lines.join("\n");
    if had_trailing_newline {
        content.push('\n');
    }

    let parent = path.parent().ok_or_else(|| {
        ScanError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "missing parent",
        ))
    })?;
    let tmp = parent.join(format!(".actioneer-apply-{}", std::process::id()));
    std::fs::write(&tmp, &content).map_err(ScanError::Io)?;
    std::fs::rename(&tmp, path).map_err(ScanError::Io)?;
    Ok(())
}

impl fmt::Display for ApplyReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.applied.is_empty() && self.failures.is_empty() {
            return write!(f, "no updates applied");
        }
        write!(f, "applied {} update(s)", self.applied.len())?;
        if !self.failures.is_empty() {
            write!(f, ", {} failed", self.failures.len())?;
        }
        Ok(())
    }
}
