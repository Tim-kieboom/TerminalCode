use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    App,
    app::components::Component,
    keybinds::Action,
    layout::{EDITOR_WIDTH, SIDEBAR_WIDTH, STATUSBAR_HEIGHT, WORKSPACE_HEIGHT},
    theme::Theme,
    utils::{horizontal_layout, vertical_layout},
};

impl App {
    pub(super) fn draw(&mut self, frame: &mut Frame) {
        self.notifications.update();

        let area = frame.area();
        let layout = vertical_layout([WORKSPACE_HEIGHT, STATUSBAR_HEIGHT], area);

        self.draw_workspace(frame, layout[0]);
        self.draw_status_bar(frame, layout[1]);
        self.debug_window.try_draw(frame, area, self.context);
        self.keybind_display.try_draw(frame, area, self.context);
        self.notifications.draw(frame, area);
    }

    fn draw_status_bar(&self, frame: &mut Frame, area: Rect) {
        let panel = self.context.description();

        let quit_key = self.key_label(Action::Quit);
        let keybinds_key = self.key_label(Action::ShowKeyBinds);

        let status = Line::from(vec![
            Span::styled(format!(" {panel} "), Theme::status_bar_key()),
            Span::styled(" │ ", Theme::status_bar_dim()),
            Span::styled(quit_key.clone(), Theme::status_bar_key()),
            Span::styled(" Quit ", Theme::status_bar_dim()),
            Span::styled("│ ", Theme::status_bar_dim()),
            Span::styled(keybinds_key, Theme::status_bar_key()),
            Span::styled(" Keybinds ", Theme::status_bar_dim()),
        ]);

        let bar = Paragraph::new(vec![Line::from(""), status]).style(Theme::status_bar());
        frame.render_widget(bar, area);
    }

    fn draw_workspace(&mut self, frame: &mut Frame, area: Rect) {
        if self.sidebar.should_hide() {
            self.editor.draw(frame, area, self.context);
            return;
        }

        let layout = horizontal_layout([SIDEBAR_WIDTH, EDITOR_WIDTH], area);
        self.sidebar.try_draw(frame, layout[0], self.context);
        self.editor.draw(frame, layout[1], self.context);
    }
}
