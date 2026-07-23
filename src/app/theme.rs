use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

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

    // ----- Selection / cursor -----
    pub const SELECTED_BG: Color = Color::Rgb(50, 55, 75);

    // -----  Composite styles -----

    pub fn border_default() -> Style {
        Style::default().fg(Self::BORDER)
    }

    pub fn border_focused() -> Style {
        Style::default()
            .fg(Self::BORDER_FOCUSED)
            .add_modifier(Modifier::BOLD)
    }

    pub fn title_default() -> Style {
        Style::default()
            .fg(Self::TEXT_DIM)
            .add_modifier(Modifier::BOLD)
    }

    pub fn title_focused() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text_normal() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn text_dim() -> Style {
        Style::default().fg(Self::TEXT_DIM)
    }

    pub fn text_accent() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn text_success() -> Style {
        Style::default().fg(Self::SUCCESS)
    }

    pub fn text_warning() -> Style {
        Style::default().fg(Self::WARNING)
    }

    pub fn status_bar() -> Style {
        Style::default()
            .fg(Self::TEXT)
            .bg(Self::BACKGROUND_DARK)
    }

    pub fn status_bar_key() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .bg(Self::BACKGROUND_DARK)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_bar_dim() -> Style {
        Style::default()
            .fg(Self::TEXT_DIM)
            .bg(Self::BACKGROUND_DARK)
    }

    pub fn line_number() -> Style {
        Style::default().fg(Self::TEXT_DIM)
    }

    pub fn line_number_active() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn explorer_item() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn explorer_folder() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn explorer_file() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn explorer_selected() -> Style {
        Style::default()
            .fg(Self::TEXT_BRIGHT)
            .bg(Self::SELECTED_BG)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_active() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .bg(Self::BACKGROUND_DARK)
            .add_modifier(Modifier::BOLD)
    }

    pub fn tab_inactive() -> Style {
        Style::default()
            .fg(Self::TEXT_DIM)
            .bg(Self::BACKGROUND_DARK)
    }

    pub fn popup_border() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn popup_title() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn keybind_action() -> Style {
        Style::default().fg(Self::TEXT)
    }

    pub fn keybind_key() -> Style {
        Style::default()
            .fg(Self::ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    pub fn keybind_dim() -> Style {
        Style::default().fg(Self::TEXT_DIM)
    }
}
