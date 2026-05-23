use std::collections::HashSet;
use std::io::{self, IsTerminal};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::Backend;
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

pub(crate) trait EventSource {
    fn next_event(&mut self) -> io::Result<Event>;
}

struct RealEventSource;

impl EventSource for RealEventSource {
    fn next_event(&mut self) -> io::Result<Event> {
        event::read()
    }
}

#[cfg(test)]
pub(crate) struct TestEventSource {
    events: std::vec::IntoIter<Event>,
}

#[cfg(test)]
impl TestEventSource {
    pub(crate) fn new(events: Vec<Event>) -> Self {
        Self {
            events: events.into_iter(),
        }
    }
}

#[cfg(test)]
impl EventSource for TestEventSource {
    fn next_event(&mut self) -> io::Result<Event> {
        self.events
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no more events"))
    }
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

        match read_key(event_source)? {
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
        .scan(None, |last_file, row| match row {
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
    frame.render_stateful_widget(list, sections[0], state);

    let footer = Paragraph::new(FOOTER)
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, sections[1]);
}

fn render_file_header(file: &str) -> ListItem<'_> {
    let lines = vec![
        Line::from(vec![
            Span::styled("▸ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                file,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ⋯ ", Style::default().fg(Color::DarkGray)),
            Span::styled("collapsed", Style::default().fg(Color::DarkGray)),
        ]),
    ];
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

fn read_key<E: EventSource>(event_source: &mut E) -> Result<Key, Error> {
    loop {
        let event = event_source.next_event()?;
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

pub(crate) fn invert_selected(selected: &mut [bool]) {
    for is_selected in selected {
        *is_selected = !*is_selected;
    }
}

pub(crate) fn toggle_all(selected: &mut [bool]) {
    let all_selected = selected.iter().all(|is_selected| *is_selected);
    selected.fill(!all_selected);
}

pub(crate) fn toggle_file(updates: &[ResolvedUpdate], selected: &mut [bool], file: &str) {
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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::backend::TestBackend;

    use crate::model::{ResolvedUpdate, ValidationState, UpdateTarget, UpdateSource};

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

    fn make_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn char_event(c: char) -> Event {
        make_event(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn special_event(code: KeyCode) -> Event {
        make_event(code, KeyModifiers::NONE)
    }

    #[test]
    fn build_visible_rows_single_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
        ];
        let collapsed = HashSet::new();
        let rows = build_visible_rows(&updates, &collapsed);

        assert_eq!(rows.len(), 3);
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
        assert!(matches!(&rows[1], VisibleRow::Update { original_index } if *original_index == 0));
        assert!(matches!(&rows[2], VisibleRow::Update { original_index } if *original_index == 1));
    }

    #[test]
    fn build_visible_rows_multiple_files() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/setup-node"),
        ];
        let collapsed = HashSet::new();
        let rows = build_visible_rows(&updates, &collapsed);

        assert_eq!(rows.len(), 4);
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
        assert!(matches!(&rows[1], VisibleRow::Update { original_index } if *original_index == 0));
        assert!(matches!(&rows[2], VisibleRow::FileHeader { file } if file == "b.yml"));
        assert!(matches!(&rows[3], VisibleRow::Update { original_index } if *original_index == 1));
    }

    #[test]
    fn build_visible_rows_collapsed_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
        ];
        let mut collapsed = HashSet::new();
        collapsed.insert("a.yml".to_string());
        let rows = build_visible_rows(&updates, &collapsed);

        assert_eq!(rows.len(), 1);
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
    }

    #[test]
    fn invert_selected_flips_all() {
        let mut selected = vec![true, false, true];
        invert_selected(&mut selected);
        assert_eq!(selected, vec![false, true, false]);
    }

    #[test]
    fn toggle_all_selects_when_none_selected() {
        let mut selected = vec![false, false, false];
        toggle_all(&mut selected);
        assert_eq!(selected, vec![true, true, true]);
    }

