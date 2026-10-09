use ropey::Rope;

use crate::buffer::{Buffer, Edit, EditInfo};
use crate::syntax::Language;
use crate::syntax::worker::{Job, coalesce};

/// A list of just one range.
fn one(range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    vec![range]
}

fn info(start: usize) -> EditInfo {
    let mut buffer = Buffer::from_text(&"x".repeat(100));
    let mut transaction = buffer.begin_transaction(Default::default());
    transaction.apply(&Edit::new(start..start, "y")).unwrap();
    transaction.commit(Default::default());
    buffer.take_edit_log()[0]
}

fn job(version: u64, base: Option<u64>, edits: Vec<EditInfo>) -> Job {
    Job {
        key: 1,
        language: Language::Rust,
        text: Rope::from_str("fn a() {}"),
        version,
        base,
        edits,
        ranges: one(0..9),
    }
}

#[test]
fn a_job_that_continues_the_one_it_replaces_takes_over_its_edits() {
    let older = job(3, Some(1), vec![info(1), info(2)]);
    let newer = job(5, Some(3), vec![info(3), info(4)]);

    let merged = coalesce(older, newer);

    assert_eq!(merged.version, 5);
    assert_eq!(
        merged.base,
        Some(1),
        "still continues from where the first began"
    );
    assert_eq!(merged.edits, [info(1), info(2), info(3), info(4)]);
}

#[test]
fn the_first_job_of_a_document_stays_without_a_base_when_joined() {
    let older = job(0, None, Vec::new());
    let newer = job(2, Some(0), vec![info(1), info(2)]);

    let merged = coalesce(older, newer);

    assert_eq!(merged.base, None);
    assert_eq!(merged.version, 2);
}

#[test]
fn a_job_that_does_not_continue_the_old_one_stands_alone() {
    let older = job(3, Some(1), vec![info(1), info(2)]);
    let newer = job(9, Some(7), vec![info(5), info(6)]);

    let merged = coalesce(older, newer);

    assert_eq!(merged.base, None);
    assert!(merged.edits.is_empty());
}

#[test]
fn the_newer_jobs_text_and_ranges_win() {
    let mut older = job(1, Some(0), vec![info(1)]);
    older.ranges = one(0..4);
    let mut newer = job(2, Some(1), vec![info(2)]);
    newer.ranges = one(5..9);
    newer.text = Rope::from_str("fn b() {}");

    let merged = coalesce(older, newer);

    assert_eq!(merged.ranges, one(5..9));
    assert_eq!(merged.text.to_string(), "fn b() {}");
}
