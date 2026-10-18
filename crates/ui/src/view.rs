/// Declarative content a plugin pushes into its pane. The host renders it;
/// plugins never see `Frame` or `Rect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNode {
    Lines(Vec<ViewLine>),
    List {
        items: Vec<Box<str>>,
        selected: Option<usize>,
    },
    /// Lines of pieces with styles of their own, one of them selected: the pane
    /// draws the selected row highlighted and scrolls to keep it on screen (see
    /// [`rows_scroll`]).
    Rows {
        rows: Vec<ViewRow>,
        selected: Option<usize>,
    },
    /// A line the user types into: a prompt and the text, with the cursor at
    /// character `cursor` of it. While the pane has the keyboard the plugin gets
    /// the keys.
    Input {
        prompt: Box<str>,
        value: Box<str>,
        cursor: usize,
    },
}

impl ViewNode {
    /// Whether the view is a line to type into.
    pub fn is_input(&self) -> bool {
        matches!(self, Self::Input { .. })
    }
}

impl Default for ViewNode {
    fn default() -> Self {
        Self::Lines(Vec::new())
    }
}

/// One row of a [`ViewNode::Rows`]: pieces of text, each with a theme slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewRow {
    pub spans: Vec<ViewLine>,
}

/// Which row is the first one shown in a pane `height` rows high, for `len` rows
/// of which `selected` must be visible: none scrolled while it is near the top,
/// and then the pane is centred on it, down to the last row at the bottom.
pub fn rows_scroll(selected: Option<usize>, len: usize, height: usize) -> usize {
    let (Some(selected), true) = (selected, height > 0) else {
        return 0;
    };
    let last_start = len.saturating_sub(height);
    selected.saturating_sub(height / 2).min(last_start)
}

/// One line of text and the theme slot that styles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    pub text: Box<str>,
    pub slot: Box<str>,
}
