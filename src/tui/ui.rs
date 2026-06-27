use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
};

use super::app::{App, ScanPhase, ViewMode};
use super::theme;
use crate::scan::truncate_label;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 24;

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = Paragraph::new(" Terminal too small — please resize ")
            .alignment(Alignment::Center)
            .style(theme::warn());
        frame.render_widget(msg, area);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(frame, layout[0]);
    render_update(frame, layout[1], app);
    render_footer(frame, layout[2], app);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(12)]).split(inner);

    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled("actioneer", theme::bold_brand()),
        Span::styled(" / ", theme::muted()),
        Span::styled("update", theme::bold_accent()),
    ]);
    frame.render_widget(Paragraph::new(title), cols[0]);

    let ver = Paragraph::new(Line::from(vec![
        Span::styled("v", theme::muted()),
        Span::styled(crate::VERSION, theme::success()),
        Span::raw(" "),
    ]))
    .alignment(Alignment::Right);
    frame.render_widget(ver, cols[1]);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let line = if app.phase != ScanPhase::Ready {
        Line::from(vec![
            Span::styled(" q", theme::key()),
            Span::styled(" quit  ", theme::key_label()),
            Span::styled("Esc", theme::key()),
            Span::styled(" quit  ", theme::key_label()),
            Span::styled("Ctrl-C", theme::key()),
            Span::styled(" quit", theme::key_label()),
        ])
    } else {
        match app.view {
            ViewMode::Select => Line::from(vec![
                Span::raw(" "),
                Span::styled("↑↓", theme::key()),
                Span::styled("/jk ", theme::key_label()),
                Span::styled("Space", theme::key()),
                Span::styled(" toggle  ", theme::key_label()),
                Span::styled("a", theme::key()),
                Span::styled(" all  ", theme::key_label()),
                Span::styled("n", theme::key()),
                Span::styled(" none  ", theme::key_label()),
                Span::styled("Enter", theme::key()),
                Span::styled(" confirm  ", theme::key_label()),
                Span::styled(
                    format!("{} ", app.selected_count()),
                    theme::success(),
                ),
                Span::styled("selected  ", theme::key_label()),
                Span::styled("q", theme::key()),
                Span::styled(" quit", theme::key_label()),
            ]),
            ViewMode::Confirm => Line::from(vec![
                Span::raw(" "),
                Span::styled("Enter", theme::key()),
                Span::styled(" apply ", theme::key_label()),
                Span::styled(
                    format!("{}", app.selected_count()),
                    theme::success(),
                ),
                Span::styled("  ", theme::key_label()),
                Span::styled("Esc", theme::key()),
                Span::styled(" back  ", theme::key_label()),
                Span::styled("q", theme::key()),
                Span::styled(" quit", theme::key_label()),
            ]),
        }
    };

    frame.render_widget(Paragraph::new(line), inner);
}

fn render_update(frame: &mut Frame, area: Rect, app: &mut App) {
    let margins = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(area);
    let content = margins[1];

    match app.phase {
        ScanPhase::Scanning => render_scanning_panel(frame, content, app),
        ScanPhase::Failed => {
            let msg = app.error.as_deref().unwrap_or("scan failed");
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(" ✗ ", theme::warn()),
                    Span::styled("error: ", theme::warn()),
                    Span::styled(msg, theme::value()),
                ])),
                content,
            );
        }
        ScanPhase::Ready => match app.view {
            ViewMode::Select => render_select_table(frame, content, app),
            ViewMode::Confirm => render_confirm_panel(frame, content, app),
        },
    }
}

fn panel_block(title: &str) -> Block<'_> {
    Block::new()
        .title(Span::styled(format!(" {title} "), theme::panel_title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme::border())
}

