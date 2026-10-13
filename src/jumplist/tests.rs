use std::path::PathBuf;

use super::{Change, JumpList, LIMIT, Place};
use crate::components::workspace::DocumentId;

fn doc(number: u32) -> DocumentId {
    DocumentId::for_test(number)
}

fn place(document: u32, byte: usize) -> Place {
    Place {
        document: doc(document),
        path: Some(PathBuf::from(format!("/p/{document}.rs"))),
        byte,
    }
}

fn always(_: &Place) -> bool {
    true
}

fn bytes(list: &JumpList) -> Vec<(u32, usize)> {
    list.entries
        .iter()
        .map(|place| (place_number(place), place.byte))
        .collect()
}

fn place_number(place: &Place) -> u32 {
    let name = place
        .path
        .as_ref()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    name.trim_end_matches(".rs").parse().unwrap()
}

#[test]
fn an_empty_list_goes_nowhere() {
    let mut list = JumpList::default();

    assert_eq!(list.back(place(1, 5), always), None);
    assert_eq!(list.forward(always), None);
    assert!(list.is_empty());
}

#[test]
fn back_goes_to_the_place_a_jump_left_and_forward_comes_back() {
    let mut list = JumpList::default();
    list.record(place(1, 10));

    let step = list.back(place(2, 40), always).unwrap();

    assert_eq!(step.place, place(1, 10));
    assert_eq!(step.skipped, 0);
    assert_eq!(list.forward(always).unwrap().place, place(2, 40));
    assert_eq!(list.forward(always), None);
}

#[test]
fn several_jumps_are_walked_back_and_forward_in_order() {
    let mut list = JumpList::default();
    list.record(place(1, 1));
    list.record(place(2, 2));
    list.record(place(3, 3));

    assert_eq!(list.back(place(4, 4), always).unwrap().place, place(3, 3));
    assert_eq!(list.back(place(3, 3), always).unwrap().place, place(2, 2));
    assert_eq!(list.back(place(2, 2), always).unwrap().place, place(1, 1));
    assert_eq!(list.back(place(1, 1), always), None);
    assert_eq!(list.forward(always).unwrap().place, place(2, 2));
    assert_eq!(list.forward(always).unwrap().place, place(3, 3));
    assert_eq!(list.forward(always).unwrap().place, place(4, 4));
    assert_eq!(list.forward(always), None);
}

#[test]
fn a_new_jump_after_going_back_drops_what_was_ahead() {
    let mut list = JumpList::default();
    list.record(place(1, 1));
    list.record(place(2, 2));
    list.back(place(3, 3), always).unwrap();
    list.back(place(2, 2), always).unwrap();

    // At 1 now; jumping from there forgets 2 and 3.
    list.record(place(1, 1));

    assert_eq!(bytes(&list), [(1, 1)]);
    assert_eq!(list.forward(always), None);
}

#[test]
fn the_same_place_twice_in_a_row_is_kept_once() {
    let mut list = JumpList::default();

    list.record(place(1, 10));
    list.record(place(1, 10));
    list.record(place(1, 11));

    assert_eq!(bytes(&list), [(1, 10), (1, 11)]);
}

#[test]
fn going_back_from_where_the_list_already_is_goes_to_the_one_before() {
    let mut list = JumpList::default();
    list.record(place(1, 1));
    list.record(place(2, 2));

    // The cursor is at 2 again, the last place that was recorded.
    let step = list.back(place(2, 2), always).unwrap();

    assert_eq!(step.place, place(1, 1));
}

#[test]
fn back_does_not_stop_on_a_place_that_is_where_you_are() {
    let mut list = JumpList::default();
    list.record(place(1, 1));
    list.record(place(2, 2));
    list.record(place(1, 1));

    // Here is 1:1; the nearest older place that is not it is 2:2.
    let step = list.back(place(1, 1), always).unwrap();

    assert_eq!(step.place, place(2, 2));
    assert_eq!(step.skipped, 0);
}

