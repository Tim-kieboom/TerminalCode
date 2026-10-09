use tokio::sync::mpsc;

use super::Background;
use crate::event::Event;

#[test]
fn nothing_is_connected_at_first() {
    let background = Background::default();

    assert!(!background.is_connected());
    assert!(background.sender().is_none());
}

#[test]
fn a_connected_background_hands_out_senders_to_the_same_channel() {
    let (tx, mut rx) = mpsc::channel(4);
    let mut background = Background::default();

    background.connect(tx);
    let sender = background.sender().expect("connected");

    assert!(background.is_connected());
    sender.try_send(Event::WatchFailed("x".to_owned())).unwrap();
    assert_eq!(rx.try_recv().unwrap(), Event::WatchFailed("x".to_owned()));
}

#[test]
fn scans_and_searches_are_numbered_from_one_independently() {
    let mut background = Background::default();

    assert_eq!(background.next_scan(), 1);
    assert_eq!(background.next_scan(), 2);
    assert_eq!(background.next_search(), 1);
    assert_eq!(background.next_scan(), 3);
    assert_eq!(background.next_search(), 2);
}

#[test]
fn a_number_is_never_zero_so_a_search_that_was_reset_to_zero_matches_none() {
    let mut background = Background::default();

    assert_ne!(background.next_scan(), 0);
    assert_ne!(background.next_search(), 0);
}