fn render_scanning_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel_block("planned changes");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(app.spinner().to_string(), theme::accent()),
            Span::raw("  "),
            Span::styled("scanning workflows", theme::info()),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("resolving action versions from ", theme::muted()),
            Span::styled("GitHub", theme::bold_accent()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("No cache? ", theme::warn()),
            Span::styled("This can take a moment. ", theme::muted()),
            Span::styled("q", theme::key()),
            Span::styled(" to cancel", theme::key_label()),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_select_table(frame: &mut Frame, area: Rect, app: &mut App) {
    let report = app.report.as_ref().unwrap();

    let block = panel_block("planned changes");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.viewport_rows = inner.height.saturating_sub(2) as usize;

    let mut lines: Vec<Line> = Vec::new();
    if let Some(banner) = &app.status_banner {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("ℹ ", theme::accent()),
            Span::styled(banner.as_str(), theme::info()),
        ]));
        lines.push(Line::from(""));
    }

    if app.selections.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("✓ ", theme::success()),
            Span::styled("No updates planned.", theme::success()),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!(
                    "Scanned {} workflow(s), {} reference(s).",
                    report.stats.workflows, report.stats.references
                ),
                theme::muted(),
            ),
        ]));
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    if !lines.is_empty() {
        let banner_height = lines.len() as u16;
        let banner_area = Rect {
            height: banner_height.min(inner.height),
            ..inner
        };
        frame.render_widget(Paragraph::new(lines), banner_area);
        if inner.height <= banner_height {
            return;
        }
        let table_area = Rect {
            y: inner.y + banner_height,
            height: inner.height - banner_height,
            ..inner
        };
        render_select_table_rows(frame, table_area, app);
        return;
    }

    render_select_table_rows(frame, inner, app);
}

fn render_select_table_rows(frame: &mut Frame, area: Rect, app: &mut App) {
    app.viewport_rows = area.height.saturating_sub(2) as usize;

    let header = Row::new(vec![
        Cell::from(""),
        Cell::from("Workflow"),
        Cell::from("Action"),
        Cell::from("From"),
        Cell::from("To"),
    ])
    .style(theme::label());

    let rows: Vec<Row> = app
        .selections
        .iter()
        .map(|item| {
            let mark = if item.selected { "✓" } else { "·" };
            Row::new(vec![
                Cell::from(mark).style(theme::checkbox(item.selected)),
                Cell::from(item.workflow_name()).style(theme::workflow()),
                Cell::from(short_action(&item.action)).style(theme::action_ref()),
                Cell::from(truncate_label(&item.from_label, 26)).style(theme::from_ref()),
                Cell::from(truncate_label(&item.to_label, 26)).style(theme::to_ref()),
            ])
        })
        .collect();

    let table = Table::new(rows, [
        Constraint::Length(3),
        Constraint::Length(18),
        Constraint::Min(14),
        Constraint::Length(28),
        Constraint::Length(28),
    ])
    .header(header)
    .column_spacing(1);

    frame.render_stateful_widget(table, area, &mut app.table_state);
    paint_selection_highlight(frame, area, app);
}

/// Extend the selection background across the full row width. Per-cell styles only
/// paint behind text, leaving gaps that look like dashes; patching the buffer row
/// after the table fills those gaps while preserving foreground colours.
fn paint_selection_highlight(frame: &mut Frame, area: Rect, app: &App) {
    let Some(selected) = app.table_state.selected() else {
        return;
    };
    let offset = app.table_state.offset();
    if selected < offset {
        return;
    }
    let visible = selected - offset;
    let data_rows = area.height.saturating_sub(1);
    if data_rows == 0 || visible >= data_rows as usize {
        return;
    }

    let row_rect = Rect {
        x: area.x,
        y: area.y + 1 + visible as u16,
        width: area.width,
        height: 1,
    };
    let bg = theme::selection_row();
    let buf = frame.buffer_mut();
    for x in row_rect.x..row_rect.x.saturating_add(row_rect.width) {
        let cell = &mut buf[(x, row_rect.y)];
        cell.set_style(cell.style().patch(bg));
    }
}

fn short_action(raw: &str) -> String {
    raw.split('@').next().unwrap_or(raw).to_string()
}

fn render_confirm_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = panel_block("confirm");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let count = app.selected_count();
    let total = app.selections.len();

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Apply ", theme::value()),
            Span::styled(format!("{count}"), theme::bold_accent()),
            Span::styled(format!(" of {total} planned "), theme::value()),
            Span::styled("update(s)", theme::success()),
            Span::styled("?", theme::value()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("⚠ ", theme::warn()),
            Span::styled(
                "File patching is not implemented yet — selection only.",
                theme::muted(),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Enter", theme::key()),
            Span::styled(" confirm   ", theme::key_label()),
            Span::styled("Esc", theme::key()),
            Span::styled(" back", theme::key_label()),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
