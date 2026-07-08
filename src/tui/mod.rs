use std::path::PathBuf;
use std::{
    io::{self, Stdout, stdout},
    panic,
};

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::config::ActioneerConfig;
use crate::scan::ApplyReport;

use self::app::{App, ScanPhase};

use self::view::DisplayRow;

pub mod app;
pub mod event;
pub mod selection;
pub mod theme;
pub mod ui;
pub mod view;

#[derive(Debug)]
pub enum TuiError {
    Io(io::Error),
}

impl std::fmt::Display for TuiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "terminal I/O error: {e}"),
        }
    }
}

impl std::error::Error for TuiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
        }
    }
}

impl From<io::Error> for TuiError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Outcome after the TUI closes.
#[derive(Debug, Default)]
pub struct TuiOutcome {
    pub apply_report: Option<ApplyReport>,
    pub apply_error: Option<String>,
}

/// Enter the update TUI, run until the user quits or applies, then restore the terminal.
///
/// Terminal state (raw mode + alternate screen) is always restored on exit —
/// including panics. Callers should print [`TuiOutcome::apply_report`] after return.
pub fn run_app(config: ActioneerConfig, workflow_paths: Vec<PathBuf>) -> Result<TuiOutcome, TuiError> {
    install_panic_hook();

    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(out);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let outcome = event_loop(&mut terminal, config, workflow_paths);

    restore_terminal()?;
    outcome
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    config: ActioneerConfig,
    workflow_paths: Vec<PathBuf>,
) -> Result<TuiOutcome, TuiError> {
    use crossterm::event::KeyModifiers;

    use self::event::{Event, EventHandler};

    let mut app = App::new(config, workflow_paths);
    let events = EventHandler::new(100);

    loop {
        app.poll_scan();
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        match events.next() {
            None => break,
            Some(Event::Tick) => app.on_tick(),
            Some(Event::Resize(_, _)) => {}
            Some(Event::Key(key)) => {
                if matches!(
                    (key.code, key.modifiers),
                    (KeyCode::Char('c'), KeyModifiers::CONTROL)
                ) {
                    app.quit();
                } else if app.phase == ScanPhase::Ready {
                    handle_ready_key(&mut app, key.code);
                } else if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    app.quit();
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(TuiOutcome {
        apply_report: app.apply_report,
        apply_error: app.apply_error,
    })
}

fn handle_ready_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Char(' ') => app.toggle_current(),
        KeyCode::Char('a') => app.select_all(),
        KeyCode::Char('n') => app.select_none(),
        KeyCode::Enter => {
            if app
                .focused_display_row()
                .and_then(|idx| app.list_view.row(idx))
                .is_some_and(|row| matches!(row, DisplayRow::GroupHeader(_)))
            {
                app.toggle_current();
            } else {
                app.apply_selected();
            }
        }
        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
        _ => {}
    }
}

/// Install a panic hook that restores the terminal before printing the panic
/// message. Without this, a panic leaves the terminal in raw/alternate-screen
/// mode, which is disorienting for the user.
fn install_panic_hook() {
    let original = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        original(info);
    }));
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
