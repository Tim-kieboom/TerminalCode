use serde_json::json;

use crate::components::workspace::DocumentId;
use crate::lsp::diagnostics::{map_range, parse_published};
use crate::lsp::position::LspPosition;
use crate::lsp::{ByteChange, Diagnostic, Diagnostics, Severity};

fn change(start: usize, old_end: usize, new_end: usize) -> ByteChange {
    ByteChange {
        start,
        old_end,
        new_end,
    }
}

fn diagnostic(start: usize, end: usize, severity: Severity) -> Diagnostic {
    Diagnostic {
        range: start..end,
        severity,
        message: format!("{start}..{end}"),
        source: None,
        code: None,
    }
}

fn doc(number: u32) -> DocumentId {
    DocumentId::for_test(number)
}

// ---- reading what a server published

#[test]
fn a_published_message_is_read_with_its_version_and_every_field_of_each_diagnostic() {
    let params = json!({
        "uri": "file:///p/a.rs",
        "version": 7,
        "diagnostics": [{
            "range": { "start": { "line": 1, "character": 4 }, "end": { "line": 1, "character": 9 } },
            "severity": 2,
            "code": "unused_variables",
            "source": "rustc",
            "message": "unused variable: `value`",
        }],
    });

    let published = parse_published(&params).unwrap();

    assert_eq!(published.uri, "file:///p/a.rs");
    assert_eq!(published.version, Some(7));
    let first = &published.diagnostics[0];
    assert_eq!(
        first.start,
        LspPosition {
            line: 1,
            character: 4
        }
    );
    assert_eq!(
        first.end,
        LspPosition {
            line: 1,
            character: 9
        }
    );
    assert_eq!(first.severity, Severity::Warning);
    assert_eq!(first.code.as_deref(), Some("unused_variables"));
    assert_eq!(first.source.as_deref(), Some("rustc"));
    assert_eq!(first.message, "unused variable: `value`");
}

#[test]
fn the_severities_have_their_numbers_and_a_missing_one_is_an_error() {
    let with = |severity: serde_json::Value| {
        let mut diagnostic = json!({
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
            "message": "m",
        });
        if !severity.is_null() {
            diagnostic["severity"] = severity;
        }
        parse_published(&json!({ "uri": "u", "diagnostics": [diagnostic] }))
            .unwrap()
            .diagnostics[0]
            .severity
    };

    assert_eq!(with(json!(1)), Severity::Error);
    assert_eq!(with(json!(2)), Severity::Warning);
    assert_eq!(with(json!(3)), Severity::Information);
    assert_eq!(with(json!(4)), Severity::Hint);
    assert_eq!(with(json!(null)), Severity::Error);
    assert_eq!(with(json!(99)), Severity::Error);
}

#[test]
fn a_code_may_be_a_number() {
    let params = json!({ "uri": "u", "diagnostics": [{
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
        "message": "m", "code": 308,
    }] });

    assert_eq!(
        parse_published(&params).unwrap().diagnostics[0]
            .code
            .as_deref(),
        Some("308")
    );
}

#[test]
fn no_version_means_the_latest_text() {
    let params = json!({ "uri": "u", "diagnostics": [] });

    assert_eq!(parse_published(&params).unwrap().version, None);
}

#[test]
fn a_diagnostic_that_is_not_usable_is_left_out_and_the_rest_stay() {
    let good = json!({
        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } },
        "message": "ok",
    });
    let params = json!({ "uri": "u", "diagnostics": [
        { "message": "no range" },
        { "range": { "start": { "line": 0, "character": 0 } }, "message": "no end" },
        { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 1 } } },
        { "range": { "start": { "line": -1, "character": 0 }, "end": { "line": 0, "character": 1 } }, "message": "negative" },
        good,
    ] });

    let published = parse_published(&params).unwrap();

    assert_eq!(published.diagnostics.len(), 1);
    assert_eq!(published.diagnostics[0].message, "ok");
}

#[test]
fn params_that_are_not_a_publication_give_nothing() {
    for params in [
        json!(null),
        json!({}),
        json!({ "uri": "u" }),
        json!({ "diagnostics": [] }),
        json!({ "uri": 3, "diagnostics": [] }),
    ] {
        assert_eq!(parse_published(&params), None, "{params}");
    }
}

