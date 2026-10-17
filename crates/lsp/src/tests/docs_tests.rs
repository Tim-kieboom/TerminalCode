use crate::docs::plain_text;

#[test]
fn plain_text_is_left_as_it_is() {
    assert_eq!(
        plain_text("Adds one.\nReturns it."),
        "Adds one.\nReturns it."
    );
}

#[test]
fn code_marks_emphasis_and_headings_are_taken_away() {
    let markdown = "# Examples\n\nUse `push` to **add** an __item__.\n\n## Panics\nNever.";

    assert_eq!(
        plain_text(markdown),
        "Examples\n\nUse push to add an item.\n\nPanics\nNever."
    );
}

#[test]
fn the_lines_that_fence_a_block_of_code_go_and_the_code_stays() {
    let markdown = "Example:\n```rust\nlet v = Vec::new();\n```\nDone.";

    assert_eq!(plain_text(markdown), "Example:\nlet v = Vec::new();\nDone.");
}

#[test]
fn a_link_is_its_text() {
    assert_eq!(
        plain_text("See [the book](https://doc.rust-lang.org) and [`Vec`][vec]."),
        "See the book and Vec."
    );
}

#[test]
fn brackets_that_are_not_a_link_stay() {
    assert_eq!(
        plain_text("A [hash map] of [u8; 4]."),
        "A [hash map] of [u8; 4]."
    );
}

#[test]
fn blank_lines_are_one_and_none_at_the_ends() {
    assert_eq!(plain_text("\n\nA\n\n\n\nB\n\n"), "A\n\nB");
}

#[test]
fn windows_line_ends_and_nothing_at_all_are_fine() {
    assert_eq!(plain_text("A\r\nB"), "A\nB");
    assert_eq!(plain_text(""), "");
    assert_eq!(plain_text("```\n```"), "");
}

#[test]
fn an_unclosed_link_or_mark_does_not_loop() {
    assert_eq!(plain_text("[text](no end"), "[text](no end");
    assert_eq!(plain_text("`open"), "open");
    assert_eq!(plain_text("**"), "");
}

#[test]
fn a_rule_between_parts_is_a_blank_line() {
    assert_eq!(
        plain_text("fn push(&mut self)\n---\nAdds one.\n***\nMore."),
        "fn push(&mut self)\n\nAdds one.\n\nMore."
    );
}
