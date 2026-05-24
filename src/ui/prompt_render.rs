use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::ResolvedUpdate;
use crate::ui::prompt_ui_state::{
    PromptState, VisibleRow, build_visible_rows, file_selection_counts,
};

const FOOTER: &str = "Up/Down/j/k move  ←/→ scroll  PgUp/PgDn jump  space toggle  tab fold  f file  a all  i invert  n none  enter apply  q cancel";
const VISIBLE_ROWS_HINT: usize = 12;

struct ColumnWidths {
    action: usize,
    change: usize,
    location: usize,
    pin: usize,
}

fn natural_column_widths(updates: &[ResolvedUpdate]) -> ColumnWidths {
    updates.iter().fold(
        ColumnWidths {
            action: 0,
            change: 0,
            location: 0,
            pin: 0,
        },
        |mut w, u| {
            w.action = w.action.max(u.action.chars().count());
            w.change = w.change.max(
                format!("{} -> {}", u.current, u.display_target())
                    .chars()
                    .count(),
            );
            w.location = w
                .location
                .max(format!("{}:{}", u.job, u.line()).chars().count());
            if u.next_ref() != u.display_target() {
                w.pin = w
                    .pin
                    .max(format!("@{}", short_sha(u.next_ref())).chars().count());
            }
            w
        },
    )
}

fn total_content_width(widths: &ColumnWidths) -> usize {
    let mut w = 6 + widths.action + 2 + widths.change + 2 + widths.location;
    if widths.pin > 0 {
        w += 2 + widths.pin;
    }
    w
}

fn max_row_width(updates: &[ResolvedUpdate], widths: &ColumnWidths) -> usize {
    updates
        .iter()
        .map(|u| {
            let mut w = total_content_width(widths);
            if u.has_version_comment() {
                w += 1 + u.version_comment().chars().count() + 1;
            }
            if u.has_sha_mismatch() {
                w += 4 + 1 + short_sha_or_full(u).chars().count() + 1;
            }
            if u.is_branch_ref() {
                w += 6 + 1;
            }
            if u.is_major_update() {
                w += 5;
            }
            w
        })
        .max()
        .unwrap_or(0)
}

pub fn render(frame: &mut ratatui::Frame<'_>, updates: &[ResolvedUpdate], state: &PromptState) {
    let visible_rows = build_visible_rows(updates, &state.collapsed);
    let area = frame.area();
    let sections = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(6),
        ratatui::layout::Constraint::Length(3),
    ])
    .split(area);
    let col_widths = natural_column_widths(updates);
    let content_width = max_row_width(updates, &col_widths);
    let viewport_width = usize::from(area.width.saturating_sub(2));
    let max_scroll = content_width.saturating_sub(viewport_width);
    let effective_scroll = state.h_scroll.min(max_scroll);
    let active_scroll = if max_scroll == 0 { 0 } else { effective_scroll };

    let items: Vec<ListItem<'_>> = visible_rows
        .iter()
        .map(|row| match row {
            VisibleRow::FileHeader { file } => render_file_header(
                file,
                updates,
                &state.selected,
                state.collapsed.contains(file),
                active_scroll,
            ),
            VisibleRow::Update { original_index } => {
                let update = &updates[*original_index];
                render_update_item(
                    update,
                    state.selected[*original_index],
                    &col_widths,
                    active_scroll,
                )
            }
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(Some(state.cursor));

    let title = format!(
        " Updates - {}/{} selected - {} ",
        state.selected_count(),
        updates.len(),
        pin_style_summary(updates)
    );

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 52, 64))
                .add_modifier(Modifier::BOLD),
        )
        .repeat_highlight_symbol(false)
        .scroll_padding(visible_scroll_padding(visible_rows.len()));
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

