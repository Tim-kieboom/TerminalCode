use std::{path::Path, sync::OnceLock};

use ratatui::style::{Color, Modifier, Style};

#[cfg(test)]
#[path = "tests/theme_tests.rs"]
mod tests;

const THEME_DEFAULTS: &str = include_str!("../theme_defaults.json");

struct ThemeConfig {
    accent: Color,

    text: Color,
    text_dim: Color,
    text_bright: Color,

    border: Color,
    border_dim: Color,
    border_focused: Color,

    note: Color,
    danger: Color,
    warning: Color,
    success: Color,

    background_dark: Color,
    background_editor: Color,
    background_sidebar: Color,
    background_terminal: Color,
    background_explorer: Color,
    background_selected: Color,
    background_highlight: Color,
}

fn parse_color(value: &serde_json::Value) -> Option<Color> {
    match value {
        serde_json::Value::Array(arr) if arr.len() == 3 => {
            let r = arr[0].as_u64()? as u8;
            let g = arr[1].as_u64()? as u8;
            let b = arr[2].as_u64()? as u8;
            Some(Color::Rgb(r, g, b))
        }
        serde_json::Value::String(s) => parse_color_string(s),
        _ => None,
    }
}

fn parse_color_string(s: &str) -> Option<Color> {
    match s.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "white" => Some(Color::White),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightblue" => Some(Color::LightBlue),
        "reset" => Some(Color::Reset),
        _ => parse_hex(s.trim_start_matches('#')),
    }
}

fn parse_hex(hex: &str) -> Option<Color> {
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

impl ThemeConfig {
    fn hardcoded_defaults() -> Self {
        Self {
            accent: Color::Rgb(80, 200, 255),

            text: Color::Rgb(220, 220, 220),
            text_dim: Color::Rgb(128, 128, 128),
            text_bright: Color::White,

            border: Color::Rgb(65, 65, 85),
            border_dim: Color::Rgb(45, 45, 60),
            border_focused: Color::Rgb(0, 140, 255),

            note: Color::Rgb(100, 180, 255),
            danger: Color::Rgb(230, 80, 80),
            success: Color::Rgb(80, 220, 120),
            warning: Color::Rgb(230, 180, 60),

            background_dark: Color::Rgb(30, 30, 40),
            background_editor: Color::Rgb(30, 30, 40),
            background_sidebar: Color::Rgb(25, 25, 35),
            background_terminal: Color::Black,
            background_explorer: Color::Rgb(25, 25, 35),
            background_selected: Color::Rgb(45, 50, 75),
            background_highlight: Color::Rgb(40, 40, 55),
        }
    }

    fn load(json: &str) -> Self {
        let mut config = Self::hardcoded_defaults();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
            config.merge(&value);
        }
        config
    }

    fn merge(&mut self, json: &serde_json::Value) {
        let obj = match json.as_object() {
            Some(o) => o,
            None => return,
        };
        for (key, value) in obj {
            let color = match parse_color(value) {
                Some(c) => c,
                None => continue,
            };
            match key.as_str() {
                "accent" => self.accent = color,

                "text" => self.text = color,
                "textDim" => self.text_dim = color,
                "textBright" => self.text_bright = color,

                "border" => self.border = color,
                "borderDim" => self.border_dim = color,
                "borderFocused" => self.border_focused = color,

                "note" => self.note = color,
                "danger" => self.danger = color,
                "success" => self.success = color,
                "warning" => self.warning = color,

                "backgroundDark" => self.background_dark = color,
                "backgroundEditor" => self.background_editor = color,
                "backgroundSidebar" => self.background_sidebar = color,
                "backgroundTerminal" => self.background_terminal = color,
                "backgroundExplorer" => self.background_explorer = color,
                "backgroundSelected" => self.background_selected = color,
                "backgroundHighlight" => self.background_highlight = color,
                _ => {}
            }
        }
    }
}

static THEME: OnceLock<ThemeConfig> = OnceLock::new();

pub struct Theme;
impl Theme {
    pub fn init(config_dir: &Path) {
        let mut config = ThemeConfig::load(THEME_DEFAULTS);
        let user_path = config_dir.join("theme.json");
        if let Ok(content) = std::fs::read_to_string(&user_path) {
            config.merge(&serde_json::from_str(&content).unwrap_or_default());
        }
        _ = THEME.set(config);
    }

    fn config() -> &'static ThemeConfig {
        THEME
            .get()
            .expect("Theme not initialized — call Theme::init() before use")
    }

    pub fn border_default() -> Style {
        Style::new().fg(Self::config().border)
    }

    pub fn border_focused() -> Style {
        Style::new()
            .fg(Self::config().border_focused)
            .add_modifier(Modifier::BOLD)
    }

    pub fn title_default() -> Style {
        Style::new()
            .fg(Self::config().text_dim)
            .add_modifier(Modifier::BOLD)
    }

    pub fn title_focused() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text_normal() -> Style {
        Style::new().fg(Self::config().text)
    }

    pub fn text_dim() -> Style {
        Style::new().fg(Self::config().text_dim)
    }

    pub fn text_accent() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text_success() -> Style {
        Style::new().fg(Self::config().success)
    }

    pub fn text_note() -> Style {
        Style::new().fg(Self::config().note)
    }

    pub fn text_error() -> Style {
        Style::new().fg(Self::config().danger)
    }

    pub fn text_warning() -> Style {
        Style::new().fg(Self::config().warning)
    }

    pub fn add_highlight(style: &mut Style) {
        *style = style.bg(Self::config().background_selected)
    }

    pub fn into_highlight(style: Style) -> Style {
        style.bg(Self::config().background_selected)
    }

    pub fn editor_background() -> Style {
        Style::new().bg(Self::config().background_editor)
    }

    pub fn explorer_bg() -> Style {
        Style::new().bg(Self::config().background_explorer)
    }

    pub fn status_bar() -> Style {
        Style::new()
            .fg(Self::config().text)
            .bg(Self::config().background_terminal)
    }

    pub fn status_bar_key() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .bg(Self::config().background_terminal)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_bar_dim() -> Style {
        Style::new()
            .fg(Self::config().text_dim)
            .bg(Self::config().background_terminal)
    }

    pub fn cursor() -> Style {
        Style::new()
            .bg(Self::config().border_focused)
            .fg(Self::config().text)
    }

    pub fn line_number() -> Style {
        Style::new().fg(Self::config().text_dim)
    }

    pub fn line_number_active() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn explorer_item() -> Style {
        Style::new().fg(Self::config().text)
    }

    pub fn explorer_folder() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn explorer_file() -> Style {
        Style::new().fg(Self::config().text)
    }

    pub fn tab_active() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .bg(Self::config().background_editor)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive() -> Style {
        Style::new()
            .fg(Self::config().text_dim)
            .bg(Self::config().background_editor)
    }

    pub fn popup_border() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn popup_title() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn keybind_action() -> Style {
        Style::new().fg(Self::config().text)
    }

    pub fn keybind_key() -> Style {
        Style::new()
            .fg(Self::config().accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn keybind_dim() -> Style {
        Style::new().fg(Self::config().text_dim)
    }
}
