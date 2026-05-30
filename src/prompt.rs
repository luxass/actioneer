use std::collections::HashSet;
use std::io::{self, IsTerminal};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::Action;

const FOOTER: &str = "Up/Down/j/k move  ←→ scroll  PgUp/PgDn jump  space toggle  tab fold  f file  a all  i invert  n none  enter apply  q cancel";

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

enum VisibleRow {
    FileHeader { file: String },
    Update { original_index: usize },
}

struct State {
    selected: Vec<bool>,
    collapsed: HashSet<String>,
    cursor: usize,
    h_scroll: usize,
}

impl State {
    fn new(count: usize) -> Self {
        Self {
            selected: vec![false; count],
            collapsed: HashSet::new(),
            cursor: 0,
            h_scroll: 0,
        }
    }

    fn selected_indices(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.then_some(i))
            .collect()
    }
}

pub fn select(actions: &[Action]) -> Result<Vec<usize>, Error> {
    if actions.is_empty() {
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

    let result = run(&mut terminal, actions);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run<B: Backend>(terminal: &mut Terminal<B>, actions: &[Action]) -> Result<Vec<usize>, Error> {
    let mut state = State::new(actions.len());

    loop {
        let visible = visible_rows(actions, &state.collapsed);
        if state.cursor >= visible.len() {
            state.cursor = visible.len().saturating_sub(1);
        }

        terminal.draw(|frame| draw(frame, actions, &visible, &state))?;

        match read_key()? {
            Key::Up => {
                state.cursor = if state.cursor > 0 {
                    state.cursor - 1
                } else {
                    visible.len().saturating_sub(1)
                };
            }
            Key::Down => {
                state.cursor = if state.cursor + 1 < visible.len() {
                    state.cursor + 1
                } else {
                    0
                };
            }
            Key::Toggle => match &visible[state.cursor] {
                VisibleRow::FileHeader { file } => {
                    let all_on = actions
                        .iter()
                        .zip(state.selected.iter())
                        .filter(|(a, _)| a.file == *file)
                        .all(|(_, s)| *s);
                    for (a, s) in actions.iter().zip(state.selected.iter_mut()) {
                        if a.file == *file {
                            *s = !all_on;
                        }
                    }
                }
                VisibleRow::Update { original_index } => {
                    state.selected[*original_index] = !state.selected[*original_index];
                }
            },
            Key::ToggleAll => {
                let all = state.selected.iter().all(|s| *s);
                state.selected.fill(!all);
            }
            Key::ToggleFile => {
                let file = cursor_file(&visible, state.cursor, actions);
                let all_on = actions
                    .iter()
                    .zip(state.selected.iter())
                    .filter(|(a, _)| a.file == file)
                    .all(|(_, s)| *s);
                for (a, s) in actions.iter().zip(state.selected.iter_mut()) {
                    if a.file == file {
                        *s = !all_on;
                    }
                }
            }
            Key::ToggleCollapse => {
                let file = cursor_file(&visible, state.cursor, actions);
                if state.collapsed.contains(file) {
                    state.collapsed.remove(file);
                } else {
                    state.collapsed.insert(file.to_string());
                }
            }
            Key::PageUp => {
                let page = usize::from(terminal.size()?.height.saturating_sub(4)).max(1);
                state.cursor = state.cursor.saturating_sub(page);
            }
            Key::PageDown => {
                let page = usize::from(terminal.size()?.height.saturating_sub(4)).max(1);
                state.cursor = (state.cursor + page).min(visible.len().saturating_sub(1));
            }
            Key::Home => state.cursor = 0,
            Key::End => state.cursor = visible.len().saturating_sub(1),
            Key::Invert => {
                for s in &mut state.selected {
                    *s = !*s;
                }
            }
            Key::SelectNone => state.selected.fill(false),
            Key::ScrollLeft => state.h_scroll = state.h_scroll.saturating_sub(4),
            Key::ScrollRight => state.h_scroll += 4,
            Key::Accept => return Ok(state.selected_indices()),
            Key::Cancel => return Err(Error::Canceled),
            Key::Resize | Key::Ignore => {}
        }
    }
}

fn cursor_file<'a>(visible: &'a [VisibleRow], cursor: usize, actions: &'a [Action]) -> &'a str {
    match visible.get(cursor) {
        Some(VisibleRow::FileHeader { file }) => file.as_str(),
        Some(VisibleRow::Update { original_index }) => &actions[*original_index].file,
        None => "",
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
    PageUp,
    PageDown,
    Home,
    End,
    Invert,
    SelectNone,
    Accept,
    Cancel,
    Resize,
    ScrollLeft,
    ScrollRight,
    Ignore,
}

fn read_key() -> Result<Key, Error> {
    loop {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                return Ok(match key.code {
                    KeyCode::Up | KeyCode::Char('k') => Key::Up,
                    KeyCode::Down | KeyCode::Char('j') => Key::Down,
                    KeyCode::Left => Key::ScrollLeft,
                    KeyCode::Right => Key::ScrollRight,
                    KeyCode::Enter => Key::Accept,
                    KeyCode::Char(' ') | KeyCode::Char('x') => Key::Toggle,
                    KeyCode::Char('a') => Key::ToggleAll,
                    KeyCode::Char('f') => Key::ToggleFile,
                    KeyCode::Tab => Key::ToggleCollapse,
                    KeyCode::PageUp => Key::PageUp,
                    KeyCode::PageDown => Key::PageDown,
                    KeyCode::Home => Key::Home,
                    KeyCode::End => Key::End,
                    KeyCode::Char('i') => Key::Invert,
                    KeyCode::Char('n') => Key::SelectNone,
                    KeyCode::Char('q') | KeyCode::Esc => Key::Cancel,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Err(Error::Interrupted);
                    }
                    _ => Key::Ignore,
                });
            }
            Event::Resize(_, _) => return Ok(Key::Resize),
            _ => {}
        }
    }
}

