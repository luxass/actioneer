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

use crate::actions::ActionUpdate;

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

enum VisibleRow {
    FileHeader { file: String },
    Update { original_index: usize },
}

struct State {
    selected: Vec<bool>,
    collapsed: HashSet<String>,
    cursor: usize,
    h_scroll: usize,
    details_visible: bool,
}

impl State {
    fn new(count: usize) -> Self {
        Self {
            selected: vec![false; count],
            collapsed: HashSet::new(),
            cursor: 0,
            h_scroll: 0,
            details_visible: true,
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
                        .filter(|(a, _)| a.action.file == *file)
                        .all(|(_, s)| *s);
                    for (a, s) in actions.iter().zip(state.selected.iter_mut()) {
                        if a.action.file == *file {
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
                    .filter(|(a, _)| a.action.file == file)
                    .all(|(_, s)| *s);
                for (a, s) in actions.iter().zip(state.selected.iter_mut()) {
                    if a.action.file == file {
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
            Key::ToggleDetails => state.details_visible = !state.details_visible,
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
            Key::OpenGitHub => {
                if let Some(VisibleRow::Update { original_index }) = visible.get(state.cursor) {
                    open_github(&actions[*original_index])?;
                }
            }
            Key::Accept => return Ok(state.selected_indices()),
            Key::Cancel => return Err(Error::Canceled),
            Key::Resize | Key::Ignore => {}
        }
    }
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

fn draw(
    frame: &mut ratatui::Frame<'_>,
    actions: &[ActionUpdate],
    visible: &[VisibleRow],
    state: &State,
) {
    let area = frame.area();
    let sections =
        layout::Layout::vertical([layout::Constraint::Min(6), layout::Constraint::Length(3)])
            .split(area);

    let short_ref = |value: &str| {
        if value.chars().count() > 12 {
            value.chars().take(7).collect::<String>()
        } else {
            value.to_string()
        }
    };
    let version_change = |a: &ActionUpdate| {
        let current = a
            .action
            .version_comment
            .as_deref()
            .unwrap_or(&a.action.current_ref);
        if current == a.new_version {
            a.new_version.clone()
        } else {
            format!("{} -> {}", current, a.new_version)
        }
    };
    let notes = |a: &ActionUpdate| {
        let mut notes = Vec::new();
        if a.sha_mismatch {
            notes.push("SHA mismatch");
        }
        if a.is_branch {
            notes.push("mutable branch");
        }
        if a.is_major {
            notes.push("major update");
        }
        notes
    };

    let (act_w, ref_w, ver_w, loc_w) =
        actions
            .iter()
            .fold((0usize, 0usize, 0usize, 0usize), |(a, r, v, l), x| {
                let refs = format!(
                    "{} -> {}",
                    short_ref(&x.action.current_ref),
                    short_ref(&x.new_ref)
                );
                let version = version_change(x);
                let loc = format!("{}:{}", x.action.file, x.action.line);
                (
                    a.max(x.action_name().chars().count()),
                    r.max(refs.chars().count()),
                    v.max(version.chars().count()),
                    l.max(loc.chars().count()),
                )
            });

    let content_width = actions
        .iter()
        .map(|a| {
            let mut w = 6 + act_w + 2 + ref_w + 2 + ver_w + 2 + loc_w;
            let notes = notes(a).join(", ");
            if !notes.is_empty() {
                w += 2 + notes.chars().count();
            }
            w
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

    items.extend(visible.iter().map(|row| match row {
        VisibleRow::FileHeader { file } => {
            let (sel, total) = actions
                .iter()
                .zip(state.selected.iter())
                .filter(|(a, _)| a.action.file == *file)
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
                        format!(
                            "{} -> {}",
                            short_ref(&a.action.current_ref),
                            short_ref(&a.new_ref)
                        ),
                        ref_w
                    ),
                    Style::default().fg(target_s),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:1$}", version_change(a), ver_w),
                    Style::default().fg(Color::Blue),
                ),
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{:1$}",
                        format!("{}:{}", a.action.file, a.action.line),
                        loc_w
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            let notes = notes(a);
            if !notes.is_empty() {
                spans.push(Span::raw("  "));
                for (i, note) in notes.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::styled(", ", Style::default().fg(Color::DarkGray)));
                    }
                    spans.push(Span::styled(*note, Style::default().fg(target_s)));
                }
            }
            ListItem::new(Line::from(scroll_spans(spans, scroll)))
        }
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
        let detail_lines = match visible.get(state.cursor) {
            Some(VisibleRow::FileHeader { file }) => {
                let (sel, total) = actions
                    .iter()
                    .zip(state.selected.iter())
                    .filter(|(a, _)| a.action.file == *file)
                    .fold((0, 0), |(s, t), (_, x)| (s + usize::from(*x), t + 1));
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
                let note_text = notes(a).join(", ");
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
                        Span::raw(version_change(a)),
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
        frame.render_widget(details, details_area);
    }

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
    use crate::actions::ActionReference;

    fn mk_action(file: &str, name: &str) -> ActionUpdate {
        ActionUpdate {
            action: ActionReference::from_discovery(
                "actions".into(),
                name.into(),
                String::new(),
                "v1.0.0".into(),
                Some("1.0.0".into()),
                file.into(),
                10,
                20,
                26,
            ),
            new_ref: "sha".into(),
            new_version: "v1.0.0".into(),
            expected_sha: String::new(),
            sha_mismatch: false,
            is_branch: false,
            is_major: false,
        }
    }

    #[test]
    fn state_new_defaults() {
        let s = State::new(3);
        assert_eq!(vec![false, false, false], s.selected);
        assert_eq!(0, s.cursor);
        assert_eq!(0, s.h_scroll);
        assert!(s.details_visible);
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

    #[test]
    fn github_url_repo_root() {
        let action = mk_action("ci.yml", "checkout");
        assert_eq!("https://github.com/actions/checkout", github_url(&action));
    }

    #[test]
    fn github_url_action_path_at_current_ref() {
        let action = ActionUpdate {
            action: ActionReference::from_discovery(
                "luxass".into(),
                "shared-workflows".into(),
                "/.github/workflows/reusable-ci.yaml".into(),
                "ce9e8e27".into(),
                Some("v0.6.0".into()),
                "ci.yml".into(),
                10,
                20,
                28,
            ),
            new_ref: "sha".into(),
            new_version: "v0.6.0".into(),
            expected_sha: String::new(),
            sha_mismatch: false,
            is_branch: false,
            is_major: false,
        };
        assert_eq!(
            "https://github.com/luxass/shared-workflows/blob/ce9e8e27/.github/workflows/reusable-ci.yaml",
            github_url(&action)
        );
    }
}
