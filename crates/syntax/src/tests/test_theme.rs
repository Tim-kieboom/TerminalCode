//! A small theme for the tests: `["syntax.name"]` tables with a `text` color, and the
//! same fallback from a specific name to a general one that the editor's themes have.

use std::collections::HashMap;
use std::str::FromStr;

use ratatui::style::{Color, Style};

use crate::CaptureStyles;

#[derive(Debug, Default)]
pub struct TestTheme {
    styles: HashMap<String, Style>,
}

impl TestTheme {
    pub fn from_toml(source: &str) -> Self {
        let table: toml::Table = source.parse().expect("the test theme is valid TOML");
        let styles = table
            .into_iter()
            .filter_map(|(slot, value)| {
                let color = value.get("text")?.as_str()?;
                let color = Color::from_str(color).expect("a color the tests know");
                Some((slot, Style::default().fg(color)))
            })
            .collect();
        Self { styles }
    }

    /// Colors for the most common captures, for tests that only need some theme.
    pub fn ordinary() -> Self {
        Self::from_toml(
            "[\"syntax.keyword\"]\ntext = \"red\"\n[\"syntax.string\"]\ntext = \"green\"\n[\"syntax.function\"]\ntext = \"blue\"\n[\"syntax.type\"]\ntext = \"cyan\"\n[\"syntax.number\"]\ntext = \"yellow\"\n[\"syntax.comment\"]\ntext = \"gray\"\n",
        )
    }
}

impl CaptureStyles for TestTheme {
    fn syntax_style(&self, capture: &str) -> Option<Style> {
        let mut slot = format!("syntax.{capture}");
        loop {
            if let Some(style) = self.styles.get(slot.as_str()) {
                return Some(*style);
            }
            let dot = slot.rfind('.')?;
            slot.truncate(dot);
        }
    }
}