#[test]
fn places_that_are_not_usable_are_forgotten_and_counted() {
    let mut list = JumpList::default();
    list.record(place(1, 1));
    list.record(place(2, 2));
    list.record(place(3, 3));
    let gone = |place: &Place| place.document != doc(3) && place.document != doc(2);

    let step = list.back(place(4, 4), gone).unwrap();

    assert_eq!(step.place, place(1, 1));
    assert_eq!(step.skipped, 2);
    assert_eq!(bytes(&list), [(1, 1), (4, 4)]);
}

#[test]
fn when_nothing_older_is_usable_it_is_none_and_they_are_forgotten() {
    let mut list = JumpList::default();
    list.record(place(1, 1));

    let step = list.back(place(2, 2), |_| false);

    assert_eq!(step, None);
    assert_eq!(bytes(&list), [(2, 2)]);
}

#[test]
fn forward_forgets_the_places_that_are_not_usable_too() {
    let mut list = JumpList::default();
    list.record(place(1, 1));
    list.record(place(2, 2));
    list.back(place(3, 3), always).unwrap();
    list.back(place(2, 2), always).unwrap();

    let step = list.forward(|place| place.document != doc(2)).unwrap();

    assert_eq!(step.place, place(3, 3));
    assert_eq!(step.skipped, 1);
}

#[test]
fn only_the_last_hundred_places_are_kept() {
    let mut list = JumpList::default();

    for byte in 0..LIMIT + 20 {
        list.record(place(1, byte));
    }

    assert_eq!(list.len(), LIMIT);
    assert_eq!(list.entries.first().unwrap().byte, 20);
    assert_eq!(list.entries.last().unwrap().byte, LIMIT + 19);
    // Going back from the present adds where you are and still keeps the limit.
    list.back(place(1, 9999), always).unwrap();
    assert_eq!(list.len(), LIMIT);
}

// ---- moving with edits

fn change(start: usize, old_end: usize, new_end: usize) -> Change {
    Change {
        start,
        old_end,
        new_end,
    }
}

#[test]
fn an_edit_before_a_place_moves_it_and_one_after_does_not() {
    let mut list = JumpList::default();
    list.record(place(1, 100));

    // Ten bytes typed at 50.
    list.remap(doc(1), change(50, 50, 60));
    assert_eq!(bytes(&list), [(1, 110)]);
    // Twenty bytes deleted at 10..30.
    list.remap(doc(1), change(10, 30, 10));
    assert_eq!(bytes(&list), [(1, 90)]);
    // Something typed after it.
    list.remap(doc(1), change(500, 500, 600));
    assert_eq!(bytes(&list), [(1, 90)]);
}

#[test]
fn text_typed_at_a_place_goes_after_it() {
    let mut list = JumpList::default();
    list.record(place(1, 10));

    list.remap(doc(1), change(10, 10, 15));

    assert_eq!(bytes(&list), [(1, 10)]);
}

#[test]
fn a_place_inside_what_was_replaced_goes_to_the_start_of_it() {
    let mut list = JumpList::default();
    list.record(place(1, 20));

    list.remap(doc(1), change(10, 30, 12));

    assert_eq!(bytes(&list), [(1, 10)]);
}

#[test]
fn an_edit_moves_only_the_places_of_its_document() {
    let mut list = JumpList::default();
    list.record(place(1, 100));
    list.record(place(2, 100));

    list.remap(doc(2), change(0, 0, 7));

    assert_eq!(bytes(&list), [(1, 100), (2, 107)]);
}

// ---- files

#[test]
fn a_reopened_file_gives_its_places_its_new_document() {
    let mut list = JumpList::default();
    list.record(place(1, 5));
    list.record(place(2, 6));

    list.reopened(&PathBuf::from("/p/1.rs"), doc(9));

    assert_eq!(list.entries[0].document, doc(9));
    assert_eq!(list.entries[1].document, doc(2));
}
