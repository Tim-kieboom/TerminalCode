//! Mouse input that needs memory between events.

use std::time::{Duration, Instant};

/// How quickly a click must follow the previous one, on the same cell, to
/// count as a double or triple click.
const MULTI_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// How many times in a row the same cell was clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clicks {
    Single,
    Double,
    Triple,
}

impl Clicks {
    /// The click after this one in a quick series; a fourth click starts over.
    fn next(self) -> Self {
        match self {
            Self::Single => Self::Double,
            Self::Double => Self::Triple,
            Self::Triple => Self::Single,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ClickTracker {
    last: Option<Click>,
}

#[derive(Debug, Clone, Copy)]
struct Click {
    at: Instant,
    column: u16,
    row: u16,
    clicks: Clicks,
}

impl ClickTracker {
    /// Records a button press and says which click of a series it is.
    pub(crate) fn register(&mut self, now: Instant, column: u16, row: u16) -> Clicks {
        let clicks = match self.last {
            Some(last)
                if last.column == column
                    && last.row == row
                    && now.saturating_duration_since(last.at) <= MULTI_CLICK_WINDOW =>
            {
                last.clicks.next()
            }
            _ => Clicks::Single,
        };
        self.last = Some(Click {
            at: now,
            column,
            row,
            clicks,
        });
        clicks
    }
}
