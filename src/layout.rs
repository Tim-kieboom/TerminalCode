use ratatui::layout::Constraint;

pub const WORKSPACE_HEIGHT: Constraint = Constraint::Min(30);
pub const STATUSBAR_HEIGHT: Constraint = Constraint::Length(2);

pub const EDITOR_WIDTH: Constraint = Constraint::Min(1);
pub const SIDEBAR_WIDTH: Constraint = Constraint::Length(28);

pub mod debug_window {
    pub const MIN_POPUP_WIDTH: u16 = 100;
}

pub mod keybind_display {
    pub const MIN_POPUP_WIDTH: u16 = 56;
}

pub mod editor {
    use ratatui::layout::Constraint;

    pub const TABS_HEIGHT: Constraint = Constraint::Length(3);
    pub const CONTENT_HEIGHT: Constraint = Constraint::Percentage(68);
    pub const BOTTOMBAR_HEIGHT: Constraint = Constraint::Percentage(32);
}

pub mod sidebar {
    use ratatui::layout::Constraint;

    pub const TABS_HEIGHT: Constraint = Constraint::Length(3);
}

pub mod toast {
    use std::time::Duration;

    pub const MAX_TOASTS: usize = 5;
    pub const TOAST_HEIGHT: u16 = 3;
    pub const TOAST_LIFETIME: Duration = Duration::from_secs(4);
}
