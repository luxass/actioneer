use std::path::Path;
use std::sync::mpsc;
use std::thread;

use ratatui::widgets::TableState;

use crate::cache::cache_dir;
use crate::config::ActioneerConfig;
use crate::github::GitHubClient;
use crate::scan::{scan_workspace, ScanError, ScanReport};

use super::selection::{from_report, SelectableUpdate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanPhase {
    Scanning,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Select,
    Confirm,
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
    pub view: ViewMode,
    pub report: Option<ScanReport>,
    pub error: Option<String>,
    pub selections: Vec<SelectableUpdate>,
    pub table_state: TableState,
    pub status_banner: Option<String>,
    /// Updated each frame for scroll calculations.
    pub viewport_rows: usize,
    scan_rx: Option<mpsc::Receiver<ScanOutcome>>,
}

impl App {
    pub fn new(config: ActioneerConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let scan_config = config.clone();

        thread::spawn(move || {
            let root = Path::new(".");
            let client = GitHubClient::new(&scan_config, cache_dir());
            let outcome = match scan_workspace(root, &scan_config, &client) {
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
            view: ViewMode::Select,
            report: None,
            error: None,
            selections: Vec::new(),
            table_state: TableState::default(),
            status_banner: None,
            viewport_rows: 20,
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
                self.selections = from_report(&report, &self.config);
                if !self.selections.is_empty() {
                    self.table_state.select(Some(0));
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

    pub fn selected_count(&self) -> usize {
        self.selections.iter().filter(|s| s.selected).count()
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.selections.is_empty() {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        let last = self.selections.len() - 1;
        let new = (current as isize + delta).clamp(0, last as isize) as usize;
        self.table_state.select(Some(new));

        let offset = self.table_state.offset();
        let visible = self.viewport_rows.max(1);
        if new < offset {
            *self.table_state.offset_mut() = new;
        } else if new >= offset + visible {
            *self.table_state.offset_mut() = new.saturating_sub(visible - 1);
        }
    }

    pub fn toggle_current(&mut self) {
        if let Some(i) = self.table_state.selected()
            && let Some(item) = self.selections.get_mut(i)
        {
            item.selected = !item.selected;
        }
    }

    pub fn select_all(&mut self) {
        for item in &mut self.selections {
            item.selected = true;
        }
    }

    pub fn select_none(&mut self) {
        for item in &mut self.selections {
            item.selected = false;
        }
    }

    pub fn open_confirm(&mut self) -> bool {
        if self.selected_count() == 0 {
            self.status_banner = Some("Select at least one update (Space to toggle).".into());
            return false;
        }
        self.view = ViewMode::Confirm;
        true
    }

    pub fn cancel_confirm(&mut self) {
        self.view = ViewMode::Select;
    }

    pub fn confirm_apply(&mut self) {
        let count = self.selected_count();
        self.status_banner = Some(format!(
            "{count} update(s) selected — file patching is not implemented yet."
        ));
        self.view = ViewMode::Select;
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

    #[test]
    fn spinner_cycles() {
        let mut app = App::new(ActioneerConfig::default());
        let first = app.spinner();
        for _ in 0..10 {
            app.on_tick();
        }
        assert_eq!(app.spinner(), first);
    }

    #[test]
    fn toggle_and_select_all_none() {
        let mut app = App::new(ActioneerConfig::default());
        app.selections = vec![
            SelectableUpdate {
                workflow_path: "ci.yml".into(),
                action: "actions/checkout@v4".into(),
                from_label: "v4".into(),
                to_label: "v4.2.0".into(),
                selected: true,
            },
            SelectableUpdate {
                workflow_path: "ci.yml".into(),
                action: "actions/setup-node@v4".into(),
                from_label: "v4".into(),
                to_label: "v4.1.0".into(),
                selected: true,
            },
        ];
        app.table_state.select(Some(0));
        app.toggle_current();
        assert_eq!(app.selected_count(), 1);

        app.select_all();
        assert_eq!(app.selected_count(), 2);

        app.select_none();
        assert_eq!(app.selected_count(), 0);
        assert!(!app.open_confirm());
    }
}
