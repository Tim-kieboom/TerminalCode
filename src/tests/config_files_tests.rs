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

#[test]
fn the_palette_lists_the_three_config_files() {
    let actions = Action::palette_actions();

    for file in [ConfigFile::Theme, ConfigFile::Keymap, ConfigFile::Layout] {
        assert!(actions.contains(&Action::OpenConfig(file)), "{file:?}");
        assert!(Action::OpenConfig(file).title().is_some());
    }
}

#[test]
fn with_no_theme_chosen_open_theme_makes_nothing_and_says_how_to_make_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("Theme: New"), "{message}");
    assert!(!dir.path().join("themes").exists());
    assert!(!dir.path().join("theme.toml").exists());
    assert!(!app.state().workspace().has_tabs() || app.state().editor().buffer().path().is_none());
}

#[test]
fn each_file_starts_from_its_own_default() {
    let cases = [
        ("Open Keymap", "keymap.toml", DEFAULT_KEYMAP_TOML),
        ("Open Layout", "layout.ron", DEFAULT_LAYOUT_RON),
    ];

    for (query, name, default) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_config_dir(dir.path());

        run_from_palette(&mut app, query);

        assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap(), default);
    }
}

#[test]
fn the_theme_file_in_use_is_opened_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "mine", "[palette]\naccent = \"red\"\n");
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: mine");

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(editor_text(&app), "[palette]\naccent = \"red\"\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("themes/mine.toml")).unwrap(),
        "[palette]\naccent = \"red\"\n"
    );
}

#[test]
fn the_config_directory_is_made_when_it_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("a").join("terminalcode");
    let mut app = app_with_config_dir(&config);

    run_from_palette(&mut app, "Open Layout");

    assert!(config.join("layout.ron").is_file());
}

#[test]
fn opening_an_existing_file_twice_shows_the_same_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Layout");
    let after_first = app.state().workspace().tab_names().0.len();

    run_from_palette(&mut app, "Open Layout");

    assert_eq!(app.state().workspace().tab_names().0.len(), after_first);
}

#[test]
fn without_a_config_directory_it_says_so_and_makes_nothing() {
    let mut app = App::default();

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("no config directory"), "{message}");
}

#[test]
fn a_directory_that_cannot_be_made_is_reported_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the config directory should be.
    let blocker = dir.path().join("terminalcode");
    fs::write(&blocker, "").unwrap();
    let mut app = app_with_config_dir(&blocker);

    run_from_palette(&mut app, "Open Layout");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("terminalcode"), "{message}");
}

#[test]
fn an_action_can_be_bound_in_a_keymap_file() {
    let action: Action = toml::from_str::<Wrapper>("action = { open_config = \"theme\" }")
        .unwrap()
        .action;

    assert_eq!(action, Action::OpenConfig(ConfigFile::Theme));
}

#[derive(serde::Deserialize)]
struct Wrapper {
    action: Action,
}

// ---- reload

use crate::app::state::Focus;
use crate::components::ComponentKind;
use crate::ui::theme::{Rgb, Theme, ThemeVariant};
use ratatui::style::Color;

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

fn reload(app: &mut App) {
    run_from_palette(app, "Reload");
}

#[test]
fn the_palette_lists_reload() {
    assert!(Action::palette_actions().contains(&Action::Reload));
    assert!(Action::Reload.title().is_some());
}

#[test]
fn reload_applies_a_changed_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let before = app.state().theme().style("pane.border.focused");
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    reload(&mut app);

    let after = app.state().theme().style("pane.border.focused");
    assert_ne!(after, before);
    assert_eq!(after.fg, Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn reload_applies_a_changed_layout() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    assert!(app.state().layout().contains(&ComponentKind::Explorer));
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn reload_applies_a_changed_keymap() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "keymap.toml",
        "[[binding]]\nkeys = \"f2\"\naction = \"quit\"\n",
    );
    press(&mut app, KeyCode::F(2));
    assert!(!app.should_quit(), "not bound before the reload");

    reload(&mut app);
    press(&mut app, KeyCode::F(2));

    assert!(app.should_quit());
}

