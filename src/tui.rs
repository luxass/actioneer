use std::io::{self, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    tty::IsTty,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

use crate::update::Candidate;

pub fn select(candidates: &[Candidate]) -> Result<Vec<usize>, String> {
    if !io::stdout().is_tty() || !io::stdin().is_tty() {
        return Err(
            "interactive TUI requires a terminal; use --yes, --dry-run, --mode plain, or --mode json"
                .to_string(),
        );
    }

    enable_raw_mode().map_err(|error| format!("failed to enable raw mode: {error}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|error| format!("failed to enter TUI: {error}"))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|error| format!("failed to create TUI: {error}"))?;

    let mut state = SelectionState::new(candidates.len());
    let result = run_loop(&mut terminal, candidates, &mut state);

    let stdout = terminal.backend_mut();
    let _ = execute!(stdout, LeaveAlternateScreen);
    let _ = disable_raw_mode();

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    candidates: &[Candidate],
    state: &mut SelectionState,
) -> Result<Vec<usize>, String> {
    loop {
        terminal
            .draw(|frame| draw(frame, candidates, state))
            .map_err(|error| format!("failed to draw TUI: {error}"))?;

        if let Event::Key(key) = event::read().map_err(|error| format!("failed to read input: {error}"))? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Vec::new()),
                KeyCode::Enter => return Ok(state.selected_indexes()),
                KeyCode::Up | KeyCode::Char('k') => state.move_up(),
                KeyCode::Down | KeyCode::Char('j') => state.move_down(),
                KeyCode::Char(' ') => state.toggle_current(),
                KeyCode::Char('a') => state.select_all(),
                KeyCode::Char('n') => state.select_none(),
                _ => {}
            }
        }
    }
}

struct SelectionState {
    cursor: usize,
    selected: Vec<bool>,
}

impl SelectionState {
    fn new(count: usize) -> Self {
        Self {
            cursor: 0,
            selected: vec![true; count],
        }
    }

    fn selected_indexes(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter(|(_, selected)| **selected)
            .map(|(index, _)| index)
            .collect()
    }

    fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.cursor + 1 < self.selected.len() {
            self.cursor += 1;
        }
    }

    fn toggle_current(&mut self) {
        if let Some(selected) = self.selected.get_mut(self.cursor) {
            *selected = !*selected;
        }
    }

    fn select_all(&mut self) {
        self.selected.fill(true);
    }

    fn select_none(&mut self) {
        self.selected.fill(false);
    }
}

fn draw(
    frame: &mut ratatui::Frame,
    candidates: &[Candidate],
    state: &SelectionState,
) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let items: Vec<ListItem> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| row_item(candidate, index == state.cursor, state.selected[index]))
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" actioneer update "),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    let help = Paragraph::new(
        "space=toggle a=all n=none enter=confirm q/esc=cancel",
    );
    frame.render_widget(help, chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_fails_when_not_a_tty() {
        let candidates = vec![Candidate {
            id: "update-1".to_string(),
            action: crate::discovery::ActionRef {
                file: std::path::PathBuf::from(".github/workflows/ci.yml"),
                line: 1,
                owner: "actions".to_string(),
                name: "checkout".to_string(),
                repo: "actions/checkout".to_string(),
                path: String::new(),
                ref_name: "v4".to_string(),
                version_comment: None,
            },
            target_ref: "sha".to_string(),
            version: "v4.2.2".to_string(),
            sha: "sha".to_string(),
            pin: crate::config::PinStyle::Sha,
            notes: vec!["mutable_ref"],
        }];

        let result = select(&candidates);
        assert!(result.is_err(), "TUI selection should fail when not a terminal");
        assert!(
            result.unwrap_err().contains("terminal"),
            "error should mention terminal"
        );
    }

    #[test]
    fn selection_state_defaults_to_all_selected() {
        let state = SelectionState::new(3);
        assert_eq!(state.selected_indexes(), vec![0, 1, 2]);
    }

    #[test]
    fn selection_state_toggles_current() {
        let mut state = SelectionState::new(3);
        state.toggle_current();
        assert_eq!(state.selected_indexes(), vec![1, 2]);
        state.move_down();
        state.toggle_current();
        assert_eq!(state.selected_indexes(), vec![2]);
    }
}

fn row_item(candidate: &Candidate, is_cursor: bool, selected: bool) -> ListItem<'_> {
    let checkbox = if selected { "[x]" } else { "[ ]" };
    let cursor = if is_cursor { ">" } else { " " };
    let note = candidate.notes.first().map(|note| format!(" ({note})")).unwrap_or_default();

    let line = Line::from(vec![
        Span::raw(format!("{cursor}{checkbox} ")),
        Span::raw(format!(
            "{}:{} {}@{} -> {} {}{note}",
            candidate.action.file.display(),
            candidate.action.line,
            candidate.action.repo,
            candidate.action.ref_name,
            candidate.target_ref,
            candidate.version
        )),
    ]);

    let style = if is_cursor {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    ListItem::new(line).style(style)
}
