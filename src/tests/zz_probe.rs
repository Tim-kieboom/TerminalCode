use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::state::AppState;
use crate::ui::theme::Theme;

#[test]
fn probe_bytes_for_exact_background() {
    let mut state = AppState::default();
    state.set_theme(
        Theme::from_toml("[background]\ncolor = \"#1e1e2e\"\nopacity = 0.3\nblend = \"none\"\n").unwrap(),
    );
    let mut out: Vec<u8> = Vec::new();
    {
        let backend = CrosstermBackend::new(&mut out);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.resize(ratatui::layout::Rect::new(0, 0, 60, 12)).unwrap();
        terminal
            .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
            .unwrap();
    }
    let text = String::from_utf8_lossy(&out);
    let bg_sets = text.matches("48;2;30;30;46").count();
    let other_bg = text.matches("\x1b[48;").count() - bg_sets;
    println!("bytes: {}, exact-bg sets: {bg_sets}, other 48 sets: {other_bg}", out.len());
    println!("bg resets (49m): {}", text.matches("\x1b[49m").count());
    println!("full reset (0m): {}", text.matches("\x1b[0m").count());
    println!("first 300: {:?}", &text.chars().take(300).collect::<String>());
}