#[test]
fn reload_says_what_it_did() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    reload(&mut app);

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("reloaded"), "{message}");
}

#[test]
fn a_removed_file_goes_back_to_the_default_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let default = app.state().theme().clone();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    reload(&mut app);
    assert_ne!(*app.state().theme(), default);

    fs::remove_file(dir.path().join("theme.toml")).unwrap();
    reload(&mut app);

    assert_eq!(*app.state().theme(), default);
}

#[test]
fn an_invalid_file_is_reported_and_the_other_files_are_still_applied() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "theme.toml", "[\"a\"]\ntext = \"@nope\"\n");
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("theme.toml"), "{message}");
    assert_eq!(*app.state().theme(), Theme::default());
    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn reload_keeps_the_terminal_background_learned_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let mut theme = Theme::default();
    theme.set_terminal_background(Some(Rgb { r: 1, g: 2, b: 3 }));
    app.state_mut().set_theme(theme);

    reload(&mut app);

    assert_eq!(
        app.state().theme().terminal_background(),
        Some(Rgb { r: 1, g: 2, b: 3 })
    );
}

#[test]
fn the_keyboard_leaves_a_component_the_new_layout_lacks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    app.state_mut().set_focus(Focus::Explorer);
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn reload_leaves_open_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    press(&mut app, KeyCode::Char('x'));
    let text = editor_text(&app);

    reload(&mut app);

    assert_eq!(editor_text(&app), text);
}

#[test]
fn reload_with_a_syntax_worker_running_starts_a_new_one_without_trouble() {
    let dir = tempfile::tempdir().unwrap();
    let (events, _results) = tokio::sync::mpsc::channel(16);
    let mut app = app_with_config_dir(dir.path()).with_events(events);
    write(dir.path(), "theme.toml", "[palette]\nteal = \"#00ff00\"\n");

    reload(&mut app);

    assert!(app.state_mut().refresh_highlights().is_empty());
    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("reloaded"), "{message}");
}

// ---- built-in themes

fn use_theme(app: &mut App, query: &str) {
    run_from_palette(app, query);
}

#[test]
fn the_palette_lists_no_built_in_theme_to_choose_only_templates() {
    let actions = Action::palette_actions();
    assert!(actions.contains(&Action::ChooseTheme));
    assert!(actions.contains(&Action::NewTheme));
    assert!(actions.iter().filter_map(Action::title).all(|title| {
        ![
            "Theme: Dark",
            "Theme: Light",
            "Theme: SolDark",
            "Theme: SolLight",
        ]
        .contains(&title)
    }));
}

#[test]
fn choosing_the_light_theme_restyles_the_editor() {
    let (mut app, _dir) = app_with_built_in_copies();
    let before = app.state().theme().syntax_style("keyword");

    use_theme(&mut app, "Theme: light");

    let after = app.state().theme().syntax_style("keyword");
    assert_ne!(after, before);
    assert_eq!(
        after,
        Theme::built_in(ThemeVariant::Light).syntax_style("keyword")
    );
}

#[test]
fn choosing_the_dark_theme_goes_back() {
    let (mut app, _dir) = app_with_built_in_copies();
    use_theme(&mut app, "Theme: light");

    use_theme(&mut app, "Theme: dark");

    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn a_theme_change_says_which_theme_it_is() {
    let (mut app, _dir) = app_with_built_in_copies();
    use_theme(&mut app, "Theme: light");
    assert!(
        app.state()
            .notifications()
            .texts()
            .iter()
            .any(|text| text.starts_with("theme: light"))
    );
}

#[test]
fn the_user_theme_file_is_laid_over_the_chosen_theme() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    let mut app = app_with_config_dir(dir.path());

    use_theme(&mut app, "Theme: light");

    let focused = app.state().theme().style("pane.border.focused");
    assert_eq!(focused.fg, Some(Color::Rgb(0, 255, 0)));
    // Everything the file leaves alone is the light theme's.
    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::built_in(ThemeVariant::Light).style("status.bar")
    );
}

#[test]
fn a_broken_user_theme_leaves_the_chosen_built_in_theme_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(dir.path(), "theme.toml", "[palette\n");
    let mut app = app_with_config_dir(dir.path());

    use_theme(&mut app, "Theme: light");

    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::built_in(ThemeVariant::Light).style("status.bar")
    );
    assert!(app.state().notifications().has_errors());
}

