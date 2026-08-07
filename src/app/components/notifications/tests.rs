use std::time::Instant;

use super::{MAX_TOASTS, Notification, Notifications, Toast};
use crate::app::components::debug_window::DebugTag;

#[test]
fn push_trims_oldest_toasts() {
    let mut notifications = Notifications::new();
    for i in 0..(MAX_TOASTS + 3) {
        notifications.push(Notification::new(DebugTag::Note, format!("msg {i}")));
    }
    assert_eq!(notifications.toasts.len(), MAX_TOASTS);
    assert_eq!(notifications.toasts[0].message, "msg 3");
    assert_eq!(notifications.toasts[4].message, "msg 7");
}

#[test]
fn push_keeps_toasts_under_limit() {
    let mut notifications = Notifications::new();
    notifications.push(Notification::new(DebugTag::Note, "a".into()));
    notifications.push(Notification::new(DebugTag::Warning, "b".into()));
    assert_eq!(notifications.toasts.len(), 2);
}

#[test]
fn toast_chars_len_counts_message_plus_four() {
    let toast = Toast {
        tag: DebugTag::Error,
        message: "ab".into(),
        added_at: Instant::now(),
    };
    assert_eq!(toast.chars_len(), 6);
}
