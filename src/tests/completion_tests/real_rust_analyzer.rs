use super::*;

/// `Vec::ne` shows `new` and `Tab` writes it. Run by hand, since CI has no
/// `rust-analyzer`:
/// `cargo test real_rust_analyzer_offers_new -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_offers_new_after_vec_colons_ne_and_tab_writes_it() {
    let mut rig = Rig::real("fn main() {\n    let v: Vec<u32> = Vec::");

    rig.type_until_offered("ne", "new");
    eprintln!("offered: {:?}", rig.shown());
    eprintln!("{}", rig.screen());
    rig.highlight("new");
    rig.press(KeyCode::Tab);

    assert_eq!(rig.text(), "fn main() {\n    let v: Vec<u32> = Vec::new()");
    assert!(rig.app.state().completion_box().is_none());
}

/// `HashMap` + `Tab` adds the `use` line, and one undo takes both away. Run by
/// hand: `cargo test real_rust_analyzer_adds_the_use -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_adds_the_use_line_for_hashmap_and_one_undo_takes_both_away() {
    let source = "fn main() {\n    let m: Hash";
    let mut rig = Rig::real(source);

    rig.type_until_offered("M", "HashMap");
    eprintln!("offered: {:?}", rig.shown());
    eprintln!("{}", rig.screen());
    rig.highlight("HashMap");
    rig.press(KeyCode::Tab);

    let written = rig.text();
    eprintln!("written:\n{written}");
    assert!(
        written.starts_with("use std::collections::HashMap;\n"),
        "{written}"
    );
    assert!(written.ends_with("let m: HashMap<>"), "{written}");

    rig.app.state_mut().edit(|editor| editor.undo().unwrap());

    assert_eq!(rig.text(), format!("{source}M"));
}

/// A method call gets its brackets with the argument selected, and the
/// documentation of the highlighted method shows. Run by hand:
/// `cargo test real_rust_analyzer_writes_the_brackets -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_writes_the_brackets_of_a_call_and_shows_its_documentation() {
    let source = "fn main() {\n\n\n\n\n    let mut v: Vec<u32> = Vec::new();\n    v.pu";
    let mut rig = Rig::real(source);

    rig.type_until_offered("s", "push");
    rig.highlight("push");
    let screen = rig.screen();
    eprintln!("{screen}");
    assert!(
        screen.contains("Appends an element"),
        "the documentation of push shows: {screen}"
    );
    rig.press(KeyCode::Tab);

    let text = rig.text();
    // rust-analyzer adds the `;` itself when the call is a statement.
    assert!(text.ends_with("v.push(value);"), "{text}");
    let selection = *rig.app.state().editor().selections().primary();
    assert_eq!(
        (selection.anchor().column, selection.head().column),
        ("    v.push(".len(), "    v.push(value".len())
    );
    rig.press(KeyCode::Char('7'));
    assert!(rig.text().ends_with("v.push(7);"), "{}", rig.text());
}
