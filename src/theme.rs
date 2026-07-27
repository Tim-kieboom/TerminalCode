use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

#[allow(unused)]
impl Theme {
    // ----- Accent ----- 
    pub const ACCENT: Color = Color::Cyan;
    pub const ACCENT_BOLD: Color = Color::Cyan;

    // ----- Text ----- 
    pub const TEXT: Color = Color::White;
    pub const TEXT_DIM: Color = Color::Rgb(128, 128, 128);
    pub const TEXT_BRIGHT: Color = Color::White;

    // ----- Background ----- 
    pub const BACKGROUND_DARK: Color = Color::Rgb(30, 30, 40);
    pub const BACKGROUND_SIDEBAR: Color = Color::Rgb(25, 25, 35);
    pub const BACKGROUND_HIGHLIGHT: Color = Color::Rgb(40, 40, 55);

    // ----- Borders -----
    pub const BORDER: Color = Color::Rgb(60, 60, 80);
    pub const BORDER_FOCUSED: Color = Color::Cyan;
    pub const BORDER_DIM: Color = Color::Rgb(45, 45, 60);

    // ----- Status -----
    pub const SUCCESS: Color = Color::Green;
    pub const WARNING: Color = Color::Yellow;
    pub const DANGER: Color = Color::Red;
    pub const NOTE: Color = Color::LightBlue;

    // ----- Selection / cursor -----
    pub const SELECTED_BG: Color = Color::Rgb(50, 55, 75);

    // -----  Composite styles -----

    pub const fn border_default() -> Style {
        Style::new().fg(Self::BORDER)
    }

    pub const fn border_focused() -> Style {
        Style::new()
            .fg(Self::BORDER_FOCUSED)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn title_default() -> Style {
        Style::new()
            .fg(Self::TEXT_DIM)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn title_focused() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn text_normal() -> Style {
        Style::new().fg(Self::TEXT)
    }

    pub const fn text_dim() -> Style {
        Style::new().fg(Self::TEXT_DIM)
    }

    pub const fn text_accent() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn text_success() -> Style {
        Style::new().fg(Self::SUCCESS)
    }

    pub const fn text_note() -> Style {
        Style::new().fg(Self::NOTE)
    }

    pub const fn text_error() -> Style {
        Style::new().fg(Self::DANGER)
    }

    pub const fn text_warning() -> Style {
        Style::new().fg(Self::WARNING)
    }

    pub const fn add_highlight(style: &mut Style) {
        *style = style
            .bg(Self::SELECTED_BG)
    }

    pub const fn status_bar() -> Style {
        Style::new()
            .fg(Self::TEXT)
            .bg(Self::BACKGROUND_DARK)
    }

    pub const fn status_bar_key() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .bg(Self::BACKGROUND_DARK)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn status_bar_dim() -> Style {
        Style::new()
            .fg(Self::TEXT_DIM)
            .bg(Self::BACKGROUND_DARK)
    }

    pub const fn line_number() -> Style {
        Style::new().fg(Self::TEXT_DIM)
    }

    pub const fn line_number_active() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn explorer_item() -> Style {
        Style::new().fg(Self::TEXT)
    }

    pub const fn explorer_folder() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn explorer_file() -> Style {
        Style::new().fg(Self::TEXT)
    }

    pub const fn explorer_selected() -> Style {
        Style::new()
            .fg(Self::TEXT_BRIGHT)
            .bg(Self::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn tab_active() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .bg(Self::BACKGROUND_DARK)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn tab_inactive() -> Style {
        Style::new()
            .fg(Self::TEXT_DIM)
            .bg(Self::BACKGROUND_DARK)
    }

    pub const fn popup_border() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn popup_title() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn keybind_action() -> Style {
        Style::new().fg(Self::TEXT)
    }

    pub const fn keybind_key() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn keybind_dim() -> Style {
        Style::new().fg(Self::TEXT_DIM)
    }

    pub const fn keybind_selected() -> Style {
        Style::new()
            .fg(Self::TEXT_BRIGHT)
            .bg(Self::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    }

    pub const fn keybind_action_selected() -> Style {
        Style::new()
            .fg(Self::TEXT)
            .bg(Self::SELECTED_BG)
    }

    pub const fn keybind_key_selected() -> Style {
        Style::new()
            .fg(Self::ACCENT)
            .bg(Self::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    }
}