#[test]
fn reload_keeps_the_chosen_theme() {
    let (mut app, _dir) = app_with_built_in_copies();
    use_theme(&mut app, "Theme: light");

    reload(&mut app);

    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::built_in(ThemeVariant::Light).style("status.bar")
    );
}

#[test]
fn a_theme_change_keeps_the_terminal_background() {
    let (mut app, _dir) = app_with_built_in_copies();
    let mut theme = Theme::default();
    theme.set_terminal_background(Some(Rgb { r: 1, g: 2, b: 3 }));
    app.state_mut().set_theme(theme);

    use_theme(&mut app, "Theme: light");

    assert_eq!(
        app.state().theme().terminal_background(),
        Some(Rgb { r: 1, g: 2, b: 3 })
    );
}

// ---- reloading when a config file is saved

use crate::event::Event;
use crate::files::paths::canonical;
use crate::files::watcher::FsWatcher;

fn saved(app: &mut App, path: &Path) {
    app.handle_event(Event::FilesChanged(vec![path.to_path_buf()]));
}

#[test]
fn saving_the_theme_file_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    saved(&mut app, &dir.path().join("theme.toml"));

    let focused = app.state().theme().style("pane.border.focused");
    assert_eq!(focused.fg, Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn saving_the_keymap_file_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "keymap.toml",
        "[[binding]]\nkeys = \"ctrl+t\"\naction = \"new_file\"\n",
    );

    saved(&mut app, &dir.path().join("keymap.toml"));
    let tabs = app.state().workspace().tab_names().0.len();
    app.handle_input(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('t'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    ));

    assert_eq!(app.state().workspace().tab_names().0.len(), tabs + 1);
}

#[test]
fn saving_the_layout_file_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    saved(&mut app, &dir.path().join("layout.ron"));

    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn another_file_in_the_config_folder_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "notes.txt", "x");
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    saved(&mut app, &dir.path().join("notes.txt"));

    assert_ne!(
        app.state().theme().style("pane.border.focused").fg,
        Some(Color::Rgb(0, 255, 0))
    );
}

#[test]
fn a_file_with_the_name_of_a_config_file_elsewhere_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    write(elsewhere.path(), "theme.toml", "x");

    saved(&mut app, &elsewhere.path().join("theme.toml"));

    assert_ne!(
        app.state().theme().style("pane.border.focused").fg,
        Some(Color::Rgb(0, 255, 0))
    );
}

#[test]
fn a_config_file_saved_with_a_mistake_is_reported_and_the_built_in_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "theme.toml", "[palette\n");

    saved(&mut app, &dir.path().join("theme.toml"));

    assert!(app.state().notifications().has_errors());
    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn without_a_config_folder_a_saved_file_is_just_a_changed_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::default();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    saved(&mut app, &dir.path().join("theme.toml"));

    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn the_config_folder_is_watched_when_it_exists() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let app = App::default()
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_watcher(FsWatcher::new(tx).unwrap());

    assert!(app.watched_directories().contains(&canonical(dir.path())));
}

#[test]
fn a_config_folder_that_does_not_exist_is_not_watched() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("not-there");
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let app = App::default()
        .with_config_dir(Some(missing.clone()))
        .with_watcher(FsWatcher::new(tx).unwrap());

    assert!(
        !app.watched_directories()
            .iter()
            .any(|watched| watched.ends_with("not-there"))
    );
}

#[test]
fn a_config_file_that_was_only_read_is_not_reloaded() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    let mut app = app_with_config_dir(dir.path());

    // The watcher also reports a file when it is read, and a reload reads it.
    saved(&mut app, &dir.path().join("theme.toml"));

    assert!(
        !app.state()
            .notifications()
            .texts()
            .iter()
            .any(|text| text.contains("reloaded")),
        "nothing was saved, so nothing is reloaded"
    );
}

