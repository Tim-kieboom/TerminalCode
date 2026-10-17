use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::config::ConfigFile;
use crate::event::action::Action;
use crate::keymap::DEFAULT_KEYMAP_TOML;
use crate::ui::layout::DEFAULT_LAYOUT_RON;
use crate::ui::theme::DEFAULT_THEME_TOML;

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

/// Runs the palette entry named by `query`.
fn run_from_palette(app: &mut App, query: &str) {
    press(app, KeyCode::F(1));
    for c in query.chars() {
        press(app, KeyCode::Char(c));
    }
    press(app, KeyCode::Enter);
}

fn app_with_config_dir(dir: &Path) -> App {
    App::default().with_config_dir(Some(dir.to_path_buf()))
}

fn editor_text(app: &App) -> String {
    app.state().editor().buffer().text().to_string()
}

#[derive(serde::Deserialize)]
struct Wrapper {
    action: Action,
}

use crate::app::state::Focus;
use crate::ui::ComponentKind;
use crate::ui::theme::{Rgb, Theme, ThemeVariant};
use ratatui::style::Color;

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

fn reload(app: &mut App) {
    run_from_palette(app, "Reload");
}

fn use_theme(app: &mut App, query: &str) {
    run_from_palette(app, query);
}

use crate::event::Event;
use crate::files::paths::canonical;
use crate::files::watcher::FsWatcher;

fn saved(app: &mut App, path: &Path) {
    app.handle_event(Event::FilesChanged(vec![path.to_path_buf()]));
}

fn install(dir: &Path, name: &str, text: &str) {
    fs::create_dir_all(dir.join("themes")).unwrap();
    fs::write(dir.join("themes").join(format!("{name}.toml")), text).unwrap();
}

/// Installs a copy of each built-in theme, standing in for themes the user made from the templates.
fn install_built_in_copies(dir: &Path) {
    for variant in ThemeVariant::ALL {
        install(dir, variant.name(), variant.source());
    }
}

fn app_with_built_in_copies() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    (app_with_config_dir(dir.path()), dir)
}

const GREEN_ACCENT: &str = "[palette]\naccent = \"#00ff00\"\n";

fn accent(app: &App) -> Option<Color> {
    app.state().theme().style("pane.border.focused").fg
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn enter(app: &mut App) {
    press(app, KeyCode::Enter);
}

/// "Theme: New..." from the palette, then the template whose title contains
/// `template`, then `name` and enter.
fn new_theme(app: &mut App, template: &str, name: &str) {
    run_from_palette(app, "Theme: New...");
    type_str(app, template);
    enter(app);
    type_str(app, name);
    enter(app);
}

/// A config folder that can never be made: its parent is a file.
fn app_with_unwritable_config_dir() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    fs::write(&blocker, "a file, not a folder").unwrap();
    let app = app_with_config_dir(&blocker.join("config"));
    (app, dir)
}

fn screen_of(app: &mut App) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn latest(app: &App) -> String {
    app.state()
        .latest_notification()
        .unwrap_or_default()
        .to_owned()
}

mod basics;
mod built_in_themes;
mod installed_themes;
mod new_theme;
mod open_theme_in_use;
mod open_theme_makes_a_file;
mod overlay_keeps_the_theme;
mod read_only_config_folder;
mod reload;
mod reload_on_save;
mod sol_themes;
