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
    background: Background,
    /// The terminal's own background color, when it was asked for; a tinted
    /// background blends with it.
    terminal_background: Option<Rgb>,
}

/// A 24-bit color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
}

impl Rgb {
    /// `self` over `behind` at `alpha` out of 255 (255 is all `self`).
    pub(super) fn over(self, behind: Rgb, alpha: u8) -> Rgb {
        let mix = |front: u8, back: u8| {
            let (alpha, front, back) = (u32::from(alpha), u32::from(front), u32::from(back));
            ((front * alpha + back * (255 - alpha) + 127) / 255) as u8
        };
        Rgb {
            r: mix(self.r, behind.r),
            g: mix(self.g, behind.g),
            b: mix(self.b, behind.b),
        }
    }
}

impl From<Rgb> for Color {
    fn from(rgb: Rgb) -> Self {
        Color::Rgb(rgb.r, rgb.g, rgb.b)
    }
}

/// What the whole screen is painted with before components draw on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Background {
    /// Nothing: the terminal's own background (and its opacity or blur) shows.
    Transparent,
    Solid(Color),
    /// `color` at `alpha` out of 255 over the terminal's background.
    Tint {
        color: Rgb,
        alpha: u8,
    },
    /// `color` exactly as written. `alpha` is only a hint for terminals that can
    /// make one specific color translucent (kitty's `transparent_background_colors`).
    Exact {
        color: Rgb,
        alpha: u8,
    },
}

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("theme is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("slot `{slot}` has an unknown color `{value}`")]
    InvalidColor { slot: Box<str>, value: Box<str> },
    #[error("background: `{0}` is not an opacity between 0 and 1")]
    InvalidOpacity(f64),
    #[error("background: opacity needs a color")]
    OpacityWithoutColor,
    #[error("background: unknown color `{0}`")]
    InvalidBackgroundColor(Box<str>),
    #[error("background: an opacity below 1 needs a hex color such as \"#1e1e2e\", not `{0}`")]
    OpacityNeedsHex(Box<str>),
}

/// A theme file: the reserved `[background]` table plus one table per slot.
#[derive(Debug, Deserialize)]
struct ThemeFile {
    background: Option<BackgroundSpec>,
    #[serde(flatten)]
    slots: HashMap<Box<str>, StyleSpec>,
}

/// The `[background]` table. `color` is a color or `"transparent"`; `opacity`
/// runs from 0 (nothing painted) to 1 (the color as is).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackgroundSpec {
    color: Option<Box<str>>,
    opacity: Option<Number>,
    blend: Option<Blend>,
}

/// What `opacity` below 1 does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Blend {
    /// Mix the color with the terminal's own background and paint the result.
    /// The cells end up solid.
    Terminal,
    /// Paint the color unchanged, for terminals that make an exact color
    /// translucent on their own.
    None,
}

/// TOML writes `1` and `1.0` differently; both are fine for an opacity.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    fn as_f64(self) -> f64 {
        match self {
            Self::Int(value) => value as f64,
            Self::Float(value) => value,
        }
    }
}

/// Style of one slot as written in a theme file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleSpec {
    /// Color of the characters. `fg` is the old name and still works.
    #[serde(alias = "fg")]
    text: Option<Box<str>>,
    /// Color behind the characters. `bg` is the old name and still works.
    #[serde(alias = "bg")]
    background: Option<Box<str>>,
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
        let file: ThemeFile = toml::from_str(source)?;
        let mut slots = HashMap::with_capacity(file.slots.len());
        for (slot, spec) in file.slots {
            let style = build_style(&slot, &spec)?;
            slots.insert(slot, style);
        }
        Ok(Self {
            slots,
            background: build_background(file.background)?,
            terminal_background: None,
        })
    }

    /// Whether painting the background needs the terminal's own background
    /// color (see [`Theme::set_terminal_background`]).
    pub(crate) fn needs_terminal_background(&self) -> bool {
        matches!(self.background, Background::Tint { .. })
    }

    /// A line to show at startup when the theme relies on the terminal to make
    /// its background color translucent, naming the setting to use.
    pub(crate) fn terminal_hint(&self) -> Option<String> {
        let Background::Exact { color, alpha } = self.background else {
            return None;
        };
        Some(format!(
            "see-through background: add `transparent_background_colors #{:02x}{:02x}{:02x}@{:.2}` to kitty.conf (kitty 0.39+)",
            color.r,
            color.g,
            color.b,
            f64::from(alpha) / 255.0,
        ))
    }

    pub(crate) fn set_terminal_background(&mut self, color: Option<Rgb>) {
        self.terminal_background = color;
    }

    /// The color to paint the whole screen with, or `None` to leave the
    /// terminal's background alone. A tint blends with the terminal's
    /// background; when that is unknown the tint color is used as is.
    pub(crate) fn background_color(&self) -> Option<Color> {
        match self.background {
            Background::Transparent => None,
            Background::Solid(color) => Some(color),
            Background::Exact { color, .. } => Some(color.into()),
            Background::Tint { color, alpha } => Some(match self.terminal_background {
                Some(behind) => color.over(behind, alpha).into(),
                None => color.into(),
            }),
        }
    }

    /// Whether the theme defines `slot`.
    pub(crate) fn contains(&self, slot: &str) -> bool {
        self.slots.contains_key(slot)
    }

    pub(crate) fn style(&self, slot: &str) -> Style {
        self.slots.get(slot).copied().unwrap_or_default()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_toml(DEFAULT_THEME_TOML).unwrap_or_else(|err| {
            panic!("while tyring to parse defaults/default_theme.toml: {}", err)
        })
    }
}

fn build_background(spec: Option<BackgroundSpec>) -> Result<Background, ThemeError> {
    let Some(spec) = spec else {
        return Ok(Background::Transparent);
    };
    let opacity = spec.opacity.map_or(1.0, Number::as_f64);
    if !(0.0..=1.0).contains(&opacity) {
        return Err(ThemeError::InvalidOpacity(opacity));
    }

    let Some(color) = spec.color else {
        return match spec.opacity {
            Some(_) => Err(ThemeError::OpacityWithoutColor),
            None => Ok(Background::Transparent),
        };
    };
    if matches!(color.to_ascii_lowercase().as_str(), "transparent" | "reset") || opacity == 0.0 {
        return Ok(Background::Transparent);
    }
    let Ok(parsed) = Color::from_str(&color) else {
        return Err(ThemeError::InvalidBackgroundColor(color));
    };
    if opacity == 1.0 {
        return Ok(Background::Solid(parsed));
    }
    let Color::Rgb(r, g, b) = parsed else {
        return Err(ThemeError::OpacityNeedsHex(color));
    };
    let alpha = (opacity * 255.0).round() as u8;
    let color = Rgb { r, g, b };
    Ok(match spec.blend.unwrap_or(Blend::Terminal) {
        Blend::Terminal => Background::Tint { color, alpha },
        Blend::None => Background::Exact { color, alpha },
    })
}

fn build_style(slot: &str, spec: &StyleSpec) -> Result<Style, ThemeError> {
    let mut style = Style::default();
    if let Some(text) = &spec.text {
        style = style.fg(parse_color(slot, text)?);
    }

    if let Some(background) = &spec.background {
        style = style.bg(parse_color(slot, background)?);
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
