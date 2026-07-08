use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use crate::cache::cache_dir;
use crate::config::ActioneerConfig;
use crate::github::GitHubClient;
use crate::scan::{apply, scan_workspace, ApplyReport, ScanError, ScanReport};

use super::selection::{from_report, WorkflowGroup};
use super::view::{self, ListView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    Scanning,
    Ready,
    Failed,
}

enum ScanOutcome {
    Ok(ScanReport),
    Err(String),
}

pub struct App {
    pub config: ActioneerConfig,
    pub should_quit: bool,
    pub tick: u64,
    pub phase: ScanPhase,
    pub report: Option<ScanReport>,
    pub error: Option<String>,
    pub groups: Vec<WorkflowGroup>,
    pub list_view: ListView,
    /// Index into [`ListView::focusable_row_indices`].
    pub focus_index: usize,
    pub scroll_offset: usize,
    pub status_banner: Option<String>,
    /// Updated each frame for scroll calculations.
    pub viewport_rows: usize,
    pub apply_report: Option<ApplyReport>,
    pub apply_error: Option<String>,
    scan_rx: Option<mpsc::Receiver<ScanOutcome>>,
}

impl App {
    pub fn new(config: ActioneerConfig, workflow_paths: Vec<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let scan_config = config.clone();

        thread::spawn(move || {
            let root = Path::new(".");
            let client = GitHubClient::new(&scan_config, cache_dir());
            let outcome = match scan_workspace(root, &workflow_paths, &scan_config, &client) {
                Ok(report) => ScanOutcome::Ok(report),
                Err(e) => ScanOutcome::Err(format_scan_error(e)),
            };
            let _ = tx.send(outcome);
        });

        Self {
            config,
            should_quit: false,
            tick: 0,
            phase: ScanPhase::Scanning,
            report: None,
            error: None,
            groups: Vec::new(),
            list_view: ListView::default(),
            focus_index: 0,
            scroll_offset: 0,
            status_banner: None,
            viewport_rows: 20,
            apply_report: None,
            apply_error: None,
            scan_rx: Some(rx),
        }
    }

