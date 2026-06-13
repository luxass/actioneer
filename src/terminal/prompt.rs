use std::collections::HashSet;
use std::io::{self, IsTerminal};
use std::process::{Command, Stdio};

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

use crate::actions::{ActionUpdate, UpdateNote};

const FOOTER: &str = "Up/Down/j/k move  ←→ scroll  PgUp/PgDn jump  d details  o open GitHub  space toggle  tab fold  f file  a all  i invert  n none  enter apply  q cancel";

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

#[derive(Clone)]
enum VisibleRow {
    FileHeader { file: String },
    Update { original_index: usize },
}

struct State {
    selected: Vec<bool>,
    collapsed: HashSet<String>,
    visible: Vec<VisibleRow>,
    cursor: usize,
    h_scroll: usize,
    details_visible: bool,
}

impl State {
    fn new(actions: &[ActionUpdate]) -> Self {
        Self {
            selected: vec![false; actions.len()],
            collapsed: HashSet::new(),
            visible: visible_rows(actions, &HashSet::new()),
            cursor: 0,
            h_scroll: 0,
            details_visible: true,
        }
    }

    fn toggle_collapse(&mut self, actions: &[ActionUpdate], file: &str) {
        if !self.collapsed.remove(file) {
            self.collapsed.insert(file.to_string());
        }
        self.visible = visible_rows(actions, &self.collapsed);
    }

    fn selected_indices(&self) -> Vec<usize> {
        self.selected
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.then_some(i))
            .collect()
    }
}

pub fn select(actions: &[ActionUpdate]) -> Result<Vec<usize>, Error> {
    if actions.is_empty() {
        return Ok(Vec::new());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::NotATerminal);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(Error::Io(err));
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(err) => {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            let _ = execute!(stdout, LeaveAlternateScreen);
            return Err(Error::Io(err));
        }
    };

    let result = run(&mut terminal, actions);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    actions: &[ActionUpdate],
) -> Result<Vec<usize>, Error> {
    let mut state = State::new(actions);

    loop {
        if state.cursor >= state.visible.len() {
            state.cursor = state.visible.len().saturating_sub(1);
        }

        terminal.draw(|frame| draw(frame, actions, &state))?;

        match read_key()? {
            Key::Up => {
                state.cursor = if state.cursor > 0 {
                    state.cursor - 1
                } else {
                    state.visible.len().saturating_sub(1)
                };
            }
            Key::Down => {
                state.cursor = if state.cursor + 1 < state.visible.len() {
                    state.cursor + 1
                } else {
                    0
                };
            }
            Key::Toggle => match state.visible.get(state.cursor).cloned() {
                Some(VisibleRow::FileHeader { file }) => {
                    toggle_file_selection(actions, &mut state.selected, &file);
                }
                Some(VisibleRow::Update { original_index }) => {
                    state.selected[original_index] = !state.selected[original_index];
                }
                None => {}
            },
            Key::ToggleAll => {
                let all = state.selected.iter().all(|s| *s);
                state.selected.fill(!all);
            }
            Key::ToggleFile => {
                let file = cursor_file(&state.visible, state.cursor, actions).to_string();
                toggle_file_selection(actions, &mut state.selected, &file);
            }
            Key::ToggleCollapse => {
                let file = cursor_file(&state.visible, state.cursor, actions).to_string();
                state.toggle_collapse(actions, &file);
            }
            Key::ToggleDetails => state.details_visible = !state.details_visible,
            Key::PageUp => {
                let page = usize::from(terminal.size()?.height.saturating_sub(4)).max(1);
                state.cursor = state.cursor.saturating_sub(page);
            }
            Key::PageDown => {
                let page = usize::from(terminal.size()?.height.saturating_sub(4)).max(1);
                state.cursor = (state.cursor + page).min(state.visible.len().saturating_sub(1));
            }
            Key::Home => state.cursor = 0,
            Key::End => state.cursor = state.visible.len().saturating_sub(1),
            Key::Invert => {
                for s in &mut state.selected {
                    *s = !*s;
                }
            }
            Key::SelectNone => state.selected.fill(false),
            Key::ScrollLeft => state.h_scroll = state.h_scroll.saturating_sub(4),
            Key::ScrollRight => state.h_scroll += 4,
            Key::OpenGitHub => {
                if let Some(VisibleRow::Update { original_index }) =
                    state.visible.get(state.cursor).cloned()
                {
                    open_github(&actions[original_index])?;
                }
            }
            Key::Accept => return Ok(state.selected_indices()),
            Key::Cancel => return Err(Error::Canceled),
            Key::Resize | Key::Ignore => {}
        }
    }
}

