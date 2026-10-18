//! From what a plugin says to what the pane draws.

use crate::plugin_api::{WireLine, WireView};
use crate::ui::view::{ViewLine, ViewNode, ViewRow};

/// The slot of the line that stands in for a view this version cannot draw.
const PLACEHOLDER_SLOT: &str = "explorer.hint";

/// What to draw for `view`. A kind of view this version does not know is drawn
/// as a line that says so, not dropped, so the pane does not look broken for no
/// reason.
pub(super) fn node_of(view: WireView) -> ViewNode {
    match view {
        WireView::Lines { lines } => ViewNode::Lines(
            lines
                .into_iter()
                .map(|line| ViewLine {
                    text: line.text.into(),
                    slot: line.slot.into(),
                })
                .collect(),
        ),
        WireView::List { items, selected } => ViewNode::List {
            items: items.into_iter().map(Into::into).collect(),
            selected,
        },
        WireView::Rows { lines, selected } => ViewNode::Rows {
            rows: lines.into_iter().map(row_of).collect(),
            selected,
        },
        WireView::Input {
            prompt,
            value,
            cursor,
        } => ViewNode::Input {
            prompt: prompt.into(),
            value: value.into(),
            cursor,
        },
        _ => ViewNode::Lines(vec![ViewLine {
            text: "(this plugin sent a view this version cannot show)".into(),
            slot: PLACEHOLDER_SLOT.into(),
        }]),
    }
}

/// A line as a row: its spans, or the text it has as the only one.
fn row_of(line: WireLine) -> ViewRow {
    let spans = match line.spans.is_empty() {
        true => vec![ViewLine {
            text: line.text.into(),
            slot: line.slot.into(),
        }],
        false => line
            .spans
            .into_iter()
            .map(|span| ViewLine {
                text: span.text.into(),
                slot: span.slot.into(),
            })
            .collect(),
    };
    ViewRow { spans }
}
