//! The password prompt of a save as root: "Save /etc/hosts as root", a masked
//! password and, after a wrong one, a line saying so. Enter tries the
//! password, Esc gives up. While `sudo` is working the keys do nothing.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::components::popups::key_with_ctrl;
use crate::files::rootsave::Secret;
use crate::ids::DocumentId;
use crate::ui::theme::Theme;

const WIDTH: u16 = 64;
const LABEL: &str = "Password: ";

/// What a key press asks the prompt to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Insert(char),
    Backspace,
    Clear,
    Submit,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        let (code, ctrl) = key_with_ctrl(key)?;
        Some(match (code, ctrl) {
            (KeyCode::Esc, _) => Self::Cancel,
            (KeyCode::Enter, _) => Self::Submit,
            (KeyCode::Backspace, _) => Self::Backspace,
            (KeyCode::Char('u'), true) => Self::Clear,
            (KeyCode::Char(c), false) => Self::Insert(c),
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub(crate) struct RootPrompt {
    /// The file that is to be saved.
    path: String,
    document: DocumentId,
    password: Secret,
    /// Said under the password after a try that did not work.
    error: Option<String>,
    /// `sudo` has the password and has not answered yet.
    waiting: bool,
}

impl RootPrompt {
    pub(crate) fn new(path: &std::path::Path, document: DocumentId) -> Self {
        Self {
            path: path.display().to_string(),
            document,
            password: Secret::default(),
            error: None,
            waiting: false,
        }
    }

    pub(crate) fn document(&self) -> DocumentId {
        self.document
    }

    pub(crate) fn is_waiting(&self) -> bool {
        self.waiting
    }

    pub(crate) fn insert(&mut self, c: char) {
        if !c.is_control() {
            self.password.push(c);
        }
    }

    pub(crate) fn backspace(&mut self) {
        self.password.pop();
    }

    pub(crate) fn clear(&mut self) {
        self.password.clear();
    }

    pub(crate) fn password_is_empty(&self) -> bool {
        self.password.is_empty()
    }

    /// Hands the password over for `sudo` and waits for its answer; the prompt
    /// is left empty.
    pub(crate) fn take_password(&mut self) -> Secret {
        self.waiting = true;
        self.error = None;
        std::mem::take(&mut self.password)
    }

    /// `sudo` did not take the password: says so and asks again.
    pub(crate) fn rejected(&mut self, message: &str) {
        self.waiting = false;
        self.password.clear();
        self.error = Some(message.to_owned());
    }

    #[cfg(test)]
    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn stars(&self) -> usize {
        self.password.chars()
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = WIDTH.min(screen.width);
        let height = 7.min(screen.height);
        if width < 10 || height < 5 {
            return;
        }
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + screen.height.saturating_sub(height) / 3,
            width,
            height,
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Save as root ", theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let stars = "*".repeat(self.password.chars());
        let status = match (&self.error, self.waiting) {
            (_, true) => Line::from("waiting for sudo..."),
            (Some(error), false) => Line::styled(error.clone(), theme.style("notification.error")),
            (None, false) => Line::from(""),
        };
        let lines = vec![
            Line::from(format!("Save {} as root", self.path)),
            Line::from(""),
            Line::from(vec![Span::raw(LABEL), Span::raw(stars)]),
            status,
            Line::from("enter: save | esc: cancel"),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
