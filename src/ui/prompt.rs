use std::collections::HashSet;
use std::io::{self, IsTerminal, Stdout};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::ResolvedUpdate;

const FOOTER: &str =
    "Up/Down/j/k move  space row  f file  tab fold  enter apply  a all  i invert  n none  q cancel";
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
    ToggleCollapse,
    Invert,
    SelectNone,
    Accept,
    Cancel,
    Ignore,
}

enum VisibleRow {
    FileHeader { file: String },
    Update { original_index: usize },
}

fn build_visible_rows(updates: &[ResolvedUpdate], collapsed: &HashSet<String>) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    let mut last_file: Option<&str> = None;

    for (index, update) in updates.iter().enumerate() {
        let file = update.file();
        let is_first = last_file != Some(file);

        if is_first {
            rows.push(VisibleRow::FileHeader {
                file: file.to_string(),
            });
            if !collapsed.contains(file) {
                rows.push(VisibleRow::Update {
                    original_index: index,
                });
            }
        } else if !collapsed.contains(file) {
            rows.push(VisibleRow::Update {
                original_index: index,
            });
        }

        last_file = Some(file);
    }

    rows
}

fn file_at_cursor(
    visible_rows: &[VisibleRow],
    cursor: usize,
    updates: &[ResolvedUpdate],
) -> String {
    if cursor >= visible_rows.len() {
        return updates[0].file().to_string();
    }
    match &visible_rows[cursor] {
        VisibleRow::FileHeader { file } => file.clone(),
        VisibleRow::Update { original_index } => updates[*original_index].file().to_string(),
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
    let mut collapsed: HashSet<String> = HashSet::new();
    let mut cursor = 0usize;
    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        let visible_rows = build_visible_rows(updates, &collapsed);
        if cursor >= visible_rows.len() {
            cursor = visible_rows.len().saturating_sub(1);
        }

        terminal
            .draw(|frame| render(frame, updates, &visible_rows, &selected, cursor, &mut state))?;

        match read_key()? {
            Key::Up => {
                if cursor > 0 {
                    cursor -= 1;
                } else {
                    cursor = visible_rows.len() - 1;
                }
                state.select(Some(cursor));
            }
            Key::Down => {
                if cursor + 1 < visible_rows.len() {
                    cursor += 1;
                } else {
                    cursor = 0;
                }
                state.select(Some(cursor));
            }
            Key::Toggle => {
                if let VisibleRow::Update { original_index } = &visible_rows[cursor] {
                    selected[*original_index] = !selected[*original_index];
                }
            }
            Key::ToggleAll => toggle_all(&mut selected),
            Key::ToggleFile => {
                let file = file_at_cursor(&visible_rows, cursor, updates);
                toggle_file(updates, &mut selected, &file);
            }
            Key::ToggleCollapse => {
                let file = file_at_cursor(&visible_rows, cursor, updates);
                if collapsed.contains(&file) {
                    collapsed.remove(&file);
                } else {
                    collapsed.insert(file);
                }
                let new_visible = build_visible_rows(updates, &collapsed);
                if cursor >= new_visible.len() {
                    cursor = new_visible.len().saturating_sub(1);
                }
                state.select(Some(cursor));
            }
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
    visible_rows: &[VisibleRow],
    selected: &[bool],
    cursor: usize,
    state: &mut ListState,
) {
    let area = frame.area();
    let sections = Layout::vertical([Constraint::Min(8), Constraint::Length(3)]).split(area);

    let items: Vec<ListItem<'_>> = visible_rows
        .iter()
        .scan(None, |last_file, row| {
            let result = match row {
                VisibleRow::FileHeader { file } => {
                    *last_file = Some(file.clone());
                    Some(render_file_header(file))
                }
                VisibleRow::Update { original_index } => {
                    let update = &updates[*original_index];
                    let file_changed = last_file.as_deref() != Some(update.file());
                    *last_file = Some(update.file().to_string());
                    Some(render_update_item(
                        update,
                        selected[*original_index],
                        file_changed,
                    ))
                }
            };
            result
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
        .scroll_padding(visible_scroll_padding(visible_rows.len()));
    frame.render_stateful_widget(list, sections[1], state);

    let footer = Paragraph::new(FOOTER)
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, sections[2]);
}

fn render_file_header(file: &str) -> ListItem<'_> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("▸ ", Style::default().fg(Color::Cyan)),
        Span::styled(
            file,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ⋯ ", Style::default().fg(Color::DarkGray)),
        Span::styled("collapsed", Style::default().fg(Color::DarkGray)),
    ]));
    ListItem::new(lines)
}

fn render_update_item(
    update: &ResolvedUpdate,
    is_selected: bool,
    file_changed: bool,
) -> ListItem<'_> {
    let marker = if is_selected { "●" } else { "○" };
    let marker_style = if is_selected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let target_style = if update.has_sha_mismatch() || update.is_branch_ref() {
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
    ]));

    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled("to ", Style::default().fg(Color::DarkGray)),
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
    if update.is_branch_ref() {
        meta.push(Span::styled(
            "  (unpinned branch ref)",
            Style::default().fg(Color::Yellow),
        ));
    }
    lines.push(Line::from(meta));

    ListItem::new(lines)
}

fn visible_scroll_padding(count: usize) -> usize {
    if count <= VISIBLE_ROWS_HINT { 0 } else { 2 }
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
            KeyCode::Tab => Key::ToggleCollapse,
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
