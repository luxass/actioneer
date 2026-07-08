use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
};

use super::app::{App, ScanPhase};
use super::theme;
use super::view::{self, DisplayRow};
use crate::scan::truncate_label;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 24;

const TABLE_COLUMNS: [Constraint; 4] = [
    Constraint::Length(3),
    Constraint::Min(20),
    Constraint::Length(28),
    Constraint::Length(28),
];

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
        let mut spans = vec![
            Span::raw(" "),
            Span::styled("↑↓", theme::key()),
            Span::styled("/jk ", theme::key_label()),
            Span::styled("Space", theme::key()),
            Span::styled(" toggle  ", theme::key_label()),
            Span::styled("Enter", theme::key()),
            Span::styled(" apply/group  ", theme::key_label()),
            Span::styled("a", theme::key()),
            Span::styled(" all  ", theme::key_label()),
            Span::styled("n", theme::key()),
            Span::styled(" none  ", theme::key_label()),
            Span::styled(
                format!("{} ", app.selected_count()),
                theme::success(),
            ),
            Span::styled("selected", theme::key_label()),
        ];

        if let Some(report) = app.report.as_ref()
            && report.stats.config_blocked > 0
        {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("{} blocked by {} level", report.stats.config_blocked, app.config.update),
                theme::warn(),
            ));
        }

        spans.push(Span::raw("  "));
        spans.push(Span::styled("q", theme::key()));
        spans.push(Span::styled(" quit", theme::key_label()));

        Line::from(spans)
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
                    Span::styled(" ✗ ", theme::error()),
                    Span::styled("error: ", theme::error()),
                    Span::styled(msg, theme::value()),
                ])),
                content,
            );
        }
        ScanPhase::Ready => render_select_list(frame, content, app),
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

fn render_select_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let report = app.report.as_ref().unwrap();

    let block = panel_block("planned changes");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut banner_lines: Vec<Line> = Vec::new();
    if let Some(banner) = &app.status_banner {
        banner_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("ℹ ", theme::accent()),
            Span::styled(banner.as_str(), theme::info()),
        ]));
        banner_lines.push(Line::from(""));
    }

    if app.total_planned_items() == 0 {
        banner_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("✓ ", theme::success()),
            Span::styled("No updates planned.", theme::success()),
        ]));
        banner_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!(
                    "Scanned {} workflow(s), {} reference(s).",
                    report.stats.workflows, report.stats.references
                ),
                theme::muted(),
            ),
        ]));
        frame.render_widget(Paragraph::new(banner_lines), inner);
        return;
    }

    let mut list_area = inner;
    if !banner_lines.is_empty() {
        let banner_height = banner_lines.len() as u16;
        frame.render_widget(Paragraph::new(banner_lines), Rect {
            height: banner_height.min(inner.height),
            ..inner
        });
        if inner.height <= banner_height {
            return;
        }
        list_area = Rect {
            y: inner.y + banner_height,
            height: inner.height - banner_height,
            ..inner
        };
    }

    render_grouped_rows(frame, list_area, app);
}

fn render_grouped_rows(frame: &mut Frame, area: Rect, app: &mut App) {
    app.viewport_rows = area.height.saturating_sub(1) as usize;
    let focused_row = app.focused_display_row();

    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(""),
        Cell::from("From").style(theme::column_from()),
        Cell::from("To").style(theme::column_to()),
    ]);

    let visible = app.viewport_rows.max(1);
    let rows: Vec<Row> = app
        .list_view
        .rows
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(visible)
        .map(|(row_idx, row)| table_row(row, app, focused_row == Some(row_idx)))
        .collect();

    let table = Table::new(rows, TABLE_COLUMNS)
        .header(header)
        .column_spacing(1);

    frame.render_widget(table, area);
    paint_selection_highlight(area, app, frame);
}

fn table_row(row: &DisplayRow, app: &App, focused: bool) -> Row<'static> {
    match row {
        DisplayRow::Spacer => Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ]),
        DisplayRow::GroupHeader(group_idx) => {
            let group = &app.groups[*group_idx];
            let label = view::group_header_label(group);
            let style = if focused {
                theme::workflow().add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                theme::workflow()
            };
            Row::new(vec![
                Cell::from(""),
                Cell::from(label).style(style),
                Cell::from(""),
                Cell::from(""),
            ])
        }
        DisplayRow::Action { group, item } => {
            let entry = &app.groups[*group].items[*item];
            let mark = if entry.selected { "✓" } else { "" };
            Row::new(vec![
                Cell::from(mark).style(theme::checkbox(entry.selected)),
                Cell::from(truncate_label(
                    &short_action(&entry.action),
                    40,
                ))
                .style(theme::action_ref()),
                Cell::from(truncate_label(&entry.from_label, 26)).style(theme::from_ref()),
                Cell::from(truncate_label(&entry.to_label, 26)).style(theme::to_ref()),
            ])
        }
    }
}

fn paint_selection_highlight(area: Rect, app: &App, frame: &mut Frame) {
    let Some(focused_row) = app.focused_display_row() else {
        return;
    };
    if focused_row < app.scroll_offset {
        return;
    }
    let visible = app.viewport_rows.max(1);
    let relative = focused_row - app.scroll_offset;
    if relative >= visible {
        return;
    }

    // +1 for table header row
    let row_y = area.y + 1 + relative as u16;
    if row_y >= area.y + area.height {
        return;
    }

    let bg = theme::selection_row();
    let buf = frame.buffer_mut();
    for x in area.x..area.x.saturating_add(area.width) {
        let cell = &mut buf[(x, row_y)];
        cell.set_style(cell.style().patch(bg));
    }
}

fn short_action(raw: &str) -> String {
    raw.split('@').next().unwrap_or(raw).to_string()
}
