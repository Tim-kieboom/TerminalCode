/// Declarative content a plugin pushes into its pane. The host renders it;
/// plugins never see `Frame` or `Rect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewNode {
    Lines(Vec<ViewLine>),
    List {
        items: Vec<Box<str>>,
        selected: Option<usize>,
    },
}

/// One line of text and the theme slot that styles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewLine {
    pub(crate) text: Box<str>,
    pub(crate) slot: Box<str>,
}
