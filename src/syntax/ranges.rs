use std::ops::Range;

/// The most ranges a document is highlighted for at once: one per pane that
/// shows it, up to this many.
pub(crate) const MAX_RANGES: usize = 8;

/// Sorts `ranges`, joins the ones that overlap or touch, drops empty ones and
/// keeps at most `cap` of the result (the first ones, by position).
pub(crate) fn merge(mut ranges: Vec<Range<usize>>, cap: usize) -> Vec<Range<usize>> {
    ranges.retain(|range| range.start < range.end);
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged.truncate(cap);
    merged
}
