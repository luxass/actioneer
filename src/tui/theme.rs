use ratatui::style::{Color, Modifier, Style};

/// Default body text.
const FG: Color = Color::Rgb(205, 210, 218);
/// Secondary / de-emphasized copy.
const FG_DIM: Color = Color::Rgb(115, 120, 132);
/// Emphasis without shouting.
const FG_BRIGHT: Color = Color::Rgb(235, 238, 242);

/// Product name — soft violet, distinct from column accents.
const BRAND: Color = Color::Rgb(175, 155, 225);
/// Subcommands, links, info chrome.
const ACCENT: Color = Color::Rgb(110, 185, 215);
/// Footer key bindings.
const KEY: Color = Color::Rgb(155, 195, 245);

/// Table: workflow file names (path-like).
const WORKFLOW: Color = Color::Rgb(120, 195, 205);
/// Table: action references (package-like).
const ACTION: Color = Color::Rgb(215, 185, 125);
/// Table: current pin — muted, receding.
const FROM: Color = Color::Rgb(130, 135, 148);
/// Table: target pin — positive forward motion.
const TO: Color = Color::Rgb(125, 195, 145);

const SUCCESS: Color = Color::Rgb(125, 195, 145);
const WARN: Color = Color::Rgb(220, 175, 105);
const ERROR: Color = Color::Rgb(225, 125, 125);

const BORDER: Color = Color::Rgb(65, 70, 82);
/// Full-width selected row band.
const SELECTION_BG: Color = Color::Rgb(48, 52, 68);

pub fn bold_brand() -> Style {
    Style::default()
        .fg(BRAND)
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

pub fn error() -> Style {
    Style::default().fg(ERROR)
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
    Style::default().fg(KEY).add_modifier(Modifier::BOLD)
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
    Style::default().fg(WORKFLOW)
}

pub fn action_ref() -> Style {
    Style::default().fg(ACTION)
}

pub fn from_ref() -> Style {
    Style::default().fg(FROM)
}

pub fn to_ref() -> Style {
    Style::default().fg(TO)
}

pub fn column_workflow() -> Style {
    Style::default().fg(WORKFLOW).add_modifier(Modifier::BOLD)
}

pub fn column_action() -> Style {
    Style::default().fg(ACTION).add_modifier(Modifier::BOLD)
}

pub fn column_from() -> Style {
    Style::default().fg(FROM).add_modifier(Modifier::BOLD)
}

pub fn column_to() -> Style {
    Style::default().fg(TO).add_modifier(Modifier::BOLD)
}

/// Full-width row highlight (painted behind the table, not per-cell).
pub fn selection_row() -> Style {
    Style::default().bg(SELECTION_BG)
}