fn toggle_file_selection(actions: &[ActionUpdate], selected: &mut [bool], file: &str) {
    let all_on = actions
        .iter()
        .zip(selected.iter())
        .filter(|(a, _)| a.action.file == file)
        .all(|(_, s)| *s);
    for (a, s) in actions.iter().zip(selected.iter_mut()) {
        if a.action.file == file {
            *s = !all_on;
        }
    }
}

fn file_selection_counts(
    actions: &[ActionUpdate],
    selected: &[bool],
    file: &str,
) -> (usize, usize) {
    actions
        .iter()
        .zip(selected.iter())
        .filter(|(a, _)| a.action.file == file)
        .fold((0, 0), |(s, t), (_, x)| (s + usize::from(*x), t + 1))
}

fn cursor_file<'a>(
    visible: &'a [VisibleRow],
    cursor: usize,
    actions: &'a [ActionUpdate],
) -> &'a str {
    match visible.get(cursor) {
        Some(VisibleRow::FileHeader { file }) => file.as_str(),
        Some(VisibleRow::Update { original_index }) => &actions[*original_index].action.file,
        None => "",
    }
}

fn open_github(action: &ActionUpdate) -> Result<(), Error> {
    let url = github_url(action);
    let mut command = if cfg!(target_os = "macos") {
        let mut cmd = Command::new("open");
        cmd.arg(url);
        cmd
    } else if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &url]);
        cmd
    } else {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(url);
        cmd
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn github_url(update: &ActionUpdate) -> String {
    let action = &update.action;
    let repo = format!("https://github.com/{}/{}", action.owner, action.name);
    let path = action.path.trim_start_matches('/');
    if path.is_empty() {
        repo
    } else {
        format!("{repo}/blob/{}/{}", action.current_ref, path)
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
    ToggleDetails,
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
    OpenGitHub,
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
                    KeyCode::Char('d') => Key::ToggleDetails,
                    KeyCode::Tab => Key::ToggleCollapse,
                    KeyCode::PageUp => Key::PageUp,
                    KeyCode::PageDown => Key::PageDown,
                    KeyCode::Home => Key::Home,
                    KeyCode::End => Key::End,
                    KeyCode::Char('o') => Key::OpenGitHub,
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

fn visible_rows(actions: &[ActionUpdate], collapsed: &HashSet<String>) -> Vec<VisibleRow> {
    let mut indexed: Vec<(usize, &str)> = actions
        .iter()
        .enumerate()
        .map(|(i, a)| (i, a.action.file.as_str()))
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

fn short_ref(value: &str) -> String {
    if value.chars().count() > 12 {
        value.chars().take(7).collect()
    } else {
        value.to_string()
    }
}

fn note_label(note: UpdateNote) -> &'static str {
    match note {
        UpdateNote::ShaMismatch => "SHA mismatch",
        UpdateNote::MutableBranch => "mutable branch",
        UpdateNote::MajorUpdate => "major update",
    }
}

fn column_widths(actions: &[ActionUpdate]) -> (usize, usize, usize, usize) {
    actions.iter().fold((0, 0, 0, 0), |(a, r, v, l), x| {
        let refs = format!(
            "{} -> {}",
            short_ref(&x.action.current_ref),
            short_ref(&x.new_ref),
        );
        let loc = format!("{}:{}", x.action.file, x.action.line);
        (
            a.max(x.action_name().chars().count()),
            r.max(refs.chars().count()),
            v.max(x.version_label().chars().count()),
            l.max(loc.chars().count()),
        )
    })
}

fn build_file_header_item(
    file: &str,
    sel: usize,
    total: usize,
    collapsed: bool,
    scroll: usize,
) -> ListItem<'static> {
    let marker = if collapsed { "▸" } else { "▾" };
    let label = if sel == total {
        "all".to_string()
    } else {
        format!("{sel}/{total}")
    };
    let bold = |c| Style::default().fg(c).add_modifier(Modifier::BOLD);
    ListItem::new(Line::from(scroll_spans(
        vec![
            Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("[{label}] "),
                bold(if sel > 0 { Color::Green } else { Color::DarkGray }),
            ),
            Span::styled(file.to_string(), bold(Color::Cyan)),
        ],
        scroll,
    )))
}

fn build_update_item(
    action: &ActionUpdate,
    selected: bool,
    (act_w, ref_w, ver_w, loc_w): (usize, usize, usize, usize),
    scroll: usize,
) -> ListItem<'static> {
    let marker = if selected { "[x]" } else { "[ ]" };
    let marker_s = if selected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let target_s = if action.is_security_sensitive() {
        Color::Yellow
    } else if action.is_major {
        Color::Red
    } else {
        Color::Green
    };
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(marker, marker_s),
        Span::raw(" "),
        Span::styled(
            format!("{:1$}", action.action_name(), act_w),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{:1$}",
                format!(
                    "{} -> {}",
                    short_ref(&action.action.current_ref),
                    short_ref(&action.new_ref),
                ),
                ref_w,
            ),
            Style::default().fg(target_s),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:1$}", action.version_label(), ver_w),
            Style::default().fg(Color::Blue),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "{:1$}",
                format!("{}:{}", action.action.file, action.action.line),
                loc_w,
            ),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    let notes = action.notes();
    if !notes.is_empty() {
        spans.push(Span::raw("  "));
        for (i, note) in notes.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(", ", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::styled(note_label(*note), Style::default().fg(target_s)));
        }
    }
    ListItem::new(Line::from(scroll_spans(spans, scroll)))
}

