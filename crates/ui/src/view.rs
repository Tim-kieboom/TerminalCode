/// Declarative content a plugin pushes into its pane. The host renders it;
/// plugins never see `Frame` or `Rect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNode {
    Lines(Vec<ViewLine>),
    List {
        items: Vec<Box<str>>,
        selected: Option<usize>,
    },
}

impl Default for ViewNode {
    fn default() -> Self {
        Self::Lines(Vec::new())
    }
}

/// One line of text and the theme slot that styles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    pub text: Box<str>,
    pub slot: Box<str>,
}
