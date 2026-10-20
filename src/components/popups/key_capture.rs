//! Asks for the keys that open a plugin: the next key pressed is the one. It is told
//! when the key cannot be used (a letter on its own, a key that is bound already) and asks again.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::ui::theme::Theme;

const WIDTH: u16 = 60;
const HELP: &str = "press the key | esc: cancel";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyCapture {
    /// The plugin the key is for.
    id: Box<str>,
    name: String,
    /// Why the last key was refused.
    problem: Option<String>,
}

impl KeyCapture {
    pub(crate) fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.to_owned(),
            problem: None,
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn refuse(&mut self, why: String) {
        self.problem = Some(why);
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = WIDTH.min(screen.width);
        let mut lines = vec![
            Line::from(format!("Press the key that opens {}.", self.name)),
            Line::styled(
                "With ctrl, alt or super, or a function key.",
                theme.style("explorer.hint"),
            ),
        ];
        if let Some(problem) = &self.problem {
            lines.push(Line::default());
            lines.push(Line::styled(problem.as_str(), theme.style("git.modified")));
        }
        let height = (lines.len() as u16 + 4).min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(
                " Key for a plugin ",
                theme.style("pane.title"),
            ));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);
        let body = Rect {
            height: inner.height.saturating_sub(1),
            ..inner
        };
        let help = Rect {
            y: inner.y + body.height,
            height: inner.height - body.height,
            ..inner
        };
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
        frame.render_widget(
            Paragraph::new(Line::styled(HELP, theme.style("tab.inactive"))),
            help,
        );
    }
}
