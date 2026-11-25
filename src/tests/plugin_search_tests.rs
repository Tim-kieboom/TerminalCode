//! The search of the plugin manager: `/` opens a box, what is typed limits the list, `tag:`
//! limits it to a tag, and what the editor has built in is said under the box.

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::components::popups::plugin_list::{Mark, PluginList, Row};
use crate::components::prepare_and_render;
use crate::plugins::tags::Tag;

fn row(id: &str, description: &str, tags: &[&str]) -> Row {
    Row {
        id: id.into(),
        name: id.to_owned(),
        version: "1.0.0".to_owned(),
        state: "not installed".to_owned(),
        tags: tags.iter().filter_map(|name| Tag::parse(name)).collect(),
        description: description.to_owned(),
        detail: description.to_owned(),
        mark: Mark::NotInstalled,
        ..Row::default()
    }
}

fn app() -> App {
    let rows = vec![
        row("git", "Shows the status of a repository", &["git"]),
        row("sol-syntax", "Colours Sol files", &["syntax"]),
        row(
            "rust-lsp",
            "A server for Rust",
            &["language-server", "rust"],
        ),
        row("test-pane", "Runs tests", &["test"]),
    ];
    let mut app = App::new(AppState::default());
    app.state_mut()
        .open_plugin_list(PluginList::new(rows, None));
    app
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn type_text(app: &mut App, text: &str) {
    for typed in text.chars() {
        key(app, KeyCode::Char(typed));
    }
}

fn search(app: &mut App, text: &str) {
    key(app, KeyCode::Char('/'));
    type_text(app, text);
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(110, 34)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn list(app: &mut App) -> &mut PluginList {
    app.state_mut().plugin_list_mut().unwrap()
}

fn chosen(app: &mut App) -> String {
    list(app).selected_id().unwrap_or_default().to_owned()
}

fn shown(app: &mut App) -> Vec<String> {
    let screen = screen(app);
    ["git", "sol-syntax", "rust-lsp", "test-pane"]
        .into_iter()
        .filter(|id| screen.contains(&format!(" {id} 1.0.0")))
        .map(str::to_owned)
        .collect()
}

#[test]
fn slash_opens_the_box_and_the_letters_are_typed_not_run_as_keys() {
    let mut app = app();

    key(&mut app, KeyCode::Char('/'));
    assert!(list(&mut app).is_searching());
    type_text(&mut app, "dixk");

    assert!(
        matches!(app.state().popup(), Popup::PluginList(_)),
        "d, i, x and k are text in the box, not commands"
    );
    assert!(screen(&mut app).contains("/ dixk"));
}

#[test]
fn a_word_limits_the_list_as_it_is_typed() {
    let mut app = app();
    search(&mut app, "rust");

    assert_eq!(shown(&mut app), ["rust-lsp"]);
    assert_eq!(chosen(&mut app), "rust-lsp");
}

#[test]
fn a_tag_limits_the_list_and_every_term_has_to_hold() {
    let mut app = app();
    search(&mut app, "tag:syntax");
    assert_eq!(shown(&mut app), ["sol-syntax"]);

    type_text(&mut app, " tag:git");
    assert!(shown(&mut app).is_empty());
    assert!(screen(&mut app).contains("No plugin matches"));
}

#[test]
fn backspace_and_ctrl_u_take_text_back() {
    let mut app = app();
    search(&mut app, "gitx");
    assert!(shown(&mut app).is_empty());

    key(&mut app, KeyCode::Backspace);
    assert_eq!(shown(&mut app), ["git"]);

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )));
    assert_eq!(shown(&mut app).len(), 4);
}