#[test]
fn reloading_does_not_cause_another_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    saved(&mut app, &dir.path().join("theme.toml"));
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("reloaded")
    );
    app.state_mut().dismiss_errors();
    app.expire_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));

    saved(&mut app, &dir.path().join("theme.toml"));

    assert_eq!(app.state().latest_notification(), None);
}

// ---- installed themes

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

#[test]
fn installed_themes_are_listed_by_name_without_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    install(dir.path(), "mono", GREEN_ACCENT);
    fs::write(dir.path().join("themes/readme.txt"), "x").unwrap();
    fs::create_dir(dir.path().join("themes/folder.toml")).unwrap();

    assert_eq!(
        crate::config::installed_themes(dir.path()),
        ["mono", "solar"]
    );
}

#[test]
fn no_themes_folder_means_no_installed_themes() {
    let dir = tempfile::tempdir().unwrap();
    assert!(crate::config::installed_themes(dir.path()).is_empty());
    assert!(crate::config::installed_themes(&dir.path().join("missing")).is_empty());
}

#[test]
fn a_theme_name_cannot_reach_outside_the_themes_folder() {
    let dir = Path::new("/cfg");
    assert!(crate::config::installed_theme_path(dir, "../keymap").is_none());
    assert!(crate::config::installed_theme_path(dir, "a/b").is_none());
    assert!(crate::config::installed_theme_path(dir, "..").is_none());
    assert!(crate::config::installed_theme_path(dir, "").is_none());
    assert_eq!(
        crate::config::installed_theme_path(dir, "solar").unwrap(),
        Path::new("/cfg/themes/solar.toml")
    );
}

#[test]
fn the_palette_lists_each_installed_theme() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "Theme: solar");

    let titles: Vec<_> = app
        .state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| entry.title.clone())
        .collect();
    // The best matches; looser ones (the letters of "solar" are also in "SolDark") follow.
    assert_eq!(titles[..2], ["Theme: solar", "Theme: New from solar"]);
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

#[test]
fn choosing_an_installed_theme_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("theme: solar")
    );
}

#[test]
fn an_installed_theme_may_leave_slots_out() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::default().style("status.bar")
    );
}

#[test]
fn the_user_theme_file_is_laid_over_the_installed_theme() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    write(dir.path(), "theme.toml", "[palette]\ntext = \"#010203\"\n");
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
    assert_eq!(
        app.state().theme().style("status.bar").fg,
        Some(Color::Rgb(1, 2, 3))
    );
}

#[test]
fn a_user_theme_file_wins_over_the_installed_theme_where_both_say_something() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#0000ff\"\n",
    );
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 0, 255)));
}

#[test]
fn a_broken_installed_theme_is_reported_and_left_out() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "bad", "[palette\n");
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: bad");

    assert!(app.state().notifications().has_errors());
    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn choosing_another_installed_theme_replaces_the_first() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    install(dir.path(), "light", crate::ui::theme::LIGHT_THEME_TOML);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_reload_keeps_the_installed_theme() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");

    reload(&mut app);

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn saving_the_installed_theme_applies_the_change() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");
    install(dir.path(), "solar", "[palette]\naccent = \"#0000ff\"\n");

    saved(&mut app, &dir.path().join("themes/solar.toml"));

    assert_eq!(accent(&app), Some(Color::Rgb(0, 0, 255)));
}

#[test]
fn a_theme_that_was_removed_cannot_be_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let mut keymap = crate::keymap::Keymap::defaults(crate::terminal::KeyboardSupport::Enhanced);
    keymap
        .apply_toml("[[binding]]\nkeys = \"f9\"\naction = { use_installed_theme = \"ghost\" }")
        .unwrap();
    let mut app = App::with_keymap(crate::app::state::AppState::default(), keymap)
        .with_config_dir(Some(dir.path().to_path_buf()));

    press(&mut app, KeyCode::F(9));

    assert!(app.state().notifications().has_errors());
}

#[test]
fn theme_choose_opens_the_palette_on_the_installed_themes_only() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: Choose");

    let palette = app.state().palette().unwrap();
    assert_eq!(palette.query(), "Theme: ");
    let titles: Vec<_> = palette.visible().map(|entry| entry.title.clone()).collect();
    assert!(titles.contains(&"Theme: solar".to_owned()));
    for template in ["dark", "light", "soldark", "sollight"] {
        assert!(
            !titles.contains(&format!("Theme: {template}")),
            "{template}"
        );
    }
}

