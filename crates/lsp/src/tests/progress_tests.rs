use serde_json::json;

use crate::progress::Progress;

fn progress(events: &[serde_json::Value]) -> Progress {
    let mut progress = Progress::default();
    for params in events {
        progress.update(params);
    }
    progress
}

fn begin(token: serde_json::Value, title: &str) -> serde_json::Value {
    json!({ "token": token, "value": { "kind": "begin", "title": title } })
}

#[test]
fn a_server_with_no_jobs_is_idle() {
    let progress = Progress::default();

    assert!(progress.is_idle());
    assert_eq!(progress.text(), None);
}

#[test]
fn a_job_that_begins_shows_its_title() {
    let progress = progress(&[begin(json!("t"), "Indexing")]);

    assert!(!progress.is_idle());
    assert_eq!(progress.text().as_deref(), Some("Indexing"));
}

#[test]
fn a_report_adds_the_message_and_the_percentage_and_keeps_what_it_leaves_out() {
    let mut progress = progress(&[begin(json!("t"), "Indexing")]);

    progress.update(&json!({ "token": "t", "value": { "kind": "report", "message": "12/340 (serde)", "percentage": 3 } }));
    assert_eq!(
        progress.text().as_deref(),
        Some("Indexing: 12/340 (serde) 3%")
    );

    progress.update(&json!({ "token": "t", "value": { "kind": "report", "percentage": 4 } }));
    assert_eq!(
        progress.text().as_deref(),
        Some("Indexing: 12/340 (serde) 4%")
    );
}

#[test]
fn a_begin_can_carry_the_message_and_percentage_too() {
    let progress = progress(&[json!({
        "token": 7,
        "value": { "kind": "begin", "title": "Loading", "message": "workspace", "percentage": 0 },
    })]);

    assert_eq!(progress.text().as_deref(), Some("Loading: workspace 0%"));
}

#[test]
fn an_empty_message_is_not_shown() {
    let progress = progress(&[json!({
        "token": "t",
        "value": { "kind": "begin", "title": "Indexing", "message": "" },
    })]);

    assert_eq!(progress.text().as_deref(), Some("Indexing"));
}

#[test]
fn the_end_of_a_job_clears_it_and_the_server_is_idle_again() {
    let mut progress = progress(&[begin(json!("t"), "Indexing")]);

    progress.update(&json!({ "token": "t", "value": { "kind": "end" } }));

    assert!(progress.is_idle());
}

#[test]
fn the_newest_job_is_shown_and_the_older_one_comes_back_when_it_ends() {
    let mut progress = progress(&[begin(json!("a"), "Indexing"), begin(json!("b"), "Checking")]);
    assert_eq!(progress.text().as_deref(), Some("Checking"));

    progress.update(&json!({ "token": "b", "value": { "kind": "end" } }));

    assert_eq!(progress.text().as_deref(), Some("Indexing"));
}

#[test]
fn tokens_may_be_numbers_or_strings_and_a_number_is_not_its_text() {
    let mut progress = progress(&[begin(json!(1), "One"), begin(json!("1"), "Text one")]);
    assert_eq!(progress.text().as_deref(), Some("Text one"));

    progress.update(&json!({ "token": "1", "value": { "kind": "end" } }));

    assert_eq!(progress.text().as_deref(), Some("One"));
}

#[test]
fn a_begin_for_a_token_in_use_starts_it_over() {
    let progress = progress(&[begin(json!("t"), "First"), begin(json!("t"), "Second")]);

    assert_eq!(progress.text().as_deref(), Some("Second"));
    assert!(progress.text().is_some());
}

#[test]
fn reports_and_ends_for_jobs_that_never_began_are_ignored() {
    let progress = progress(&[
        json!({ "token": "ghost", "value": { "kind": "report", "percentage": 50 } }),
        json!({ "token": "ghost", "value": { "kind": "end" } }),
    ]);

    assert!(progress.is_idle());
}

#[test]
fn what_is_not_a_report_is_ignored_instead_of_failing() {
    let progress = progress(&[
        json!(null),
        json!({}),
        json!({ "token": "t" }),
        json!({ "token": "t", "value": { "kind": "surprise" } }),
        json!({ "token": "t", "value": 3 }),
    ]);

    assert!(progress.is_idle());
}

#[test]
fn a_begin_with_no_title_still_shows_that_something_is_going_on() {
    let progress = progress(&[json!({ "token": "t", "value": { "kind": "begin" } })]);

    assert_eq!(progress.text().as_deref(), Some("working"));
}