fn visible_rows(actions: &[Action], collapsed: &HashSet<String>) -> Vec<VisibleRow> {
    let mut indexed: Vec<(usize, &str)> = actions
        .iter()
        .enumerate()
        .map(|(i, a)| (i, a.file.as_str()))
        .collect();
    indexed.sort_by_key(|(_, f)| *f);

    let mut rows = Vec::new();
    let mut last: Option<&str> = None;
    for (i, file) in indexed {
        if last != Some(file) {
            rows.push(VisibleRow::FileHeader {
                file: file.to_string(),
            });
            if !collapsed.contains(file) {
                rows.push(VisibleRow::Update { original_index: i });
            }
        } else if !collapsed.contains(file) {
            rows.push(VisibleRow::Update { original_index: i });
        }
        last = Some(file);
    }
    rows
}

// --- draw ---

fn draw(frame: &mut ratatui::Frame<'_>, actions: &[Action], visible: &[VisibleRow], state: &State) {
    let area = frame.area();
    let sections =
        layout::Layout::vertical([layout::Constraint::Min(6), layout::Constraint::Length(3)])
            .split(area);

    let (act_w, chg_w, loc_w) = actions.iter().fold((0usize, 0, 0), |(a, c, l), x| {
        let change = format!("{} -> {}", x.current_ref, x.new_version);
        let loc = format!("{}:{}", x.file, x.line);
        (
            a.max(x.action_name().chars().count()),
            c.max(change.chars().count()),
            l.max(loc.chars().count()),
        )
    });

    let content_width = actions
        .iter()
        .map(|a| {
            let mut w = 6 + act_w + 2 + chg_w + 2 + loc_w;
            if a.new_ref != a.new_version {
                w += 3 + a.new_ref.len().min(7) + 1;
            }
            if let Some(vc) = &a.version_comment {
                w += 1 + vc.chars().count() + 1;
            }
            if a.sha_mismatch {
                w += 5 + a.expected_sha.len().min(7) + 1;
            }
            if a.is_branch {
                w += 7;
            }
            if a.is_major {
                w += 6;
            }
            w
        })
        .max()
        .unwrap_or(0);

    let viewport = usize::from(area.width.saturating_sub(2));
    let scroll = state.h_scroll.min(content_width.saturating_sub(viewport));

    let items: Vec<ListItem<'_>> = visible
        .iter()
        .map(|row| match row {
            VisibleRow::FileHeader { file } => {
                let (sel, total) = actions
                    .iter()
                    .zip(state.selected.iter())
                    .filter(|(a, _)| a.file == *file)
                    .fold((0, 0), |(s, t), (_, x)| (s + usize::from(*x), t + 1));
                let marker = if state.collapsed.contains(file) {
                    "▸"
                } else {
                    "▾"
                };
                let label = if sel == total {
                    "all".to_string()
                } else {
                    format!("{sel}/{total}")
                };
                let style = |c| Style::default().fg(c).add_modifier(Modifier::BOLD);
                ListItem::new(Line::from(scroll_spans(
                    vec![
                        Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
                        Span::styled(
                            format!("[{label}] "),
                            style(if sel > 0 {
                                Color::Green
                            } else {
                                Color::DarkGray
                            }),
                        ),
                        Span::styled(file.clone(), style(Color::Cyan)),
                    ],
                    scroll,
                )))
            }
            VisibleRow::Update { original_index } => {
                let a = &actions[*original_index];
                let sel = state.selected[*original_index];
                let marker = if sel { "[x]" } else { "[ ]" };
                let marker_s = if sel {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let target_s = if a.sha_mismatch || a.is_branch {
                    Color::Yellow
                } else if a.is_major {
                    Color::Red
                } else {
                    Color::Green
                };

                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(marker, marker_s),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:1$}", a.action_name(), act_w),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!(
                            "{:1$}",
                            format!("{} -> {}", a.current_ref, a.new_version),
                            chg_w
                        ),
                        Style::default().fg(target_s),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("{:1$}", format!("{}:{}", a.file, a.line), loc_w),
                        Style::default().fg(Color::DarkGray),
                    ),
                ];
                if a.new_ref != a.new_version {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        format!("@{}", &a.new_ref[..a.new_ref.len().min(7)]),
                        Style::default().fg(Color::Blue),
                    ));
                    spans.push(Span::raw(" "));
                }
                if let Some(vc) = &a.version_comment {
                    spans.push(Span::styled("#", Style::default().fg(Color::DarkGray)));
                    spans.push(Span::styled(vc.clone(), Style::default().fg(Color::Blue)));
                    spans.push(Span::raw(" "));
                }
                if a.sha_mismatch {
                    spans.push(Span::styled("sha!", Style::default().fg(Color::Yellow)));
                    spans.push(Span::raw(" "));
                }
                if a.is_branch {
                    spans.push(Span::styled("branch", Style::default().fg(Color::Yellow)));
                    spans.push(Span::raw(" "));
                }
                if a.is_major {
                    spans.push(Span::styled("major", Style::default().fg(Color::Red)));
                }
                ListItem::new(Line::from(scroll_spans(spans, scroll)))
            }
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));
    let selected_count = state.selected.iter().filter(|s| **s).count();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    " Updates — {selected_count}/{} selected ",
                    actions.len()
                )),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 52, 64))
                .add_modifier(Modifier::BOLD),
        )
        .repeat_highlight_symbol(false);
    frame.render_stateful_widget(list, sections[0], &mut list_state);

    let footer = Paragraph::new(FOOTER)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Keys "),
        )
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, sections[1]);
}