#[test]
fn the_themes_folder_is_watched_so_a_theme_dropped_in_shows_up_on_reopening() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let app = App::default()
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_watcher(FsWatcher::new(tx).unwrap());

    assert!(
        app.watched_directories()
            .contains(&canonical(&dir.path().join("themes")))
    );
}

// ---- Config: Open Theme opens the theme in use

#[test]
fn open_theme_opens_the_installed_theme_that_was_chosen() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(editor_text(&app), GREEN_ACCENT);
    assert!(
        !dir.path().join("theme.toml").exists(),
        "no overlay was made"
    );
}

#[test]
fn an_installed_theme_whose_file_vanished_is_reported_and_nothing_is_made() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");
    fs::remove_file(dir.path().join("themes/solar.toml")).unwrap();

    run_from_palette(&mut app, "Open Theme");

    assert!(app.state().notifications().has_errors());
    assert!(!dir.path().join("themes/solar.toml").exists());
}

// ---- an overlay must not undo the choice of theme

#[test]
fn switching_themes_works_with_an_overlay_in_the_config_folder() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(dir.path(), "theme.toml", "# nothing in it\n");
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn an_overlay_that_is_a_copy_of_the_dark_theme_does_not_pin_it() {
    // What older versions of Open Theme wrote.
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(dir.path(), "theme.toml", DEFAULT_THEME_TOML);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_copy_of_the_dark_theme_does_not_hide_an_installed_theme_either() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    write(dir.path(), "theme.toml", DEFAULT_THEME_TOML);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn an_overlay_that_changes_something_is_still_applied_over_the_light_theme() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(
        dir.path(),
        "theme.toml",
        &format!("{DEFAULT_THEME_TOML}\n# edited\n[\"my.slot\"]\ntext = \"red\"\n"),
    );
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    // A file that differs from the built-in one is the user's, and applies in full.
    assert_eq!(app.state().theme().style("my.slot").fg, Some(Color::Red));
    assert_ne!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

// ---- Open Theme makes themes/NAME.toml and chooses it

#[test]
fn a_theme_made_from_a_template_is_chosen_so_its_edits_show() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    new_theme(&mut app, "Dark", "mine");
    write(
        dir.path(),
        "themes/mine.toml",
        &format!("{DEFAULT_THEME_TOML}\n[\"my.slot\"]\ntext = \"red\"\n"),
    );

    saved(&mut app, &dir.path().join("themes/mine.toml"));

    assert_eq!(app.state().theme().style("my.slot").fg, Some(Color::Red));
}

#[test]
fn opening_the_theme_twice_shows_one_tab() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");
    run_from_palette(&mut app, "Open Theme");
    let tabs = app.state().workspace().tab_names().0.len();

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(app.state().workspace().tab_names().0.len(), tabs);
}

// ---- Theme: New...

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

#[test]
fn theme_new_lists_the_built_in_themes_and_the_installed_ones_as_templates() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: New...");

    let palette = app.state().palette().unwrap();
    assert_eq!(palette.query(), "Theme: New from ");
    let titles: Vec<_> = palette.visible().map(|entry| entry.title.clone()).collect();
    assert_eq!(
        titles,
        [
            "Theme: New from Dark",
            "Theme: New from Light",
            "Theme: New from SolDark",
            "Theme: New from SolLight",
            "Theme: New from solar"
        ]
    );
}

#[test]
fn copies_of_the_built_in_themes_are_not_offered_twice() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "dark", GREEN_ACCENT);
    install(dir.path(), "Light", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: New...");

    let count = app.state().palette().unwrap().visible().count();
    assert_eq!(count, 4);
}

#[test]
fn choosing_a_template_asks_for_a_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: New...");
    type_str(&mut app, "Light");
    enter(&mut app);

    let prompt = app.state().name_prompt().unwrap();
    assert_eq!(prompt.name(), "");
}

