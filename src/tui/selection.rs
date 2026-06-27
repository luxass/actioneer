use std::path::PathBuf;

use crate::config::ActioneerConfig;
use crate::scan::{plan_from_label, plan_to_label, ApplyTarget, ScanReport};

/// One planned update row in the interactive TUI list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectableUpdate {
    pub workflow_path: PathBuf,
    pub line: u32,
    pub action: String,
    pub from_label: String,
    pub to_label: String,
    pub selected: bool,
}

impl SelectableUpdate {
    pub fn workflow_name(&self) -> &str {
        self.workflow_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
    }

    pub fn apply_target(&self) -> ApplyTarget {
        ApplyTarget {
            workflow_path: self.workflow_path.clone(),
            line: self.line,
        }
    }
}

/// Build the selectable list from a scan report (rows start unselected).
pub fn from_report(report: &ScanReport, config: &ActioneerConfig) -> Vec<SelectableUpdate> {
    report
        .planned_changes()
        .map(|(path, reference)| {
            let planned = reference.planned.as_ref().unwrap();
            SelectableUpdate {
                workflow_path: path.clone(),
                line: reference.resolved.located.reference.line.unwrap_or(0),
                action: reference.resolved.located.reference.raw.clone(),
                from_label: plan_from_label(&reference.resolved, planned),
                to_label: plan_to_label(planned, config.pin),
                selected: false,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind};
    use crate::github::{RefKind, ResolvedRef};
    use crate::scan::types::{
        LocatedReference, PlannedChange, PlanReason, ReferenceReport, ResolvedReference,
        ScanReport, ScanStats, WorkflowReport,
    };

    use crate::config::ActioneerConfig;

    use super::*;

    fn sample_report() -> ScanReport {
        ScanReport {
            workflows: vec![WorkflowReport {
                path: PathBuf::from(".github/workflows/ci.yml"),
                name: Some("CI".into()),
                references: vec![ReferenceReport {
                    resolved: ResolvedReference {
                        located: LocatedReference {
                            workflow_path: PathBuf::from(".github/workflows/ci.yml"),
                            reference: ActionReference {
                                raw: "actions/checkout@v4".into(),
                                kind: ReferenceKind::Action,
                                pin_kind: PinKind::Tag,
                                owner: Some("actions".into()),
                                repo: Some("checkout".into()),
                                subpath: None,
                                git_ref: Some("v4".into()),
                                step_name: None,
                                job_id: "build".into(),
                                job_name: None,
                                step_index: Some(0),
                                line: Some(10),
                                line_comment: None,
                            },
                        },
                        current: ResolvedRef {
                            sha: "a".repeat(40),
                            ref_kind: RefKind::Tag,
                            published_at: None,
                        },
                        comment_match: CommentMatch::NoComment,
                    },
                    issues: vec![],
                    planned: Some(PlannedChange {
                        from_ref: "v4".into(),
                        to_ref: "v4.2.0".into(),
                        from_version: Some("v4".into()),
                        to_sha: "b".repeat(40),
                        to_comment: None,
                        reason: PlanReason::SemverBump {
                            level: "minor".into(),
                        },
                    }),
                }],
            }],
            stats: ScanStats {
                workflows: 1,
                references: 1,
                planned: 1,
                ..Default::default()
            },
        }
    }

    #[test]
    fn from_report_starts_unselected() {
        let items = from_report(
            &sample_report(),
            &ActioneerConfig {
                pin: crate::config::PinMode::Tag,
                ..Default::default()
            },
        );
        assert_eq!(items.len(), 1);
        assert!(!items[0].selected);
        assert_eq!(items[0].to_label, "v4.2.0");
    }
}