fn scroll_spans(spans: Vec<Span<'static>>, scroll: usize) -> Vec<Span<'static>> {
    if scroll == 0 {
        return spans;
    }
    let mut remaining = scroll;
    spans
        .into_iter()
        .flat_map(|span| {
            if remaining == 0 {
                return vec![span];
            }
            let chars: Vec<char> = span.content.chars().collect();
            if remaining >= chars.len() {
                remaining -= chars.len();
                vec![]
            } else {
                let r = remaining;
                remaining = 0;
                vec![Span::styled(
                    chars[r..].iter().collect::<String>(),
                    span.style,
                )]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_action(file: &str, name: &str) -> Action {
        Action::from_scan(
            "actions".into(),
            name.into(),
            String::new(),
            "v1.0.0".into(),
            Some("1.0.0".into()),
            file.into(),
            10,
            20,
            26,
        )
    }

    #[test]
    fn state_new_defaults() {
        let s = State::new(3);
        assert_eq!(vec![false, false, false], s.selected);
        assert_eq!(0, s.cursor);
        assert_eq!(0, s.h_scroll);
    }

    #[test]
    fn state_new_zero() {
        let s = State::new(0);
        assert!(s.selected.is_empty());
    }

    #[test]
    fn selected_indices_none() {
        let s = State::new(4);
        assert_eq!(Vec::<usize>::new(), s.selected_indices());
    }

    #[test]
    fn selected_indices_some() {
        let mut s = State::new(4);
        s.selected = vec![false, true, false, true];
        assert_eq!(vec![1, 3], s.selected_indices());
    }

    #[test]
    fn selected_indices_all() {
        let mut s = State::new(3);
        s.selected = vec![true, true, true];
        assert_eq!(vec![0, 1, 2], s.selected_indices());
    }

    #[test]
    fn visible_rows_single_file_two_actions() {
        let actions = vec![
            mk_action("a.yml", "checkout"),
            mk_action("a.yml", "setup-node"),
        ];
        let rows = visible_rows(&actions, &HashSet::new());
        assert_eq!(3, rows.len());
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
        assert!(matches!(&rows[1], VisibleRow::Update { .. }));
        assert!(matches!(&rows[2], VisibleRow::Update { .. }));
    }

    #[test]
    fn visible_rows_two_files() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("b.yml", "c2")];
        let rows = visible_rows(&actions, &HashSet::new());
        assert_eq!(4, rows.len());
    }

    #[test]
    fn visible_rows_collapsed_hides_updates() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2")];
        let collapsed = HashSet::from(["a.yml".into()]);
        let rows = visible_rows(&actions, &collapsed);
        assert_eq!(1, rows.len());
        assert!(matches!(&rows[0], VisibleRow::FileHeader { file } if file == "a.yml"));
    }

    #[test]
    fn visible_rows_all_collapsed_only_headers() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("b.yml", "c2")];
        let collapsed = HashSet::from(["a.yml".into(), "b.yml".into()]);
        let rows = visible_rows(&actions, &collapsed);
        assert_eq!(2, rows.len());
    }

    #[test]
    fn cursor_file_on_header() {
        let actions = vec![mk_action("a.yml", "c1")];
        let visible = visible_rows(&actions, &HashSet::new());
        assert_eq!("a.yml", cursor_file(&visible, 0, &actions));
    }

    #[test]
    fn cursor_file_on_update() {
        let actions = vec![mk_action("f.yml", "c1")];
        let visible = visible_rows(&actions, &HashSet::new());
        assert_eq!("f.yml", cursor_file(&visible, 1, &actions));
    }

    #[test]
    fn cursor_file_out_of_bounds() {
        let visible: Vec<VisibleRow> = vec![];
        assert_eq!("", cursor_file(&visible, 99, &[]));
    }
}