fn render_file_header(
    file: &str,
    updates: &[ResolvedUpdate],
    selected: &[bool],
    is_collapsed: bool,
    h_scroll: usize,
) -> ListItem<'static> {
    let (selected_count, total_count) = file_selection_counts(updates, selected, file);
    let marker = if is_collapsed { "▸" } else { "▾" };
    let state_label = if selected_count == total_count {
        "all".to_string()
    } else {
        format!("{selected_count}/{total_count}")
    };

    let spans = vec![
        Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("[{state_label}] "),
            Style::default()
                .fg(if selected_count > 0 {
                    Color::Green
                } else {
                    Color::DarkGray
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            file.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    ListItem::new(Line::from(apply_h_scroll(spans, h_scroll)))
}

fn render_update_item(
    update: &ResolvedUpdate,
    is_selected: bool,
    widths: &ColumnWidths,
    h_scroll: usize,
) -> ListItem<'static> {
    let marker = if is_selected { "[x]" } else { "[ ]" };
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
    let change = format!("{} -> {}", update.current, update.display_target());
    let location = format!("{}:{}", update.job, update.line());
    let pin = if update.next_ref() != update.display_target() {
        format!("@{}", short_sha(update.next_ref()))
    } else {
        String::new()
    };
    let mut spans = vec![
        Span::raw("  "),
        Span::styled(marker, marker_style),
        Span::raw(" "),
        Span::styled(
            format!("{:1$}", &update.action, widths.action),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(format!("{:1$}", change, widths.change), target_style),
        Span::raw("  "),
        Span::styled(
            format!("{:1$}", location, widths.location),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    if widths.pin > 0 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{:1$}", pin, widths.pin),
            Style::default().fg(Color::Blue),
        ));
    }
    if update.has_version_comment() {
        spans.push(Span::styled("#", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            update.version_comment().to_string(),
            Style::default().fg(Color::Blue),
        ));
        spans.push(Span::raw(" "));
    }
    if update.has_sha_mismatch() {
        spans.push(Span::styled("sha!", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            short_sha_or_full(update),
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::raw(" "));
    }
    if update.is_branch_ref() {
        spans.push(Span::styled("branch", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" "));
    }
    if update.is_major_update() {
        spans.push(Span::styled("major", Style::default().fg(Color::Red)));
    }

    ListItem::new(Line::from(apply_h_scroll(spans, h_scroll)))
}

fn apply_h_scroll(spans: Vec<Span<'static>>, h_scroll: usize) -> Vec<Span<'static>> {
    if h_scroll == 0 {
        return spans;
    }
    let mut remaining = h_scroll;
    spans
        .into_iter()
        .flat_map(|span| {
            if remaining == 0 {
                return vec![span];
            }
            let span_chars: Vec<char> = span.content.chars().collect();
            if remaining >= span_chars.len() {
                remaining -= span_chars.len();
                vec![]
            } else {
                let new_content: String = span_chars[remaining..].iter().collect();
                remaining = 0;
                vec![Span::styled(new_content, span.style)]
            }
        })
        .collect()
}

fn visible_scroll_padding(count: usize) -> usize {
    if count <= VISIBLE_ROWS_HINT { 0 } else { 2 }
}

fn pin_style_summary(updates: &[ResolvedUpdate]) -> &'static str {
    if updates
        .iter()
        .any(|update| update.next_ref() != update.display_target())
    {
        "pinning SHAs"
    } else {
        "using tags"
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

#[cfg(test)]
mod tests {
    use crate::model::{ResolvedUpdate, UpdateSource, UpdateTarget, ValidationState};

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

    fn make_sha_pinned_update(file: &str, action: &str) -> ResolvedUpdate {
        ResolvedUpdate::new(
            action,
            "build",
            "v1.0.0",
            ValidationState::new("abc1234", "1.0.0", false),
            UpdateTarget::new("abcdef1234567890", "v2.0.0", false),
            UpdateSource::new(file, 10, 20, 30),
            false,
        )
    }

    #[test]
    fn natural_column_widths_computes_max_widths() {
        let updates = vec![
            make_update("a.yml", "actions/checkout"),
            make_update("b.yml", "actions/setup-node@v16"),
        ];
        let w = natural_column_widths(&updates);
        assert!(w.action >= "actions/setup-node@v16".chars().count());
        assert!(w.change > 0);
        assert!(w.location > 0);
    }

    #[test]
    fn apply_h_scroll_strips_prefix() {
        let spans = vec![
            Span::raw("  "),
            Span::styled("[x]", Style::default()),
            Span::raw(" hello"),
        ];
        let result = apply_h_scroll(spans, 3);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "x]");
        assert_eq!(result[1].content, " hello");
    }

    #[test]
    fn apply_h_scroll_noop_when_zero() {
        let spans = vec![Span::raw("hello"), Span::raw(" world")];
        let result = apply_h_scroll(spans.clone(), 0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "hello");
        assert_eq!(result[1].content, " world");
    }

    #[test]
    fn pin_style_summary_reports_sha_pinning_when_next_ref_differs_from_label() {
        let updates = vec![make_sha_pinned_update("a.yml", "actions/checkout")];
        assert_eq!(pin_style_summary(&updates), "pinning SHAs");
    }

    #[test]
    fn pin_style_summary_reports_tags_when_next_ref_matches_label() {
        let updates = vec![make_update("a.yml", "actions/checkout")];
        assert_eq!(pin_style_summary(&updates), "using tags");
    }
}