#[test]
fn a_name_makes_the_theme_from_the_template_chooses_it_and_opens_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "Light", "paper");

    let path = dir.path().join("themes/paper.toml");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        crate::ui::theme::LIGHT_THEME_TOML
    );
    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
    assert_eq!(app.state().editor().buffer().path(), Some(path.as_path()));
    assert!(app.state().name_prompt().is_none());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("themes/paper.toml")
    );
}

#[test]
fn the_new_theme_is_the_one_in_use_so_saving_it_applies_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    new_theme(&mut app, "Dark", "mine");
    write(
        dir.path(),
        "themes/mine.toml",
        &format!("{DEFAULT_THEME_TOML}\n[\"my.slot\"]\ntext = \"red\"\n"),
    );

    saved(&mut app, &dir.path().join("themes/mine.toml"));

    assert_eq!(app.state().theme().style("my.slot").fg, Some(Color::Red));
}

#[test]
fn an_installed_theme_can_be_the_template() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "solar", "solar2");

    assert_eq!(
        fs::read_to_string(dir.path().join("themes/solar2.toml")).unwrap(),
        GREEN_ACCENT
    );
    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn a_name_may_end_in_toml() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "Dark", "mine.toml");

    assert!(dir.path().join("themes/mine.toml").is_file());
    assert!(!dir.path().join("themes/mine.toml.toml").exists());
}

#[test]
fn a_name_that_is_taken_is_refused_and_the_prompt_stays() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "mine", "# old\n");
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "Dark", "mine");

    assert_eq!(
        fs::read_to_string(dir.path().join("themes/mine.toml")).unwrap(),
        "# old\n"
    );
    assert!(app.state().name_prompt().is_some());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("already exists")
    );
}

#[test]
fn bad_names_are_refused() {
    for name in ["", "  ", "a/b", "..", ".hidden", "../x"] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_config_dir(dir.path());

        new_theme(&mut app, "Dark", name);

        assert!(app.state().name_prompt().is_some(), "{name:?}");
        assert!(
            crate::config::installed_themes(dir.path()).is_empty(),
            "{name:?}"
        );
    }
}

#[test]
fn escape_at_the_name_makes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: New...");
    type_str(&mut app, "Dark");
    enter(&mut app);
    type_str(&mut app, "mine");

    press(&mut app, KeyCode::Esc);

    assert!(app.state().name_prompt().is_none());
    assert!(!dir.path().join("themes").exists());
}

#[test]
fn the_new_theme_shows_up_in_the_theme_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    new_theme(&mut app, "Dark", "mine");

    assert_eq!(crate::config::installed_themes(dir.path()), ["mine"]);
}