#[test]
fn enter_keeps_the_search_and_gives_the_keys_back_to_the_list() {
    let mut app = app();
    search(&mut app, "tag:git");

    key(&mut app, KeyCode::Enter);

    assert!(!list(&mut app).is_searching());
    assert_eq!(shown(&mut app), ["git"]);
    // `d` is a command again: it opens the plugin's page, which closes the list.
    key(&mut app, KeyCode::Char('d'));
    assert!(!matches!(app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn esc_in_the_box_forgets_the_search() {
    let mut app = app();
    search(&mut app, "rust");

    key(&mut app, KeyCode::Esc);

    assert!(!list(&mut app).is_searching());
    assert!(!list(&mut app).has_query());
    assert_eq!(shown(&mut app).len(), 4);
    assert!(matches!(app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn esc_on_the_list_clears_a_search_before_it_closes_the_list() {
    let mut app = app();
    search(&mut app, "rust");
    key(&mut app, KeyCode::Enter);

    key(&mut app, KeyCode::Esc);
    assert!(matches!(app.state().popup(), Popup::PluginList(_)));
    assert!(!list(&mut app).has_query());

    key(&mut app, KeyCode::Esc);
    assert!(!matches!(app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn a_search_that_finds_nothing_can_still_be_opened_and_cleared() {
    let mut app = app();
    search(&mut app, "nothing like this");
    key(&mut app, KeyCode::Enter);
    assert_eq!(chosen(&mut app), "");

    key(&mut app, KeyCode::Char('/'));
    assert!(
        list(&mut app).is_searching(),
        "`/` works with nothing to choose"
    );
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Esc);

    assert!(list(&mut app).selected_id().is_some());
    assert!(matches!(app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn the_arrows_choose_in_the_limited_list_while_typing() {
    let mut app = app();
    search(&mut app, "t");
    let before = shown(&mut app);
    assert!(before.len() > 1, "{before:?}");
    let first = chosen(&mut app);

    key(&mut app, KeyCode::Down);

    assert_ne!(chosen(&mut app), first);
    assert!(list(&mut app).is_searching());
}

#[test]
fn the_title_says_how_many_of_how_many() {
    let mut app = app();
    assert!(
        screen(&mut app).contains("Plugins 4"),
        "{}",
        screen(&mut app)
    );
    search(&mut app, "tag:git");
    assert!(screen(&mut app).contains("1/4"));
}

#[test]
fn the_tags_to_choose_from_are_listed_while_typing_and_narrowed_by_a_tag_being_typed() {
    let mut app = app();
    key(&mut app, KeyCode::Char('/'));
    let all = screen(&mut app);
    assert!(
        all.contains("tags: syntax git test language-server"),
        "{all}"
    );

    type_text(&mut app, "tag:c");
    let narrowed = screen(&mut app);
    assert!(narrowed.contains("tags: c c++ c# css"), "{narrowed}");
    assert!(!narrowed.contains("syntax git"));
}

#[test]
fn a_tag_that_does_not_exist_is_said_and_the_list_is_empty() {
    let mut app = app();
    search(&mut app, "tag:sytnax");
    key(&mut app, KeyCode::Enter);

    let shown = screen(&mut app);

    assert!(shown.contains("no tag `sytnax`"), "{shown}");
    assert!(shown.contains("No plugin matches"), "{shown}");
}

#[test]
fn a_tag_that_is_still_being_typed_is_not_called_wrong() {
    let mut app = app();
    search(&mut app, "tag:s");

    let typing = screen(&mut app);
    assert!(
        typing.contains("tags: syntax · shell sol sql swift"),
        "{typing}"
    );
    assert!(!typing.contains("no tag"), "{typing}");

    key(&mut app, KeyCode::Enter);
    assert!(
        screen(&mut app).contains("no tag `s`"),
        "once it is left, it is a tag that does not exist"
    );
}

#[test]
fn a_language_the_editor_has_is_said_to_be_built_in() {
    let mut app = app();
    search(&mut app, "rust syntax");

    let shown = screen(&mut app);

    assert!(shown.contains("Rust syntax is built in"), "{shown}");
}

#[test]
fn a_language_that_nothing_has_is_not_available_yet() {
    let mut app = app();
    search(&mut app, "python syntax");

    let shown = screen(&mut app);

    assert!(
        shown.contains("Python syntax is not available yet"),
        "{shown}"
    );
}

#[test]
fn a_language_with_a_plugin_listed_is_not_called_unavailable() {
    let mut app = app();
    // `rust-lsp` is tagged rust, and rust is built in: the note and the row come together.
    search(&mut app, "tag:rust");

    let shown = screen(&mut app);

    assert!(shown.contains("rust-lsp"), "{shown}");
    assert!(shown.contains("Rust syntax is built in"), "{shown}");
    assert!(!shown.contains("not available yet"));
}

#[test]
fn the_list_is_the_same_after_the_rows_are_rebuilt_while_a_search_is_on() {
    let mut app = app();
    search(&mut app, "tag:git");

    let rows = vec![
        row("git", "Shows the status of a repository", &["git"]),
        row("other", "Something", &[]),
    ];
    list(&mut app).set_rows(rows);

    assert_eq!(chosen(&mut app), "git");
    assert!(!screen(&mut app).contains("other 1.0.0"));
}

#[test]
fn a_click_in_the_list_chooses_the_row_under_it_with_the_search_box_open() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut app = app();
    search(&mut app, "t");
    key(&mut app, KeyCode::Enter);
    let shown_text = screen(&mut app);
    let (row, line) = shown_text
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("test-pane 1.0.0"))
        .unwrap();
    let column = line.trim_matches('"').find("test-pane").unwrap() as u16;

    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(chosen(&mut app), "test-pane");
}