    #[test]
    fn toggle_all_deselects_when_all_selected() {
        let mut selected = vec![true, true, true];
        toggle_all(&mut selected);
        assert_eq!(selected, vec![false, false, false]);
    }

    #[test]
    fn toggle_all_selects_when_partial() {
        let mut selected = vec![true, false, true];
        toggle_all(&mut selected);
        assert_eq!(selected, vec![true, true, true]);
    }

    #[test]
    fn toggle_file_selects_all_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let mut selected = vec![false, false, false];
        toggle_file(&updates, &mut selected, "a.yml");
        assert_eq!(selected, vec![true, true, false]);
    }

    #[test]
    fn toggle_file_deselects_all_in_file() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("a.yml", "actions/setup-node"),
            make_update("b.yml", "actions/cache"),
        ];
        let mut selected = vec![true, true, false];
        toggle_file(&updates, &mut selected, "a.yml");
        assert_eq!(selected, vec![false, false, false]);
    }

    #[test]
    fn read_key_maps_up() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Up)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Up);
    }

    #[test]
    fn read_key_maps_down() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Down)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Down);
    }

    #[test]
    fn read_key_maps_j_to_down() {
        let mut src = TestEventSource::new(vec![char_event('j')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Down);
    }

    #[test]
    fn read_key_maps_k_to_up() {
        let mut src = TestEventSource::new(vec![char_event('k')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Up);
    }

    #[test]
    fn read_key_maps_enter_to_accept() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Enter)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Accept);
    }

    #[test]
    fn read_key_maps_space_to_toggle() {
        let mut src = TestEventSource::new(vec![char_event(' ')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Toggle);
    }

    #[test]
    fn read_key_maps_x_to_toggle() {
        let mut src = TestEventSource::new(vec![char_event('x')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Toggle);
    }

    #[test]
    fn read_key_maps_a_to_toggle_all() {
        let mut src = TestEventSource::new(vec![char_event('a')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ToggleAll);
    }

    #[test]
    fn read_key_maps_f_to_toggle_file() {
        let mut src = TestEventSource::new(vec![char_event('f')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ToggleFile);
    }

    #[test]
    fn read_key_maps_tab_to_collapse() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Tab)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::ToggleCollapse);
    }

    #[test]
    fn read_key_maps_i_to_invert() {
        let mut src = TestEventSource::new(vec![char_event('i')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Invert);
    }

    #[test]
    fn read_key_maps_n_to_select_none() {
        let mut src = TestEventSource::new(vec![char_event('n')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::SelectNone);
    }

    #[test]
    fn read_key_maps_q_to_cancel() {
        let mut src = TestEventSource::new(vec![char_event('q')]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Cancel);
    }

    #[test]
    fn read_key_maps_esc_to_cancel() {
        let mut src = TestEventSource::new(vec![special_event(KeyCode::Esc)]);
        assert_eq!(read_key(&mut src).unwrap(), Key::Cancel);
    }

    #[test]
    fn read_key_maps_ctrl_c_to_interrupted() {
        let mut src = TestEventSource::new(vec![make_event(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )]);
        let result = read_key(&mut src);
        assert!(matches!(result, Err(Error::Interrupted)));
    }

    #[test]
    fn read_key_ignores_unknown_keys() {
        let mut src = TestEventSource::new(vec![
            char_event('z'),
            special_event(KeyCode::F(1)),
            char_event('y'),
            char_event(' '),
        ]);
        read_key(&mut src).unwrap();
        read_key(&mut src).unwrap();
        read_key(&mut src).unwrap();
        assert_eq!(read_key(&mut src).unwrap(), Key::Toggle);
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
        let events = vec![
            char_event('f'),
            special_event(KeyCode::Enter),
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut src = TestEventSource::new(events);

        let result = run_prompt(&mut terminal, &mut src, &updates).unwrap();
        assert_eq!(result, vec![0]);
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
