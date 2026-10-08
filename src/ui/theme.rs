use std::collections::HashMap;
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use thiserror::Error;

/// Built-in theme, embedded at compile time.
const DEFAULT_THEME_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_theme.toml"
));

/// Named style slots such as `"pane.border"`. Components ask for a slot, never
/// for a concrete color, so a theme file can restyle everything. Unknown slots
/// fall back to the default style, which lets plugins use slots a theme does
/// not know about yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Theme {
    slots: HashMap<Box<str>, Style>,
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("theme is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("slot `{slot}` has an unknown color `{value}`")]
    InvalidColor { slot: Box<str>, value: Box<str> },
}

/// Style of one slot as written in a theme file.
#[derive(Debug, Deserialize)]
struct StyleSpec {
    fg: Option<Box<str>>,
    bg: Option<Box<str>>,
    #[serde(default)]
    modifiers: Vec<ModifierName>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ModifierName {
    Bold,
    Dim,
    Italic,
    Underlined,
    Reversed,
    CrossedOut,
}

impl From<ModifierName> for Modifier {
    fn from(name: ModifierName) -> Self {
        match name {
            ModifierName::Bold => Modifier::BOLD,
            ModifierName::Dim => Modifier::DIM,
            ModifierName::Italic => Modifier::ITALIC,
            ModifierName::Underlined => Modifier::UNDERLINED,
            ModifierName::Reversed => Modifier::REVERSED,
            ModifierName::CrossedOut => Modifier::CROSSED_OUT,
        }
    }
}

impl Theme {
    /// Parses a theme described as TOML. Colors are names such as `"red"` or
    /// `"dark_gray"`, or hex values such as `"#1e1e2e"`.
    pub(crate) fn from_toml(source: &str) -> Result<Self, ThemeError> {
        let specs: HashMap<Box<str>, StyleSpec> = toml::from_str(source)?;
        let mut slots = HashMap::with_capacity(specs.len());
        for (slot, spec) in specs {
            let style = build_style(&slot, &spec)?;
            slots.insert(slot, style);
        }
        Ok(Self { slots })
    }

    pub(crate) fn style(&self, slot: &str) -> Style {
        self.slots.get(slot).copied().unwrap_or_default()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_toml(DEFAULT_THEME_TOML)
            .expect("defaults/default_theme.toml must be a valid theme")
    }
}

fn build_style(slot: &str, spec: &StyleSpec) -> Result<Style, ThemeError> {
    let mut style = Style::default();
    if let Some(fg) = &spec.fg {
        style = style.fg(parse_color(slot, fg)?);
    }

    if let Some(bg) = &spec.bg {
        style = style.bg(parse_color(slot, bg)?);
    }

    for modifier in &spec.modifiers {
        style = style.add_modifier(Modifier::from(*modifier));
    }

    Ok(style)
}

fn parse_color(slot: &str, value: &str) -> Result<Color, ThemeError> {
    Color::from_str(value).map_err(|_| ThemeError::InvalidColor {
        slot: slot.into(),
        value: value.into(),
    })
}