#[test]
fn without_a_config_folder_there_is_no_name_to_ask_for() {
    let mut app = App::default();

    run_from_palette(&mut app, "New from Dark");

    assert!(app.state().name_prompt().is_none());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_template_that_vanished_makes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: New...");
    type_str(&mut app, "solar");
    enter(&mut app);
    fs::remove_file(dir.path().join("themes/solar.toml")).unwrap();
    type_str(&mut app, "copy");

    enter(&mut app);

    assert!(!dir.path().join("themes/copy.toml").exists());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_palette_offers_theme_new() {
    assert!(Action::palette_actions().contains(&Action::NewTheme));
    assert!(Action::NewTheme.title().is_some());
}

// ---- the Sol themes

#[test]
fn the_sol_themes_are_built_in_templates() {
    for variant in [ThemeVariant::SolDark, ThemeVariant::SolLight] {
        assert!(Theme::from_toml(variant.source()).is_ok(), "{variant:?}");
        assert!(crate::config::theme_templates(None).contains(&variant.title().to_owned()));
    }
}

#[test]
fn choosing_a_sol_theme_applies_its_interface_colors() {
    for (query, variant) in [
        ("Theme: soldark", ThemeVariant::SolDark),
        ("Theme: sollight", ThemeVariant::SolLight),
    ] {
        let (mut app, _dir) = app_with_built_in_copies();

        run_from_palette(&mut app, query);

        assert_eq!(*app.state().theme(), Theme::built_in(variant), "{query}");
    }
}

#[test]
fn soldark_has_the_given_colors_and_the_default_syntax_colors() {
    let sol = Theme::built_in(ThemeVariant::SolDark);
    let dark = Theme::default();

    assert_eq!(
        sol.style("pane.border.focused").fg,
        Some(Color::Rgb(0xd1, 0xc8, 0x8f))
    );
    assert_eq!(
        sol.style("status.bar").bg,
        Some(Color::Rgb(0x1e, 0x1e, 0x2e))
    );
    assert_eq!(
        sol.style("explorer.selected").bg,
        Some(Color::Rgb(0x28, 0x2e, 0x1e))
    );
    for capture in [
        "keyword", "string", "function", "type", "number", "comment", "variable", "operator",
    ] {
        assert_eq!(
            sol.syntax_style(capture),
            dark.syntax_style(capture),
            "{capture}"
        );
    }
}

#[test]
fn sollight_has_the_light_syntax_colors() {
    let sol = Theme::built_in(ThemeVariant::SolLight);
    let light = Theme::built_in(ThemeVariant::Light);

    for capture in ["keyword", "string", "function", "type", "number", "comment"] {
        assert_eq!(
            sol.syntax_style(capture),
            light.syntax_style(capture),
            "{capture}"
        );
    }
    assert_ne!(sol.style("status.bar"), light.style("status.bar"));
}

#[test]
fn the_sol_themes_have_the_slots_of_the_dark_theme() {
    let dark = DEFAULT_THEME_TOML.parse::<toml::Table>().unwrap();
    for variant in [ThemeVariant::SolDark, ThemeVariant::SolLight] {
        let sol = variant.source().parse::<toml::Table>().unwrap();
        let names =
            |t: &toml::Table| -> std::collections::BTreeSet<String> { t.keys().cloned().collect() };
        assert_eq!(names(&sol), names(&dark), "{variant:?}");
    }
}

#[test]
fn a_new_theme_can_start_from_a_sol_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "SolDark", "mine");

    assert_eq!(
        fs::read_to_string(dir.path().join("themes/mine.toml")).unwrap(),
        crate::ui::theme::SOL_DARK_THEME_TOML
    );
    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::SolDark));
}

#[test]
fn a_theme_file_that_is_an_unchanged_copy_of_a_sol_theme_does_not_pin_it() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(
        dir.path(),
        "theme.toml",
        crate::ui::theme::SOL_DARK_THEME_TOML,
    );
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

// ---- a config folder that cannot be written

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

#[test]
fn a_settings_file_that_cannot_be_made_opens_the_defaults_in_a_locked_tab() {
    let (mut app, dir) = app_with_unwritable_config_dir();

    run_from_palette(&mut app, "Open Settings");

    assert_eq!(editor_text(&app), crate::settings::DEFAULT_SETTINGS_JSONC);
    assert_eq!(app.state().editor().buffer().path(), None);
    assert!(app.state().notifications().has_errors());
    assert!(latest(&app).contains("could not make"), "{}", latest(&app));
    assert!(
        latest(&app).contains("cannot be edited or saved"),
        "{}",
        latest(&app)
    );
    let screen = screen_of(&mut app);
    assert!(
        screen.contains("settings.json (defaults) [read-only]"),
        "{screen}"
    );
    assert!(!dir.path().join("blocker/config").exists());
}

#[test]
fn the_defaults_copy_cannot_be_edited_and_cannot_be_saved() {
    let (mut app, _dir) = app_with_unwritable_config_dir();
    run_from_palette(&mut app, "Open Settings");

    for c in "xyz".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(editor_text(&app), crate::settings::DEFAULT_SETTINGS_JSONC);
    assert!(!app.state().editor().buffer().is_dirty());

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("copy of the defaults"),
        "{}",
        latest(&app)
    );
}

#[test]
fn the_defaults_copy_can_still_be_moved_around_selected_and_copied() {
    let (mut app, _dir) = app_with_unwritable_config_dir();
    run_from_palette(&mut app, "Open Settings");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    // Dismisses the error that is on screen, then runs the commands.
    press(&mut app, KeyCode::Esc);

    for c in ['a', 'c'] {
        app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    assert!(
        !app.state().notifications().has_errors(),
        "{}",
        latest(&app)
    );
}
