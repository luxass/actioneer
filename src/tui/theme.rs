use ratatui::style::{Color, Modifier, Style};

/// Default body text — soft gray on dark terminals.
const FG: Color = Color::Rgb(180, 186, 196);
/// Secondary / de-emphasized copy.
const FG_DIM: Color = Color::Rgb(110, 118, 132);
/// Emphasis without shouting.
const FG_BRIGHT: Color = Color::Rgb(220, 224, 230);
/// Single accent for titles and highlights.
const ACCENT: Color = Color::Rgb(140, 176, 220);
/// Muted positive signal.
const SUCCESS: Color = Color::Rgb(130, 188, 150);
/// Muted caution signal.
const WARN: Color = Color::Rgb(196, 160, 110);
/// Panel chrome.
const BORDER: Color = Color::Rgb(58, 62, 72);
/// Selected table row — simulates a ~20% white overlay on a dark background.
const SELECTION_BG: Color = Color::Rgb(44, 48, 58);

pub fn bold_brand() -> Style {
    Style::default()
        .fg(FG_BRIGHT)
        .add_modifier(Modifier::BOLD)
}

pub fn bold_accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn success() -> Style {
    Style::default().fg(SUCCESS)
}

pub fn warn() -> Style {
    Style::default().fg(WARN)
}

pub fn info() -> Style {
    Style::default().fg(FG)
}

pub fn label() -> Style {
    Style::default().fg(FG_DIM).add_modifier(Modifier::BOLD)
}

pub fn value() -> Style {
    Style::default().fg(FG)
}

pub fn muted() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn dim() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn border() -> Style {
    Style::default().fg(BORDER)
}

pub fn panel_title() -> Style {
    Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)
}

pub fn key() -> Style {
    Style::default().fg(FG_BRIGHT).add_modifier(Modifier::BOLD)
}

pub fn key_label() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn checkbox(checked: bool) -> Style {
    if checked {
        Style::default().fg(SUCCESS)
    } else {
        Style::default().fg(FG_DIM)
    }
}

pub fn workflow() -> Style {
    Style::default().fg(FG)
}

pub fn action_ref() -> Style {
    Style::default().fg(FG)
}

pub fn from_ref() -> Style {
    Style::default().fg(FG_DIM)
}

pub fn to_ref() -> Style {
    Style::default().fg(SUCCESS)
}

/// Full-width row highlight (painted behind the table, not per-cell).
pub fn selection_row() -> Style {
    Style::default().bg(SELECTION_BG)
}
