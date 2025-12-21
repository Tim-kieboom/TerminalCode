use anyhow::{Error, Result};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span, Text},
};
use std::path::Path;
use syntect::{
    easy::HighlightLines,
    highlighting::ThemeSet,
    parsing::{SyntaxReference, SyntaxSet},
};

const DEFAULT_THEME: &str = "base16-mocha.dark";

/// Syntax highlighting engine for the IDE.
///
/// Uses `syntect` for accurate language detection (by file extension) and
/// theme-based highlighting. Converts `syntect::Style` → `ratatui::Style`
/// for terminal rendering.
#[derive(Debug)]
pub struct Syntaxer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    syntax: Option<SyntaxReference>,
    theme_name: String,
}
impl Default for Syntaxer {
    fn default() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            syntax: None,
            theme_name: "base16-eighties.dark".to_string(),
        }
    }
}
impl Syntaxer {
    /// Updates syntax rules based on file extension.
    ///
    /// Falls back to "Plain Text" if no syntax found for extension.
    pub fn update_syntax(&mut self, path: &Path) {
        const FALL_BACK: &str = "Plain Text";
        let extention = path.extension().and_then(|el| el.to_str()).unwrap_or("");

        self.syntax = self
            .syntax_set
            .find_syntax_by_extension(extention)
            .or_else(|| self.syntax_set.find_syntax_by_name(FALL_BACK))
            .cloned();
    }

    /// Converts plain text to syntax-highlighted `ratatui::Text`.
    pub fn highlight_text<'a>(&mut self, text: &'a str) -> Result<Text<'a>> {
        let syntax = match &self.syntax {
            Some(val) => val,
            None => return Ok(Text::from(text)),
        };

        let theme = match self.theme_set.themes.get(&self.theme_name) {
            Some(val) => val,
            None => {
                self.theme_name = DEFAULT_THEME.to_string();
                return Err(Error::msg(format!(
                    "theme: {} was not found",
                    self.theme_name
                )));
            }
        };

        let mut result = vec![];
        let mut parser = HighlightLines::new(syntax, theme);
        for line in text.lines() {
            let mut spans = vec![];
            for (style, styled_text) in parser.highlight_line(line, &self.syntax_set)? {
                let color = to_color(style);
                spans.push(Span::styled(styled_text, Style::new().fg(color)));
            }
            result.push(Line::from(spans));
        }

        Ok(Text::from(result))
    }
}

fn to_color(style: syntect::highlighting::Style) -> ratatui::style::Color {
    translate_color(style.foreground).unwrap_or(Color::Reset)
}

fn translate_color(syntect_color: syntect::highlighting::Color) -> Option<ratatui::style::Color> {
    match syntect_color {
        syntect::highlighting::Color { r, g, b, a } if a > 0 => {
            Some(ratatui::style::Color::Rgb(r, g, b))
        }
        _ => None,
    }
}
