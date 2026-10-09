use super::Span;
use crate::buffer::EditInfo;

/// Moves `spans` (sorted, not overlapping) to match the text after `edit`, so
/// colors stay where they were until the text is parsed again.
///
/// A span before the edit stays, one after it shifts. Text inserted exactly at
/// the edge of a span stays outside it. If the edit lies inside a span the
/// span grows or shrinks with it, and new text that replaces a whole span (or
/// part of its inside) takes its color. A span the edit cuts into keeps only
/// the part that is left, with the new text outside; a span it covers
/// completely is dropped.
pub(crate) fn map_spans(spans: &mut Vec<Span>, edit: &EditInfo) {
    let (start, old_end, new_end) = (edit.start_byte, edit.old_end_byte, edit.new_end_byte);
    let shift = |byte: usize| byte - old_end + new_end;
    spans.retain_mut(|span| {
        let range = &mut span.range;
        if range.end <= start {
            return true;
        }
        if range.start >= old_end {
            *range = shift(range.start)..shift(range.end);
            return true;
        }
        // The edit overlaps the span.
        if range.start <= start && old_end <= range.end {
            range.end = shift(range.end);
        } else if range.start < start {
            range.end = start;
        } else if range.end > old_end {
            *range = new_end..shift(range.end);
        } else {
            return false;
        }
        range.start < range.end
    });
}