// ---- moving a range through a change

#[test]
fn an_edit_after_the_range_leaves_it_alone() {
    assert_eq!(map_range(10..15, &change(20, 20, 25)), Some(10..15));
    assert_eq!(map_range(10..15, &change(20, 22, 21)), Some(10..15));
}

#[test]
fn an_edit_that_starts_where_the_range_ends_leaves_it_alone() {
    assert_eq!(
        map_range(10..15, &change(15, 15, 18)),
        Some(10..15),
        "typing right after it"
    );
    assert_eq!(
        map_range(10..15, &change(15, 17, 15)),
        Some(10..15),
        "deleting right after it"
    );
}

#[test]
fn an_edit_before_the_range_moves_it_by_the_difference_in_length() {
    assert_eq!(map_range(10..15, &change(2, 2, 5)), Some(13..18));
    assert_eq!(map_range(10..15, &change(2, 6, 4)), Some(8..13));
    assert_eq!(
        map_range(10..15, &change(0, 10, 0)),
        Some(0..5),
        "deleting right up to it"
    );
}

#[test]
fn typing_right_before_the_range_pushes_it_along() {
    assert_eq!(map_range(10..15, &change(10, 10, 13)), Some(13..18));
}

#[test]
fn text_typed_inside_the_range_makes_it_longer() {
    assert_eq!(map_range(10..15, &change(12, 12, 14)), Some(10..17));
}

#[test]
fn an_edit_that_deletes_or_replaces_inside_the_range_drops_it() {
    assert_eq!(
        map_range(10..15, &change(12, 13, 12)),
        None,
        "deleting inside"
    );
    assert_eq!(
        map_range(10..15, &change(11, 13, 15)),
        None,
        "replacing inside"
    );
    assert_eq!(
        map_range(10..15, &change(10, 15, 10)),
        None,
        "deleting all of it"
    );
    assert_eq!(
        map_range(10..15, &change(8, 12, 8)),
        None,
        "deleting its start"
    );
    assert_eq!(
        map_range(10..15, &change(13, 18, 13)),
        None,
        "deleting its end"
    );
    assert_eq!(
        map_range(10..15, &change(5, 20, 5)),
        None,
        "deleting it and more"
    );
    assert_eq!(map_range(10..15, &change(12, 13, 20)), None);
}

#[test]
fn a_point_stays_before_text_typed_at_it_and_moves_for_an_edit_before_it() {
    assert_eq!(
        map_range(10..10, &change(10, 10, 14)),
        Some(10..10),
        "typed at the point"
    );
    assert_eq!(map_range(10..10, &change(5, 5, 7)), Some(12..12));
    assert_eq!(map_range(10..10, &change(12, 12, 20)), Some(10..10));
}

#[test]
fn a_point_inside_deleted_text_is_dropped() {
    assert_eq!(map_range(10..10, &change(8, 12, 8)), None);
}

#[test]
fn a_change_that_replaces_the_text_with_the_same_length_drops_what_it_covers() {
    // One letter typed over another.
    assert_eq!(map_range(10..15, &change(11, 12, 12)), None);
    assert_eq!(map_range(10..15, &change(15, 16, 16)), Some(10..15));
    assert_eq!(map_range(10..15, &change(9, 10, 10)), Some(10..15));
}

// ---- what a document has

fn store() -> Diagnostics {
    let mut diagnostics = Diagnostics::default();
    diagnostics.set(
        doc(1),
        vec![
            diagnostic(30, 35, Severity::Warning),
            diagnostic(10, 15, Severity::Error),
            diagnostic(50, 50, Severity::Hint),
        ],
    );
    diagnostics
}

#[test]
fn a_document_has_its_diagnostics_in_the_order_of_the_text() {
    let starts: Vec<usize> = store().of(doc(1)).iter().map(|d| d.range.start).collect();

    assert_eq!(starts, [10, 30, 50]);
    assert!(store().of(doc(2)).is_empty());
}