    pub fn poll_scan(&mut self) {
        if self.phase != ScanPhase::Scanning {
            return;
        }
        let Some(rx) = &self.scan_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(ScanOutcome::Ok(report)) => {
                self.groups = from_report(&report, &self.config);
                self.rebuild_list();
                if !self.focusable_rows().is_empty() {
                    self.focus_index = 0;
                }
                self.report = Some(report);
                self.phase = ScanPhase::Ready;
                self.scan_rx = None;
            }
            Ok(ScanOutcome::Err(message)) => {
                self.error = Some(message);
                self.phase = ScanPhase::Failed;
                self.scan_rx = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.error = Some("scan worker exited unexpectedly".into());
                self.phase = ScanPhase::Failed;
                self.scan_rx = None;
            }
        }
    }

    pub fn rebuild_list(&mut self) {
        self.list_view = ListView::rebuild(&self.groups);
        self.clamp_focus();
    }

    pub fn focusable_rows(&self) -> Vec<usize> {
        self.list_view.focusable_row_indices()
    }

    pub fn focused_display_row(&self) -> Option<usize> {
        view::focus_to_row(&self.focusable_rows(), self.focus_index)
    }

    pub fn total_planned_items(&self) -> usize {
        self.groups.iter().map(|g| g.items.len()).sum()
    }

    pub fn selected_count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| g.items.iter())
            .filter(|item| item.selected)
            .count()
    }

    pub fn move_selection(&mut self, delta: isize) {
        let focusable = self.focusable_rows();
        if focusable.is_empty() {
            return;
        }
        self.focus_index = view::move_focus(&focusable, self.focus_index, delta);
        self.scroll_offset = view::scroll_for_focus(
            &focusable,
            self.focus_index,
            self.scroll_offset,
            self.viewport_rows,
        );
    }

    pub fn toggle_current(&mut self) {
        let Some(row) = self
            .focused_display_row()
            .and_then(|idx| self.list_view.row(idx))
        else {
            return;
        };
        match row {
            view::DisplayRow::GroupHeader(group_idx) => {
                if let Some(group) = self.groups.get_mut(group_idx) {
                    group.collapsed = !group.collapsed;
                }
                self.rebuild_list();
            }
            view::DisplayRow::Action { group, item } => {
                if let Some(entry) = self.groups.get_mut(group).and_then(|g| g.items.get_mut(item))
                {
                    entry.selected = !entry.selected;
                }
            }
            view::DisplayRow::Spacer => {}
        }
    }

    pub fn select_all(&mut self) {
        for group in &mut self.groups {
            for item in &mut group.items {
                item.selected = true;
            }
        }
    }

    pub fn select_none(&mut self) {
        for group in &mut self.groups {
            for item in &mut group.items {
                item.selected = false;
            }
        }
    }

    pub fn apply_selected(&mut self) {
        if self.selected_count() == 0 {
            self.status_banner = Some("Select at least one update (Space to toggle).".into());
            return;
        }

        let Some(report) = self.report.as_ref() else {
            return;
        };

        let targets: Vec<_> = self
            .groups
            .iter()
            .flat_map(|group| {
                group
                    .items
                    .iter()
                    .filter(|item| item.selected)
                    .map(|item| item.apply_target(&group.workflow_path))
            })
            .collect();

        let root = Path::new(".");
        match apply(root, report, &targets, &self.config, false) {
            Ok(result) => {
                self.apply_report = Some(result);
                self.quit();
            }
            Err(error) => {
                self.apply_error = Some(error.to_string());
                self.quit();
            }
        }
    }

    fn clamp_focus(&mut self) {
        let focusable = self.focusable_rows();
        if focusable.is_empty() {
            self.focus_index = 0;
            self.scroll_offset = 0;
            return;
        }
        if self.focus_index >= focusable.len() {
            self.focus_index = focusable.len() - 1;
        }
        self.scroll_offset = view::scroll_for_focus(
            &focusable,
            self.focus_index,
            self.scroll_offset,
            self.viewport_rows,
        );
    }

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    pub fn spinner(&self) -> char {
        const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        FRAMES[(self.tick as usize) % FRAMES.len()]
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

fn format_scan_error(error: ScanError) -> String {
    match error {
        ScanError::Discovery(e) => e.to_string(),
        ScanError::Io(e) => e.to_string(),
        ScanError::Parse { path, error } => format!("{}: {error}", path.display()),
        ScanError::GitHub(e) => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::selection::SelectableItem;

    #[test]
    fn spinner_cycles() {
        let mut app = App::new(ActioneerConfig::default(), Vec::new());
        let first = app.spinner();
        for _ in 0..10 {
            app.on_tick();
        }
        assert_eq!(app.spinner(), first);
    }

    #[test]
    fn toggle_and_select_all_none() {
        let mut app = App::new(ActioneerConfig::default(), Vec::new());
        app.groups = vec![WorkflowGroup {
            workflow_path: "ci.yml".into(),
            collapsed: false,
            items: vec![
                SelectableItem {
                    line: 10,
                    action: "actions/checkout@v4".into(),
                    from_label: "v4".into(),
                    to_label: "v4.2.0".into(),
                    selected: true,
                },
                SelectableItem {
                    line: 11,
                    action: "actions/setup-node@v4".into(),
                    from_label: "v4".into(),
                    to_label: "v4.1.0".into(),
                    selected: true,
                },
            ],
        }];
        app.rebuild_list();
        // Focus first action row (index 1: header, index 2: first action)
        app.focus_index = 1;
        app.toggle_current();
        assert_eq!(app.selected_count(), 1);

        app.select_all();
        assert_eq!(app.selected_count(), 2);

        app.select_none();
        assert_eq!(app.selected_count(), 0);
        app.apply_selected();
        assert_eq!(
            app.status_banner.as_deref(),
            Some("Select at least one update (Space to toggle).")
        );
    }

    #[test]
    fn toggle_header_collapses_group() {
        let mut app = App::new(ActioneerConfig::default(), Vec::new());
        app.groups = vec![WorkflowGroup {
            workflow_path: "ci.yml".into(),
            collapsed: false,
            items: vec![SelectableItem {
                line: 10,
                action: "actions/checkout@v4".into(),
                from_label: "v4".into(),
                to_label: "v4.2.0".into(),
                selected: false,
            }],
        }];
        app.rebuild_list();
        assert!(app.list_view.rows.iter().any(|r| matches!(
            r,
            view::DisplayRow::Action { .. }
        )));

        app.focus_index = 0;
        app.toggle_current();
        assert!(app.groups[0].collapsed);
        assert!(!app
            .list_view
            .rows
            .iter()
            .any(|r| matches!(r, view::DisplayRow::Action { .. })));
    }
}
