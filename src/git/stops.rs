//! Where "next change" and "previous change" go, for the diff tab and the gutter bars alike.

/// The line a jump to a change goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stop {
    pub line: usize,
    /// Whether there was no change in that direction, so the jump went round to the other end.
    pub wrapped: bool,
}

/// The stop for a jump from `line`. `starts` are where the changes start, in order of line;
/// a start past the text (a removal at its very end) is held to its last line, `lines` being
/// how many lines the text has. `None` when there is no change at all.
pub(crate) fn stop_among(
    starts: impl Iterator<Item = usize>,
    lines: usize,
    line: usize,
    forward: bool,
) -> Option<Stop> {
    let last = lines.saturating_sub(1);
    let stops: Vec<usize> = starts.map(|start| start.min(last)).collect();
    let (ahead, round) = match forward {
        true => (
            stops.iter().copied().find(|&stop| stop > line),
            stops.first().copied(),
        ),
        false => (
            stops.iter().rev().copied().find(|&stop| stop < line),
            stops.last().copied(),
        ),
    };
    match (ahead, round) {
        (Some(line), _) => Some(Stop {
            line,
            wrapped: false,
        }),
        (None, Some(line)) => Some(Stop {
            line,
            wrapped: true,
        }),
        (None, None) => None,
    }
}
