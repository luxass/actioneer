//! Interactive terminal interface for selecting and applying planned updates.

use std::path::PathBuf;
use std::{
    io::{self, Stdout, stdout},
    panic,
};

use crossterm::event::KeyCode;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::config::ActioneerConfig;
use crate::scan::ApplyReport;

use self::app::{App, ScanPhase};

use self::view::DisplayRow;

mod app;
mod event;
mod selection;
mod theme;
mod ui;
mod view;

trait TerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate_screen(&mut self) -> io::Result<()>;
    fn leave_alternate_screen(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
}

struct CrosstermTerminalOps;

impl TerminalOps for CrosstermTerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn enter_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), EnterAlternateScreen)
    }

    fn leave_alternate_screen(&mut self) -> io::Result<()> {
        execute!(io::stdout(), LeaveAlternateScreen)
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
}

struct TerminalSession<O: TerminalOps> {
    ops: O,
    raw_mode: bool,
    alternate_screen: bool,
}

impl<O: TerminalOps> TerminalSession<O> {
    fn enter(ops: O) -> io::Result<Self> {
        let mut session = Self {
            ops,
            raw_mode: false,
            alternate_screen: false,
        };

        session.ops.enable_raw_mode()?;
        session.raw_mode = true;
        session.ops.enter_alternate_screen()?;
        session.alternate_screen = true;
        Ok(session)
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut first_error = None;

        if self.alternate_screen {
            if let Err(error) = self.ops.leave_alternate_screen() {
                first_error = Some(error);
            }
            self.alternate_screen = false;
        }

        if self.raw_mode {
            if let Err(error) = self.ops.disable_raw_mode()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
            self.raw_mode = false;
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl<O: TerminalOps> Drop for TerminalSession<O> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[derive(Debug)]
/// Errors that prevent the terminal interface from running or restoring state.
pub enum TuiError {
    /// Terminal input, drawing, setup, or cleanup failed.
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
///
/// Both fields are `None` when the user quits without applying. Apply success
/// and failure are mutually exclusive, so at most one field is populated.
#[derive(Debug, Default)]
pub struct TuiOutcome {
    /// Apply result when the user selected updates and apply produced a report.
    pub apply_report: Option<ApplyReport>,
    /// Apply error when selected updates could not produce a report.
    pub apply_error: Option<String>,
}

/// Enter the update TUI, run until the user quits or applies, then restore the terminal.
///
/// Terminal state (raw mode + alternate screen) is always restored on exit —
/// including panics. Callers should print [`TuiOutcome::apply_report`] after return.
/// Relative workflow paths are resolved from the process current directory.
///
/// # Side effects
///
/// Scanning reads workflow files and may use the GitHub cache or network.
/// Applying selected rows rewrites their workflow files before the TUI exits.
///
/// # Errors
///
/// Returns [`TuiError`] when terminal setup, event handling, drawing, or explicit
/// restoration fails. A state-aware guard still attempts best-effort cleanup on
/// every partially initialized or error return path.
pub fn run_app(
    config: ActioneerConfig,
    workflow_paths: Vec<PathBuf>,
) -> Result<TuiOutcome, TuiError> {
    install_panic_hook();

    let mut session = TerminalSession::enter(CrosstermTerminalOps)?;
    let out = stdout();
    let backend = CrosstermBackend::new(out);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let outcome = event_loop(&mut terminal, config, workflow_paths);

    session.restore()?;
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
            Some(Event::Resize) => {}
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
    let mut session = TerminalSession {
        ops: CrosstermTerminalOps,
        raw_mode: true,
        alternate_screen: true,
    };
    session.restore()
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, io, rc::Rc};

    use super::{TerminalOps, TerminalSession};

    #[derive(Clone)]
    struct FakeTerminalOps {
        calls: Rc<RefCell<Vec<&'static str>>>,
        fail_enter_alternate: bool,
        fail_leave_alternate: bool,
    }

    impl TerminalOps for FakeTerminalOps {
        fn enable_raw_mode(&mut self) -> io::Result<()> {
            self.calls.borrow_mut().push("enable-raw");
            Ok(())
        }

        fn enter_alternate_screen(&mut self) -> io::Result<()> {
            self.calls.borrow_mut().push("enter-alternate");
            if self.fail_enter_alternate {
                Err(io::Error::other("enter failed"))
            } else {
                Ok(())
            }
        }

        fn leave_alternate_screen(&mut self) -> io::Result<()> {
            self.calls.borrow_mut().push("leave-alternate");
            if self.fail_leave_alternate {
                Err(io::Error::other("leave failed"))
            } else {
                Ok(())
            }
        }

        fn disable_raw_mode(&mut self) -> io::Result<()> {
            self.calls.borrow_mut().push("disable-raw");
            Ok(())
        }
    }

    #[test]
    fn alternate_screen_setup_failure_restores_raw_mode() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let ops = FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_enter_alternate: true,
            fail_leave_alternate: false,
        };

        let result = TerminalSession::enter(ops);

        assert!(result.is_err());
        assert_eq!(
            *calls.borrow(),
            ["enable-raw", "enter-alternate", "disable-raw"]
        );
    }

    #[test]
    fn explicit_restore_is_not_repeated_on_drop() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let ops = FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_enter_alternate: false,
            fail_leave_alternate: false,
        };
        let mut session = TerminalSession::enter(ops).unwrap();

        session.restore().unwrap();
        drop(session);

        assert_eq!(
            *calls.borrow(),
            [
                "enable-raw",
                "enter-alternate",
                "leave-alternate",
                "disable-raw"
            ]
        );
    }

    #[test]
    fn restore_attempts_raw_cleanup_after_alternate_screen_error() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let ops = FakeTerminalOps {
            calls: Rc::clone(&calls),
            fail_enter_alternate: false,
            fail_leave_alternate: true,
        };
        let mut session = TerminalSession::enter(ops).unwrap();

        let error = session.restore().unwrap_err();

        assert_eq!(error.to_string(), "leave failed");
        assert_eq!(
            *calls.borrow(),
            [
                "enable-raw",
                "enter-alternate",
                "leave-alternate",
                "disable-raw"
            ]
        );
    }
}
