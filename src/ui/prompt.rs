use std::io::{self, IsTerminal};

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::backend::CrosstermBackend;

use crate::model::ResolvedUpdate;

use crate::ui::prompt_input::{EventSource, Key, RealEventSource, read_key};
use crate::ui::prompt_render::render;
use crate::ui::prompt_ui_state::{PromptState, build_visible_rows};

#[cfg(test)]
use crate::ui::prompt_input::TestEventSource;

#[derive(Debug)]
pub enum Error {
    NotATerminal,
    Canceled,
    Interrupted,
    Io(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotATerminal => f.write_str("interactive selection requires a terminal"),
            Self::Canceled => f.write_str("selection canceled"),
            Self::Interrupted => f.write_str("selection interrupted"),
            Self::Io(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn select_updates(updates: &[ResolvedUpdate]) -> Result<Vec<usize>, Error> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::NotATerminal);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_prompt(&mut terminal, &mut RealEventSource, updates);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_prompt<B: Backend, E: EventSource>(
    terminal: &mut Terminal<B>,
    event_source: &mut E,
    updates: &[ResolvedUpdate],
) -> Result<Vec<usize>, Error> {
    if updates.is_empty() {
        return Ok(Vec::new());
    }

    let mut state = PromptState::new(updates.len());

    loop {
        let visible_rows = build_visible_rows(updates, &state.collapsed);
        state.clamp_cursor(&visible_rows);

        terminal.draw(|frame| render(frame, updates, &state))?;

        match read_key(event_source)? {
            Key::Up => state.move_up(&visible_rows),
            Key::Down => state.move_down(&visible_rows),
            Key::Toggle => state.toggle_at(&visible_rows, updates),
            Key::ToggleAll => state.toggle_all(),
            Key::ToggleFile => state.toggle_file_at_cursor(&visible_rows, updates),
            Key::ToggleCollapse => {
                state.toggle_collapse(&visible_rows, updates);
            }
            Key::PageUp => {
                let page_size = page_size(terminal)?;
                state.page_up(page_size);
            }
            Key::PageDown => {
                let page_size = page_size(terminal)?;
                state.page_down(page_size, &visible_rows);
            }
            Key::Home => state.home(),
            Key::End => state.end(&visible_rows),
            Key::Invert => state.invert(),
            Key::SelectNone => state.select_none(),
            Key::ScrollLeft => state.scroll_left(),
            Key::ScrollRight => state.scroll_right(),
            Key::Accept => {
                return Ok(state.selected_indices().collect());
            }
            Key::Cancel => return Err(Error::Canceled),
            Key::Resize => {}
            Key::Ignore => {}
        }
    }
}

fn page_size<B: Backend>(terminal: &Terminal<B>) -> Result<usize, Error> {
    Ok(usize::from(terminal.size()?.height.saturating_sub(4)).max(1))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::backend::TestBackend;

    use crate::model::{ResolvedUpdate, UpdateSource, UpdateTarget, ValidationState};

    use super::*;

    fn make_update(file: &str, action: &str) -> ResolvedUpdate {
        ResolvedUpdate::new(
            action,
            "build",
            "v1.0.0",
            ValidationState::new("abc1234", "1.0.0", false),
            UpdateTarget::new("v2.0.0", "v2.0.0", false),
            UpdateSource::new(file, 10, 20, 30),
            false,
        )
    }

    fn make_event(code: KeyCode, modifiers: KeyModifiers) -> crossterm::event::Event {
        crossterm::event::Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn char_event(c: char) -> crossterm::event::Event {
        make_event(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn special_event(code: KeyCode) -> crossterm::event::Event {
        make_event(code, KeyModifiers::NONE)
    }

    #[test]
    fn run_prompt_accept_returns_selected_indices() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![
            special_event(KeyCode::Down),
            char_event(' '),
            special_event(KeyCode::Down),
            char_event(' '),
            special_event(KeyCode::Enter),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn run_prompt_cancel_returns_error() {
        let updates = vec![make_update("a.yml", "actions/checkout")];
        let events = vec![char_event('q')];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates);
        assert!(matches!(result, Err(Error::Canceled)));
    }

    #[test]
    fn run_prompt_toggle_all_selects_all() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![char_event('a'), special_event(KeyCode::Enter)];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn run_prompt_invert_flips_selection() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![
            special_event(KeyCode::Down),
            char_event(' '),
            char_event('i'),
            special_event(KeyCode::Enter),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn run_prompt_select_none_clears_selection() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![
            char_event('a'),
            char_event('n'),
            special_event(KeyCode::Enter),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn run_prompt_collapse_and_toggle_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![
            special_event(KeyCode::Down),
            special_event(KeyCode::Down),
            special_event(KeyCode::Down),
            special_event(KeyCode::Down),
            char_event(' '),
            special_event(KeyCode::Enter),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![2]);
    }

    #[test]
    fn run_prompt_toggle_file_toggles_all_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![char_event('f'), special_event(KeyCode::Enter)];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn run_prompt_space_on_file_header_toggles_all_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![char_event(' '), special_event(KeyCode::Enter)];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn run_prompt_end_moves_to_last_visible_row() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/cache"),
        ];
        let events = vec![
            special_event(KeyCode::End),
            char_event(' '),
            special_event(KeyCode::Enter),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn run_prompt_empty_updates_returns_empty() {
        let updates: Vec<ResolvedUpdate> = vec![];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(vec![]);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert!(result.is_empty());
    }
}