fn render_details(
    frame: &mut ratatui::Frame<'_>,
    area: layout::Rect,
    actions: &[ActionUpdate],
    state: &State,
) {
    let detail_lines = match state.visible.get(state.cursor) {
        Some(VisibleRow::FileHeader { file }) => {
            let (sel, total) = file_selection_counts(actions, &state.selected, file);
            vec![
                Line::from(Span::styled(
                    file.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(format!("Selected: {sel}/{total}")),
                Line::from(format!("Collapsed: {}", state.collapsed.contains(file))),
            ]
        }
        Some(VisibleRow::Update { original_index }) => {
            let a = &actions[*original_index];
            let note_text = a
                .notes()
                .into_iter()
                .map(note_label)
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                Line::from(Span::styled(
                    a.action_name(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Current ref: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(a.action.current_ref.clone()),
                ]),
                Line::from(vec![
                    Span::styled("Target ref:  ", Style::default().fg(Color::DarkGray)),
                    Span::raw(a.new_ref.clone()),
                ]),
                Line::from(vec![
                    Span::styled("Version:     ", Style::default().fg(Color::DarkGray)),
                    Span::raw(a.version_label()),
                ]),
                Line::from(vec![
                    Span::styled("Location:    ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}:{}", a.action.file, a.action.line)),
                ]),
                Line::from(vec![
                    Span::styled("GitHub:      ", Style::default().fg(Color::DarkGray)),
                    Span::raw(github_url(a)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Notes:       ", Style::default().fg(Color::DarkGray)),
                    Span::raw(if note_text.is_empty() {
                        "none".to_string()
                    } else {
                        note_text
                    }),
                ]),
            ]
        }
        None => vec![Line::from("No selection")],
    };
    let details = Paragraph::new(detail_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Details "),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(details, area);
}

fn render_footer(frame: &mut ratatui::Frame<'_>, area: layout::Rect) {
    let footer = Paragraph::new(FOOTER)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" Keys "),
        )
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(footer, area);
}

fn draw(frame: &mut ratatui::Frame<'_>, actions: &[ActionUpdate], state: &State) {
    let area = frame.area();
    let sections =
        layout::Layout::vertical([layout::Constraint::Min(6), layout::Constraint::Length(3)])
            .split(area);

    let widths = column_widths(actions);
    let (act_w, ref_w, ver_w, loc_w) = widths;
    let content_width = actions
        .iter()
        .map(|a| {
            let notes = a
                .notes()
                .into_iter()
                .map(note_label)
                .collect::<Vec<_>>()
                .join(", ");
            let base = 6 + act_w + 2 + ref_w + 2 + ver_w + 2 + loc_w;
            if notes.is_empty() { base } else { base + 2 + notes.chars().count() }
        })
        .max()
        .unwrap_or(0);

    let show_details = state.details_visible && sections[0].width >= 120;
    let body = if show_details {
        let panes = layout::Layout::horizontal([
            layout::Constraint::Min(60),
            layout::Constraint::Length(42),
        ])
        .split(sections[0]);
        (panes[0], Some(panes[1]))
    } else {
        (sections[0], None)
    };

    let viewport = usize::from(body.0.width.saturating_sub(2));
    let scroll = state.h_scroll.min(content_width.saturating_sub(viewport));

    let header_style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    let mut items = vec![ListItem::new(Line::from(scroll_spans(
        vec![
            Span::raw("      "),
            Span::styled(format!("{:1$}", "Action", act_w), header_style),
            Span::raw("  "),
            Span::styled(format!("{:1$}", "Ref", ref_w), header_style),
            Span::raw("  "),
            Span::styled(format!("{:1$}", "Version", ver_w), header_style),
            Span::raw("  "),
            Span::styled(format!("{:1$}", "Location", loc_w), header_style),
            Span::raw("  "),
            Span::styled("Notes", header_style),
        ],
        scroll,
    )))];

    items.extend(state.visible.iter().map(|row| match row {
        VisibleRow::FileHeader { file } => {
            let (sel, total) = file_selection_counts(actions, &state.selected, file);
            build_file_header_item(file, sel, total, state.collapsed.contains(file), scroll)
        }
        VisibleRow::Update { original_index } => build_update_item(
            &actions[*original_index],
            state.selected[*original_index],
            widths,
            scroll,
        ),
    }));

    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor + 1));
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
    frame.render_stateful_widget(list, body.0, &mut list_state);

    if let Some(details_area) = body.1 {
        render_details(frame, details_area, actions, state);
    }

    render_footer(frame, sections[1]);
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
    use crate::actions::{ActionReference, UpdateNote, fixtures};

    fn mk_action(file: &str, name: &str) -> ActionUpdate {
        fixtures::update(ActionReference {
            name: name.into(),
            version_comment: Some("1.0.0".into()),
            file: file.into(),
            ..fixtures::reference()
        })
    }

    #[test]
    fn state_new_defaults() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2"), mk_action("b.yml", "c3")];
        let s = State::new(&actions);
        assert_eq!(vec![false, false, false], s.selected);
        assert_eq!(0, s.cursor);
        assert_eq!(0, s.h_scroll);
        assert!(s.details_visible);
        assert!(!s.visible.is_empty());
    }

    #[test]
    fn state_new_empty() {
        let s = State::new(&[]);
        assert!(s.selected.is_empty());
        assert!(s.visible.is_empty());
    }

    #[test]
    fn state_toggle_collapse_hides_and_restores() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2")];
        let mut s = State::new(&actions);
        assert_eq!(3, s.visible.len());
        s.toggle_collapse(&actions, "a.yml");
        assert_eq!(1, s.visible.len());
        s.toggle_collapse(&actions, "a.yml");
        assert_eq!(3, s.visible.len());
    }

    #[test]
    fn selected_indices_none() {
        let s = State::new(&[mk_action("a.yml", "c1"), mk_action("a.yml", "c2"), mk_action("a.yml", "c3"), mk_action("a.yml", "c4")]);
        assert_eq!(Vec::<usize>::new(), s.selected_indices());
    }

    #[test]
    fn selected_indices_some() {
        let actions = vec![
            mk_action("a.yml", "c1"),
            mk_action("a.yml", "c2"),
            mk_action("a.yml", "c3"),
            mk_action("a.yml", "c4"),
        ];
        let mut s = State::new(&actions);
        s.selected = vec![false, true, false, true];
        assert_eq!(vec![1, 3], s.selected_indices());
    }

    #[test]
    fn selected_indices_all() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2"), mk_action("a.yml", "c3")];
        let mut s = State::new(&actions);
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

    #[test]
    fn github_url_repo_root() {
        let action = mk_action("ci.yml", "checkout");
        assert_eq!("https://github.com/actions/checkout", github_url(&action));
    }

    #[test]
    fn github_url_action_path_at_current_ref() {
        let action = fixtures::update(ActionReference {
            owner: "luxass".into(),
            name: "shared-workflows".into(),
            path: "/.github/workflows/reusable-ci.yaml".into(),
            current_ref: "ce9e8e27".into(),
            version_comment: Some("v0.6.0".into()),
            ..fixtures::reference()
        });
        assert_eq!(
            "https://github.com/luxass/shared-workflows/blob/ce9e8e27/.github/workflows/reusable-ci.yaml",
            github_url(&action)
        );
    }

    #[test]
    fn short_ref_preserves_short_values() {
        assert_eq!("abc", short_ref("abc"));
        assert_eq!("abcdef012345", short_ref("abcdef012345"));
    }

    #[test]
    fn short_ref_truncates_long_values() {
        assert_eq!("abcdefg", short_ref("abcdefghijklm"));
        assert_eq!("abcdefg", short_ref("abcdefg0123456789"));
    }

    #[test]
    fn note_label_all_variants() {
        assert_eq!("SHA mismatch", note_label(UpdateNote::ShaMismatch));
        assert_eq!("mutable branch", note_label(UpdateNote::MutableBranch));
        assert_eq!("major update", note_label(UpdateNote::MajorUpdate));
    }

    #[test]
    fn column_widths_empty() {
        assert_eq!((0, 0, 0, 0), column_widths(&[]));
    }

    #[test]
    fn column_widths_action_name_width() {
        let actions = vec![mk_action("ci.yml", "checkout")];
        let (act_w, _, _, _) = column_widths(&actions);
        assert_eq!("actions/checkout".chars().count(), act_w);
    }

    #[test]
    fn column_widths_picks_widest_action() {
        let actions = vec![mk_action("a.yml", "x"), mk_action("a.yml", "much-longer-name")];
        let (act_w, _, _, _) = column_widths(&actions);
        assert_eq!("actions/much-longer-name".chars().count(), act_w);
    }

    #[test]
    fn scroll_spans_zero_passthrough() {
        let spans = vec![Span::raw("hello"), Span::raw(" world")];
        let result = scroll_spans(spans, 0);
        assert_eq!(2, result.len());
        assert_eq!("hello", result[0].content.as_ref());
        assert_eq!(" world", result[1].content.as_ref());
    }

    #[test]
    fn scroll_spans_within_first_span() {
        let spans = vec![Span::raw("hello world")];
        let result = scroll_spans(spans, 5);
        assert_eq!(1, result.len());
        assert_eq!(" world", result[0].content.as_ref());
    }

    #[test]
    fn scroll_spans_past_first_span() {
        let spans = vec![Span::raw("abc"), Span::raw("def")];
        let result = scroll_spans(spans, 3);
        assert_eq!(1, result.len());
        assert_eq!("def", result[0].content.as_ref());
    }

    #[test]
    fn scroll_spans_partial_second_span() {
        let spans = vec![Span::raw("abc"), Span::raw("defgh")];
        let result = scroll_spans(spans, 4);
        assert_eq!(1, result.len());
        assert_eq!("efgh", result[0].content.as_ref());
    }

    #[test]
    fn scroll_spans_past_all() {
        let spans = vec![Span::raw("abc"), Span::raw("def")];
        let result = scroll_spans(spans, 100);
        assert!(result.is_empty());
    }

    #[test]
    fn toggle_file_all_off_turns_on() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2")];
        let mut selected = vec![false, false];
        toggle_file_selection(&actions, &mut selected, "a.yml");
        assert_eq!(vec![true, true], selected);
    }

    #[test]
    fn toggle_file_partial_turns_all_on() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2")];
        let mut selected = vec![true, false];
        toggle_file_selection(&actions, &mut selected, "a.yml");
        assert_eq!(vec![true, true], selected);
    }

    #[test]
    fn toggle_file_all_on_turns_off() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2")];
        let mut selected = vec![true, true];
        toggle_file_selection(&actions, &mut selected, "a.yml");
        assert_eq!(vec![false, false], selected);
    }

    #[test]
    fn toggle_file_only_affects_target_file() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("b.yml", "c2")];
        let mut selected = vec![false, true];
        toggle_file_selection(&actions, &mut selected, "a.yml");
        assert_eq!(vec![true, true], selected);
    }

    #[test]
    fn file_selection_counts_none_selected() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("a.yml", "c2")];
        let selected = vec![false, false];
        assert_eq!((0, 2), file_selection_counts(&actions, &selected, "a.yml"));
    }

    #[test]
    fn file_selection_counts_partial() {
        let actions = vec![
            mk_action("a.yml", "c1"),
            mk_action("a.yml", "c2"),
            mk_action("a.yml", "c3"),
        ];
        let selected = vec![true, false, true];
        assert_eq!((2, 3), file_selection_counts(&actions, &selected, "a.yml"));
    }

    #[test]
    fn file_selection_counts_ignores_other_files() {
        let actions = vec![mk_action("a.yml", "c1"), mk_action("b.yml", "c2")];
        let selected = vec![true, true];
        assert_eq!((1, 1), file_selection_counts(&actions, &selected, "a.yml"));
        assert_eq!((1, 1), file_selection_counts(&actions, &selected, "b.yml"));
    }
}
