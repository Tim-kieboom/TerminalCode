//! What the highlighter needs from the theme: a style for each capture of a
//! highlight query.

use ratatui::style::Style;

/// Gives the style of a capture name such as `keyword.control`, or `None` to leave
/// the text plain. A theme usually falls back from the specific name to the general
/// one (`keyword.control`, then `keyword`).
pub trait CaptureStyles {
    fn syntax_style(&self, capture: &str) -> Option<Style>;
}
