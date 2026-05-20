use std::io::{self, IsTerminal, Stdout};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Terminal;

use crate::model::ResolvedUpdate;

const PROMPT_TITLE: &str = "Choose action updates";
const CONTROLS_SUMMARY: &str = "Move selection with arrows or j/k";
const FOOTER: &str =
    "Up/Down/j/k move  space row  f file  enter apply  a all  i invert  n none  q cancel";
const VISIBLE_ROWS_HINT: usize = 12;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Key {
    Up,
    Down,
    Toggle,
    ToggleAll,
    ToggleFile,
    Invert,
    SelectNone,
    Accept,
    Cancel,
    Ignore,
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

    let result = run_prompt(&mut terminal, updates);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_prompt(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    updates: &[ResolvedUpdate],
) -> Result<Vec<usize>, Error> {
    let mut selected = vec![false; updates.len()];
    let mut cursor = 0usize;
    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        terminal.draw(|frame| render(frame, updates, &selected, cursor, &mut state))?;

        match read_key()? {
            Key::Up => {
                cursor = if cursor == 0 {
                    updates.len() - 1
                } else {
                    cursor - 1
                };
                state.select(Some(cursor));
            }
            Key::Down => {
                cursor = if cursor + 1 == updates.len() {
                    0
                } else {
                    cursor + 1
                };
                state.select(Some(cursor));
            }
            Key::Toggle => selected[cursor] = !selected[cursor],
            Key::ToggleAll => toggle_all(&mut selected),
            Key::ToggleFile => toggle_file(updates, &mut selected, updates[cursor].file()),
            Key::Invert => invert_selected(&mut selected),
            Key::SelectNone => selected.fill(false),
            Key::Accept => {
                return Ok(selected
                    .into_iter()
                    .enumerate()
                    .filter_map(|(index, is_selected)| is_selected.then_some(index))
                    .collect());
            }
            Key::Cancel => return Err(Error::Canceled),
            Key::Ignore => {}
        }
    }
}

fn render(
    frame: &mut ratatui::Frame<'_>,
    updates: &[ResolvedUpdate],
    selected: &[bool],
    cursor: usize,
    state: &mut ListState,
) {
    let area = frame.area();
    let sections = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(8),
        Constraint::Length(4),
    ])
    .split(area);

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::Green)),
            Span::raw(format!(
                "{} available updates across {} workflow files",
                updates.len(),
                workflow_count(updates)
            )),
        ]),
        Line::from(vec![
            Span::styled("? ", Style::default().fg(Color::Cyan)),
            Span::raw(CONTROLS_SUMMARY),
        ]),
    ])
    .block(Block::default().title(PROMPT_TITLE).borders(Borders::ALL))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, sections[0]);

    let items: Vec<ListItem<'_>> = updates
        .iter()
        .enumerate()
        .map(|(index, update)| {
            let file_changed = index == 0 || updates[index - 1].file() != update.file();
            let marker = if selected[index] { "●" } else { "○" };
            let marker_style = if selected[index] {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let target_style = if update.has_sha_mismatch() {
                Style::default().fg(Color::Yellow)
            } else if update.is_major_update() {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            let mut lines = Vec::new();

            if file_changed {
                lines.push(Line::from(vec![
                    Span::styled("▸ ", Style::default().fg(Color::Cyan)),
                    Span::styled(
                        update.file(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }

            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(
                    &update.action,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled("from ", Style::default().fg(Color::DarkGray)),
                Span::styled(&update.current, Style::default().fg(Color::Yellow)),
                if update.has_version_comment() {
                    Span::styled(" (", Style::default().fg(Color::DarkGray))
                } else {
                    Span::raw("")
                },
                if update.has_version_comment() {
                    Span::styled(update.version_comment(), Style::default().fg(Color::Blue))
                } else {
                    Span::raw("")
                },
                if update.has_version_comment() {
                    Span::styled(")", Style::default().fg(Color::DarkGray))
                } else {
                    Span::raw("")
                },
                Span::styled("  ->  ", Style::default().fg(Color::DarkGray)),
                Span::styled(update.display_target(), target_style),
            ]));

            let mut meta = vec![
                Span::raw("    "),
                Span::styled("job ", Style::default().fg(Color::DarkGray)),
                Span::raw(&update.job),
            ];
            if update.has_sha_mismatch() {
                meta.push(Span::styled(
                    "  expected ",
                    Style::default().fg(Color::DarkGray),
                ));
                meta.push(Span::styled(
                    short_sha_or_full(update),
                    Style::default().fg(Color::Yellow),
                ));
            }
            lines.push(Line::from(meta));

            ListItem::new(lines)
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(cursor));
    *state = list_state;

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Updates"))
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 52, 64))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ")
        .repeat_highlight_symbol(false)
        .scroll_padding(visible_scroll_padding(updates.len()));
    frame.render_stateful_widget(list, sections[1], state);

    let footer = Paragraph::new(FOOTER)
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, sections[2]);
}

fn visible_scroll_padding(count: usize) -> usize {
    if count <= VISIBLE_ROWS_HINT {
        0
    } else {
        2
    }
}

fn read_key() -> Result<Key, Error> {
    loop {
        let event = event::read()?;
        let Event::Key(key_event) = event else {
            continue;
        };
        if key_event.kind != KeyEventKind::Press {
            continue;
        }

        return Ok(match key_event.code {
            KeyCode::Up | KeyCode::Char('k') => Key::Up,
            KeyCode::Down | KeyCode::Char('j') => Key::Down,
            KeyCode::Enter => Key::Accept,
            KeyCode::Char(' ') | KeyCode::Char('x') => Key::Toggle,
            KeyCode::Char('a') => Key::ToggleAll,
            KeyCode::Char('f') => Key::ToggleFile,
            KeyCode::Char('i') => Key::Invert,
            KeyCode::Char('n') => Key::SelectNone,
            KeyCode::Char('q') | KeyCode::Esc => Key::Cancel,
            KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                return Err(Error::Interrupted);
            }
            _ => Key::Ignore,
        });
    }
}

fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn short_sha_or_full(update: &ResolvedUpdate) -> String {
    if !update.has_current_ref() {
        update.current.clone()
    } else {
        short_sha(update.current_ref()).to_string()
    }
}

fn workflow_count(updates: &[ResolvedUpdate]) -> usize {
    let mut count = 0usize;
    let mut last_file: Option<&str> = None;
    for update in updates {
        if last_file != Some(update.file()) {
            count += 1;
            last_file = Some(update.file());
        }
    }
    count
}

fn invert_selected(selected: &mut [bool]) {
    for is_selected in selected {
        *is_selected = !*is_selected;
    }
}

fn toggle_all(selected: &mut [bool]) {
    let all_selected = selected.iter().all(|is_selected| *is_selected);
    selected.fill(!all_selected);
}

fn toggle_file(updates: &[ResolvedUpdate], selected: &mut [bool], file: &str) {
    let all_selected = updates
        .iter()
        .zip(selected.iter())
        .filter(|(update, _)| update.file() == file)
        .all(|(_, is_selected)| *is_selected);

    for (update, is_selected) in updates.iter().zip(selected.iter_mut()) {
        if update.file() == file {
            *is_selected = !all_selected;
        }
    }
}
