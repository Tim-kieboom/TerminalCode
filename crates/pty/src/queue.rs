use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A queue with room for a fixed number of items. Pushing waits a limited time
/// for room; popping waits for an item. Closing it wakes everyone.
#[derive(Debug)]
pub(super) struct BoundedQueue<T> {
    state: Mutex<State<T>>,
    has_item: Condvar,
    has_room: Condvar,
    capacity: usize,
}

#[derive(Debug)]
struct State<T> {
    items: VecDeque<T>,
    closed: bool,
}

/// Why an item was not queued; the item comes back.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PushError<T> {
    /// There was still no room when the wait ran out.
    Full(T),
    Closed(T),
}

impl<T> BoundedQueue<T> {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State {
                items: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            has_item: Condvar::new(),
            has_room: Condvar::new(),
            capacity,
        }
    }

    /// Queues `item`, waiting up to `wait` for room.
    pub(super) fn push_within(&self, item: T, wait: Duration) -> Result<(), PushError<T>> {
        let deadline = Instant::now() + wait;
        let mut state = self.lock();
        loop {
            if state.closed {
                return Err(PushError::Closed(item));
            }
            if state.items.len() < self.capacity {
                state.items.push_back(item);
                self.has_item.notify_one();
                return Ok(());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(PushError::Full(item));
            }
            state = self
                .has_room
                .wait_timeout(state, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }

    /// The oldest item, waiting for one. `None` once the queue is closed and
    /// empty.
    pub(super) fn pop(&self) -> Option<T> {
        let mut state = self.lock();
        loop {
            if let Some(item) = state.items.pop_front() {
                self.has_room.notify_one();
                return Some(item);
            }
            if state.closed {
                return None;
            }
            state = self
                .has_item
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Refuses further pushes and wakes everyone waiting. Items already queued
    /// can still be popped.
    pub(super) fn close(&self) {
        self.lock().closed = true;
        self.has_item.notify_all();
        self.has_room.notify_all();
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