#[test]
fn setting_replaces_what_was_there_and_an_empty_set_removes_it() {
    let mut diagnostics = store();

    diagnostics.set(doc(1), vec![diagnostic(1, 2, Severity::Error)]);
    assert_eq!(diagnostics.of(doc(1)).len(), 1);
    diagnostics.set(doc(1), Vec::new());
    assert!(diagnostics.of(doc(1)).is_empty());
}

#[test]
fn a_change_moves_the_diagnostics_and_drops_the_ones_it_took() {
    let mut diagnostics = store();

    // Three bytes typed at the start, then the middle one's text deleted.
    diagnostics.remap(doc(1), &change(0, 0, 3));
    diagnostics.remap(doc(1), &change(34, 36, 34));

    let ranges: Vec<_> = diagnostics
        .of(doc(1))
        .iter()
        .map(|d| d.range.clone())
        .collect();
    assert_eq!(
        ranges,
        [13..18, 51..51],
        "the point moved back by the two bytes deleted before it"
    );
}

#[test]
fn a_change_to_another_document_does_nothing() {
    let mut diagnostics = store();

    diagnostics.remap(doc(2), &change(0, 0, 3));

    assert_eq!(diagnostics.of(doc(1))[0].range, 10..15);
}

#[test]
fn clearing_one_document_leaves_the_others() {
    let mut diagnostics = store();
    diagnostics.set(doc(2), vec![diagnostic(1, 2, Severity::Error)]);

    diagnostics.clear(doc(1));
    assert!(diagnostics.of(doc(1)).is_empty());
    assert_eq!(diagnostics.of(doc(2)).len(), 1);
    diagnostics.clear_all();
    assert!(diagnostics.of(doc(2)).is_empty());
}

#[test]
fn the_ones_that_touch_a_span_of_the_text() {
    let diagnostics = store();
    let starts = |from: usize, to: usize| -> Vec<usize> {
        diagnostics
            .overlapping(doc(1), from..to)
            .map(|d| d.range.start)
            .collect()
    };

    assert_eq!(
        starts(0, 10),
        Vec::<usize>::new(),
        "a range that starts where the span ends is not in it"
    );
    assert_eq!(starts(0, 11), [10]);
    assert_eq!(starts(14, 31), [10, 30]);
    assert_eq!(
        starts(15, 30),
        Vec::<usize>::new(),
        "one that ends where the span starts is not in it"
    );
    assert_eq!(starts(50, 60), [50], "a point at the start of the span");
    assert_eq!(
        starts(40, 50),
        Vec::<usize>::new(),
        "a point at the end is not"
    );
    assert_eq!(starts(0, 100), [10, 30, 50]);
}

#[test]
fn next_and_previous_go_round_the_document() {
    let diagnostics = store();
    let start = |found: Option<&Diagnostic>| found.map(|d| d.range.start);

    assert_eq!(start(diagnostics.next_after(doc(1), 0)), Some(10));
    assert_eq!(start(diagnostics.next_after(doc(1), 10)), Some(30));
    assert_eq!(
        start(diagnostics.next_after(doc(1), 50)),
        Some(10),
        "past the last goes to the first"
    );
    assert_eq!(start(diagnostics.previous_before(doc(1), 60)), Some(50));
    assert_eq!(start(diagnostics.previous_before(doc(1), 30)), Some(10));
    assert_eq!(
        start(diagnostics.previous_before(doc(1), 10)),
        Some(50),
        "before the first goes to the last"
    );
    assert_eq!(start(diagnostics.next_after(doc(2), 0)), None);
    assert_eq!(start(diagnostics.previous_before(doc(2), 0)), None);
}

#[test]
fn the_counts_are_errors_warnings_and_the_rest() {
    assert_eq!(store().counts(doc(1)), (1, 1, 1));
    assert_eq!(store().counts(doc(2)), (0, 0, 0));
}

#[test]
fn severities_sort_with_the_worst_first() {
    let mut severities = vec![
        Severity::Hint,
        Severity::Error,
        Severity::Information,
        Severity::Warning,
    ];

    severities.sort();

    assert_eq!(
        severities,
        [
            Severity::Error,
            Severity::Warning,
            Severity::Information,
            Severity::Hint
        ]
    );
}
