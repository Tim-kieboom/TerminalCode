use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::pty::queue::{BoundedQueue, PushError};

const SHORT: Duration = Duration::from_millis(20);

#[test]
fn items_come_out_in_the_order_they_went_in() {
    let queue = BoundedQueue::new(4);

    for n in 0..3 {
        queue.push_within(n, SHORT).unwrap();
    }

    assert_eq!(queue.pop(), Some(0));
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.pop(), Some(2));
}

#[test]
fn a_full_queue_gives_the_item_back_after_the_wait() {
    let queue = BoundedQueue::new(2);
    queue.push_within(1, SHORT).unwrap();
    queue.push_within(2, SHORT).unwrap();

    let start = Instant::now();
    let result = queue.push_within(3, SHORT);

    assert_eq!(result, Err(PushError::Full(3)));
    assert!(start.elapsed() >= SHORT, "it did wait");
    assert!(start.elapsed() < Duration::from_secs(1), "but not long");
}

#[test]
fn a_push_with_no_wait_fails_at_once_when_full() {
    let queue = BoundedQueue::new(1);
    queue.push_within(1, SHORT).unwrap();

    let start = Instant::now();

    assert_eq!(
        queue.push_within(2, Duration::ZERO),
        Err(PushError::Full(2))
    );
    assert!(start.elapsed() < Duration::from_millis(15));
}

#[test]
fn popping_makes_room_for_a_waiting_push() {
    let queue = Arc::new(BoundedQueue::new(1));
    queue.push_within(1, SHORT).unwrap();
    let popper = Arc::clone(&queue);
    let thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        popper.pop()
    });

    let result = queue.push_within(2, Duration::from_secs(5));

    assert_eq!(result, Ok(()));
    assert_eq!(thread.join().unwrap(), Some(1));
    assert_eq!(queue.pop(), Some(2));
}

#[test]
fn popping_waits_for_an_item() {
    let queue = Arc::new(BoundedQueue::new(1));
    let pusher = Arc::clone(&queue);
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        pusher.push_within(7, SHORT).unwrap();
    });

    assert_eq!(queue.pop(), Some(7));
}

#[test]
fn a_closed_queue_refuses_pushes_but_hands_out_what_it_holds() {
    let queue = BoundedQueue::new(4);
    queue.push_within(1, SHORT).unwrap();

    queue.close();

    assert_eq!(queue.push_within(2, SHORT), Err(PushError::Closed(2)));
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.pop(), None);
}

#[test]
fn closing_wakes_a_waiting_pop_and_a_waiting_push() {
    let queue = Arc::new(BoundedQueue::<u8>::new(1));
    let popper = {
        let queue = Arc::clone(&queue);
        thread::spawn(move || queue.pop())
    };
    thread::sleep(Duration::from_millis(30));
    queue.close();
    assert_eq!(popper.join().unwrap(), None);

    let full = Arc::new(BoundedQueue::new(1));
    full.push_within(1, SHORT).unwrap();
    let pusher = {
        let full = Arc::clone(&full);
        thread::spawn(move || full.push_within(2, Duration::from_secs(30)))
    };
    thread::sleep(Duration::from_millis(30));
    full.close();
    assert_eq!(pusher.join().unwrap(), Err(PushError::Closed(2)));
}
