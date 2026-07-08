//! Flat display rows built from collapsible workflow groups.

use super::selection::WorkflowGroup;

/// One rendered line in the planned-changes list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayRow {
    Spacer,
    GroupHeader(usize),
    Action { group: usize, item: usize },
}

/// Scrollable list of display rows derived from workflow groups.
#[derive(Debug, Clone, Default)]
pub struct ListView {
    pub rows: Vec<DisplayRow>,
}

impl ListView {
    pub fn rebuild(groups: &[WorkflowGroup]) -> Self {
        let mut rows = Vec::new();
        for (group_idx, group) in groups.iter().enumerate() {
            if group_idx > 0 {
                rows.push(DisplayRow::Spacer);
                rows.push(DisplayRow::Spacer);
            }
            rows.push(DisplayRow::GroupHeader(group_idx));
            if !group.collapsed && !group.items.is_empty() {
                rows.push(DisplayRow::Spacer);
                for item_idx in 0..group.items.len() {
                    rows.push(DisplayRow::Action {
                        group: group_idx,
                        item: item_idx,
                    });
                }
            }
        }
        Self { rows }
    }

    pub fn focusable_row_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_focusable())
            .map(|(idx, _)| idx)
            .collect()
    }

    pub fn row(&self, index: usize) -> Option<DisplayRow> {
        self.rows.get(index).copied()
    }
}

impl DisplayRow {
    pub fn is_focusable(self) -> bool {
        !matches!(self, Self::Spacer)
    }
}

/// Map a focus index (among focusable rows) to a display row index.
pub fn focus_to_row(focusable: &[usize], focus_index: usize) -> Option<usize> {
    focusable.get(focus_index).copied()
}

/// Move within focusable rows, wrapping at bounds.
pub fn move_focus(focusable: &[usize], current: usize, delta: isize) -> usize {
    if focusable.is_empty() {
        return 0;
    }
    let last = focusable.len() - 1;
    (current as isize + delta).clamp(0, last as isize) as usize
}

/// Keep the focused display row visible in a viewport of `visible_rows` lines.
pub fn scroll_for_focus(
    focusable: &[usize],
    focus_index: usize,
    current_scroll: usize,
    visible_rows: usize,
) -> usize {
    if visible_rows == 0 {
        return 0;
    }
    let Some(&row) = focusable.get(focus_index) else {
        return current_scroll;
    };
    if row < current_scroll {
        return row;
    }
    if row >= current_scroll + visible_rows {
        return row.saturating_sub(visible_rows - 1);
    }
    current_scroll
}

pub fn group_header_label(group: &WorkflowGroup) -> String {
    let chevron = if group.collapsed { "▸" } else { "▾" };
    let name = group.workflow_name();
    let count = group.items.len();
    if group.collapsed {
        format!("{chevron} {name}  ({count})")
    } else {
        format!("{chevron} {name}")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::tui::selection::SelectableItem;

    use super::*;

    fn sample_groups() -> Vec<WorkflowGroup> {
        vec![
            WorkflowGroup {
                workflow_path: PathBuf::from("main.yml"),
                collapsed: false,
                items: vec![
                    SelectableItem {
                        line: 1,
                        action: "actions/checkout@v4".into(),
                        from_label: "v4".into(),
                        to_label: "v4.2.0".into(),
                        selected: false,
                    },
                    SelectableItem {
                        line: 2,
                        action: "actions/setup-go@v5".into(),
                        from_label: "v5".into(),
                        to_label: "v5.2.0".into(),
                        selected: false,
                    },
                ],
            },
            WorkflowGroup {
                workflow_path: PathBuf::from("pr.yml"),
                collapsed: true,
                items: vec![SelectableItem {
                    line: 3,
                    action: "azure/setup-helm@v4".into(),
                    from_label: "v4".into(),
                    to_label: "v4.2.0".into(),
                    selected: false,
                }],
            },
        ]
    }

    #[test]
    fn rebuild_inserts_spacer_and_hides_collapsed_actions() {
        let view = ListView::rebuild(&sample_groups());
        assert_eq!(
            view.rows,
            vec![
                DisplayRow::GroupHeader(0),
                DisplayRow::Spacer,
                DisplayRow::Action { group: 0, item: 0 },
                DisplayRow::Action { group: 0, item: 1 },
                DisplayRow::Spacer,
                DisplayRow::Spacer,
                DisplayRow::GroupHeader(1),
            ]
        );
    }

    #[test]
    fn focusable_skips_spacers() {
        let view = ListView::rebuild(&sample_groups());
        let focusable = view.focusable_row_indices();
        assert!(!focusable.is_empty());
        assert!(view.rows[focusable[0]].is_focusable());
        for &idx in &focusable {
            assert_ne!(view.rows[idx], DisplayRow::Spacer);
        }
    }

    #[test]
    fn move_focus_clamps_at_bounds() {
        let focusable = vec![0, 2, 4];
        assert_eq!(move_focus(&focusable, 0, -1), 0);
        assert_eq!(move_focus(&focusable, 2, 1), 2);
    }
}
