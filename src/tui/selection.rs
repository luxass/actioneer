use std::path::{Path, PathBuf};

use crate::config::ActioneerConfig;
use crate::scan::{ApplyTarget, ScanReport, plan_from_label, plan_to_label};

/// One planned update within a workflow group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectableItem {
    pub line: u32,
    pub action: String,
    pub from_label: String,
    pub to_label: String,
    pub selected: bool,
}

/// Planned updates for one workflow file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGroup {
    pub workflow_path: PathBuf,
    pub collapsed: bool,
    pub items: Vec<SelectableItem>,
}

impl WorkflowGroup {
    pub fn workflow_name(&self) -> &str {
        self.workflow_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
    }
}

impl SelectableItem {
    pub fn apply_target(&self, workflow_path: &Path) -> ApplyTarget {
        ApplyTarget {
            workflow_path: workflow_path.to_path_buf(),
            line: self.line,
        }
    }
}

/// Build collapsible workflow groups from a scan report (items start unselected).
pub fn from_report(report: &ScanReport, config: &ActioneerConfig) -> Vec<WorkflowGroup> {
    let mut groups: Vec<WorkflowGroup> = Vec::new();

    for (path, reference) in report.planned_changes() {
        let planned = reference.planned.as_ref().unwrap();
        let item = SelectableItem {
            line: reference.resolved.located.reference.line.unwrap_or(0),
            action: reference.resolved.located.reference.raw.clone(),
            from_label: plan_from_label(&reference.resolved, planned),
            to_label: plan_to_label(planned, config.pin),
            selected: false,
        };

        if let Some(group) = groups.last_mut()
            && group.workflow_path == *path
        {
            group.items.push(item);
        } else {
            groups.push(WorkflowGroup {
                workflow_path: path.clone(),
                collapsed: false,
                items: vec![item],
            });
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::engine::{ActionReference, CommentMatch, PinKind, ReferenceKind};
    use crate::github::{RefKind, ResolvedRef};
    use crate::scan::{
        LocatedReference, PlanReason, PlannedChange, ReferenceReport, ResolvedReference,
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
        let groups = from_report(
            &sample_report(),
            &ActioneerConfig {
                pin: crate::config::PinMode::Tag,
                ..Default::default()
            },
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 1);
        assert!(!groups[0].items[0].selected);
        assert_eq!(groups[0].items[0].to_label, "v4.2.0");
    }

    #[test]
    fn from_report_merges_items_in_same_workflow() {
        let mut report = sample_report();
        report.workflows[0].references.push(ReferenceReport {
            resolved: ResolvedReference {
                located: LocatedReference {
                    workflow_path: PathBuf::from(".github/workflows/ci.yml"),
                    reference: ActionReference {
                        raw: "actions/setup-node@v3".into(),
                        kind: ReferenceKind::Action,
                        pin_kind: PinKind::Tag,
                        owner: Some("actions".into()),
                        repo: Some("setup-node".into()),
                        subpath: None,
                        git_ref: Some("v3".into()),
                        step_name: None,
                        job_id: "lint".into(),
                        job_name: None,
                        step_index: Some(1),
                        line: Some(12),
                        line_comment: None,
                    },
                },
                current: ResolvedRef {
                    sha: "c".repeat(40),
                    ref_kind: RefKind::Tag,
                    published_at: None,
                },
                comment_match: CommentMatch::NoComment,
            },
            issues: vec![],
            planned: Some(PlannedChange {
                from_ref: "v3".into(),
                to_ref: "v4.2.0".into(),
                from_version: Some("v3".into()),
                to_sha: "d".repeat(40),
                to_comment: None,
                reason: PlanReason::SemverBump {
                    level: "major".into(),
                },
            }),
        });
        report.stats.planned = 2;

        let groups = from_report(
            &report,
            &ActioneerConfig {
                pin: crate::config::PinMode::Tag,
                ..Default::default()
            },
        );

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2);
    }

    #[test]
    fn from_report_splits_different_workflows() {
        let mut report = sample_report();
        report.workflows.push(WorkflowReport {
            path: PathBuf::from(".github/workflows/lint.yml"),
            name: Some("Lint".into()),
            references: vec![ReferenceReport {
                resolved: ResolvedReference {
                    located: LocatedReference {
                        workflow_path: PathBuf::from(".github/workflows/lint.yml"),
                        reference: ActionReference {
                            raw: "actions/setup-node@v3".into(),
                            kind: ReferenceKind::Action,
                            pin_kind: PinKind::Tag,
                            owner: Some("actions".into()),
                            repo: Some("setup-node".into()),
                            subpath: None,
                            git_ref: Some("v3".into()),
                            step_name: None,
                            job_id: "lint".into(),
                            job_name: None,
                            step_index: Some(0),
                            line: Some(5),
                            line_comment: None,
                        },
                    },
                    current: ResolvedRef {
                        sha: "c".repeat(40),
                        ref_kind: RefKind::Tag,
                        published_at: None,
                    },
                    comment_match: CommentMatch::NoComment,
                },
                issues: vec![],
                planned: Some(PlannedChange {
                    from_ref: "v3".into(),
                    to_ref: "v4.2.0".into(),
                    from_version: Some("v3".into()),
                    to_sha: "d".repeat(40),
                    to_comment: None,
                    reason: PlanReason::SemverBump {
                        level: "major".into(),
                    },
                }),
            }],
        });
        report.stats.planned = 2;

        let groups = from_report(
            &report,
            &ActioneerConfig {
                pin: crate::config::PinMode::Tag,
                ..Default::default()
            },
        );

        assert_eq!(groups.len(), 2);
    }
}
