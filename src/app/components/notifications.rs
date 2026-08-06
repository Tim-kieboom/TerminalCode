use std::{time::Instant, vec};

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    app::components::debug_window::DebugTag,
    layout::toast::{MAX_TOASTS, TOAST_HEIGHT, TOAST_LIFETIME},
    theme::Theme,
};

pub struct Notifications {
    toasts: Vec<Toast>,
}

struct Toast {
    tag: DebugTag,
    message: String,
    added_at: Instant,
}

impl Notifications {
    pub fn new() -> Self {
        Self { toasts: vec![] }
    }

    pub fn push(&mut self, tag: DebugTag, message: String) {
        self.toasts.push(Toast {
            tag,
            message,
            added_at: Instant::now(),
        });
        if self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        self.toasts
            .retain(|toast| now.duration_since(toast.added_at) < TOAST_LIFETIME);
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        if self.toasts.is_empty() {
            return;
        }

        let mut bottom = area.y + area.height.saturating_sub(TOAST_HEIGHT);

        for toast in self.toasts.iter().rev() {
            let width = toast.chars_len().min(area.width.saturating_sub(2));

            let rect = Rect::new(
                area.x + area.width.saturating_sub(width),
                bottom,
                width,
                TOAST_HEIGHT,
            );

            let (title, title_style, text_style) = match toast.tag {
                DebugTag::Note => (" Note ", Theme::text_note(), Theme::text_note()),
                DebugTag::Error => (" Error ", Theme::text_error(), Theme::text_error()),
                DebugTag::Warning => (" Warning ", Theme::text_warning(), Theme::text_warning()),
            };

            let max_len = width.saturating_sub(2) as usize;
            let message: String = toast.message.chars().take(max_len).collect();

            let block = Block::default()
                .title(Span::styled(title, title_style))
                .borders(Borders::ALL)
                .border_style(Theme::popup_border());

            let paragraph = Paragraph::new(Line::styled(message, text_style)).block(block);

            frame.render_widget(Clear, rect);
            frame.render_widget(paragraph, rect);

            bottom = rect.y.saturating_sub(TOAST_HEIGHT + 1);
        }
    }
}
impl Toast {
    pub fn chars_len(&self) -> u16 {
        self.message.chars().count() as u16 + 4
    }
}
