//! A plugin that is a language: turned on in the plugin manager, the files of its kind are
//! colored by its tree-sitter grammar; turned off, they are plain again. The grammar is the one
//! of the Sol extension for Zed, with its highlight query, as the syntax crate's tests have it.

use std::fs;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;
use tokio::sync::mpsc;

use super::highlight_tests::{KEYWORD, Rig, color_of, draw};
use crate::app::App;
use crate::event::action::Action;
use crate::initial_state;
use crate::persist::saved_state::SavedState;

const GRAMMAR: &[u8] = include_bytes!("../../crates/syntax/src/tests/fixtures/sol.wasm");
const HIGHLIGHTS: &str = include_str!("../../crates/syntax/src/tests/fixtures/sol-highlights.scm");

fn manifest(id: &str, extensions: &str) -> String {
    format!(
        "id = \"{id}\"\nname = \"Sol\"\nversion = \"0.0.1\"\nabi = \"1.12\"\n\n[language]\nname = \"Sol\"\ngrammar = \"sol.wasm\"\nextensions = [{extensions}]\ncomment = \"//\"\nhighlights = '''\n{HIGHLIGHTS}'''\n"
    )
}

/// The editor with `a.<extension>` open and the plugin installed, off. The folder of the config is kept
/// as long as the rig is.
///
/// The languages of plugins are one table for the whole program, so each test has a plugin and
/// extensions of its own (the tests run side by side).
fn rig(id: &str, extension: &str) -> (Rig, tempfile::TempDir, tempfile::TempDir) {
    let project = tempfile::tempdir().unwrap();
    let file = project.path().join(format!("a.{extension}"));
    fs::write(&file, "pub struct P {}\n").unwrap();
    let config = tempfile::tempdir().unwrap();
    let plugin = config.path().join("plugins").join(id);
    fs::create_dir_all(&plugin).unwrap();
    fs::write(
        plugin.join("manifest.toml"),
        manifest(id, &format!("\"{extension}\", \"{extension}c\"")),
    )
    .unwrap();
    fs::write(plugin.join("sol.wasm"), GRAMMAR).unwrap();
    let (tx, events) = mpsc::channel(256);
    let app = App::new(initial_state(&[file]).unwrap())
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(SavedState::default())
        .with_events(tx);
    (Rig { app, events }, project, config)
}

fn press(rig: &mut Rig, code: KeyCode) {
    rig.app
        .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn screen(rig: &mut Rig) -> String {
    let terminal = draw(rig);
    terminal.backend().to_string()
}

#[test]
fn turning_a_language_on_colors_its_files_and_turning_it_off_makes_them_plain_again() {
    let (mut rig, _project, _config) = rig("sol-manager", "sold");
    assert_ne!(
        color_of(&draw(&mut rig), "pub"),
        Some(KEYWORD),
        "plain at first"
    );

    // The manager lists it; turning it on asks what it does, and the answer is yes.
    rig.app.run_action(Action::ShowPlugins);
    press(&mut rig, KeyCode::Enter);
    let question = screen(&mut rig);
    assert!(
        question.contains("color files ending in .sold, .soldc as Sol"),
        "{question}"
    );
    press(&mut rig, KeyCode::Char('y'));
    press(&mut rig, KeyCode::Esc);

    // Its keywords are in the color of the theme's keywords, from its grammar.
    let colored = draw(&mut rig);
    assert_eq!(color_of(&colored, "pub"), Some(KEYWORD));
    assert_eq!(color_of(&colored, "struct"), Some(KEYWORD));

    // Off again.
    rig.app.run_action(Action::ShowPlugins);
    let listed = screen(&mut rig);
    assert!(listed.contains("on · colors .sold, .soldc"), "{listed}");
    // Enter has no pane to open: it says what the plugin is. Space turns it off.
    press(&mut rig, KeyCode::Enter);
    let said = screen(&mut rig);
    // The toast wraps it in two lines.
    assert!(
        said.contains("is a language: it colors") && said.contains("files (space turns it off)"),
        "{said}"
    );
    press(&mut rig, KeyCode::Char(' '));
    press(&mut rig, KeyCode::Esc);
    let plain = draw(&mut rig);
    assert_ne!(color_of(&plain, "pub"), Some(KEYWORD));
    let _: Color = KEYWORD;
}

#[test]
fn a_language_that_was_on_is_on_when_the_editor_starts() {
    let (mut rig, project, config) = rig("sol-boot", "solx");
    let _ = (&mut rig, &project);
    let mut saved = SavedState::default();
    saved.set_plugin(
        "sol-boot",
        crate::persist::saved_state::PluginRecord {
            enabled: true,
            ..Default::default()
        },
    );
    let (tx, events) = mpsc::channel(256);
    let file = project.path().join("a.solx");
    let app = App::new(initial_state(&[file]).unwrap())
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    let mut started = Rig { app, events };

    assert_eq!(color_of(&draw(&mut started), "struct"), Some(KEYWORD));
}

/// The plugin as its own repository ships it (<https://github.com/Tim-kieboom/trcode-plugin-sol-syntax>):
/// ignored unless `TRCODE_PLUGIN_SOL_DIR` names a checkout of it.
#[test]
#[ignore = "needs a checkout of trcode-plugin-sol-syntax (TRCODE_PLUGIN_SOL_DIR)"]
fn the_sol_plugin_as_it_is_published_colors_sol_files() {
    let repo = std::path::PathBuf::from(
        std::env::var_os("TRCODE_PLUGIN_SOL_DIR").expect("set TRCODE_PLUGIN_SOL_DIR"),
    );
    let project = tempfile::tempdir().unwrap();
    let file = project.path().join("a.sol");
    fs::write(&file, "pub fn main() { println(\"hi\", 3); }\n").unwrap();
    let config = tempfile::tempdir().unwrap();
    let plugin = config.path().join("plugins/sol-syntax");
    fs::create_dir_all(&plugin).unwrap();
    fs::copy(repo.join("manifest.toml"), plugin.join("manifest.toml")).unwrap();
    fs::copy(repo.join("sol.wasm"), plugin.join("sol.wasm")).unwrap();
    let mut saved = SavedState::default();
    saved.set_plugin(
        "sol-syntax",
        crate::persist::saved_state::PluginRecord {
            enabled: true,
            ..Default::default()
        },
    );
    let (tx, events) = mpsc::channel(256);
    let app = App::new(initial_state(&[file]).unwrap())
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    let mut rig = Rig { app, events };

    let colored = draw(&mut rig);

    assert_eq!(color_of(&colored, "pub"), Some(KEYWORD));
    assert_eq!(
        color_of(&colored, "println"),
        Some(KEYWORD),
        "a keyword of Sol"
    );
    assert_ne!(
        color_of(&colored, "main"),
        Some(KEYWORD),
        "a call, not a keyword"
    );
}
