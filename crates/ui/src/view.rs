/// Declarative content a plugin pushes into its pane. The host renders it;
/// plugins never see `Frame` or `Rect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewNode {
    Lines(Vec<ViewLine>),
    List {
        items: Vec<Box<str>>,
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

/// One line of text and the theme slot that styles it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewLine {
    pub text: Box<str>,
    pub slot: Box<str>,
}
