use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

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
        /// Whether the line is being typed into (a line inside a column may not be).
        active: bool,
    },
    /// A row of buttons.
    Buttons(Vec<ViewButton>),
    /// Parts one under the other (see [`ViewNode::leaves`] for how the height is
    /// shared).
    Column(Vec<ViewNode>),
}

/// One button of a [`ViewNode::Buttons`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewButton {
    pub id: Box<str>,
    pub label: Box<str>,
    pub enabled: bool,
}

impl ViewNode {
    /// Whether the view has a line that is being typed into, so that the keys go to
    /// the plugin as text.
    pub fn is_input(&self) -> bool {
        match self {
            Self::Input { active, .. } => *active,
            Self::Column(parts) => parts.iter().any(Self::is_input),
            _ => false,
        }
    }

    /// The parts that are drawn and what they are given, for an `area`: a column
    /// is cut into its parts and every other view is its own. A buttons row and a
    /// line to type into take one row, lines as many as they have, and rows or a
    /// list share what is left; parts that do not fit get less, the last ones none
    /// (and are left out). A column inside a column is not drawn.
    pub fn leaves(&self, area: Rect) -> Vec<(Rect, &ViewNode)> {
        let Self::Column(parts) = self else {
            return vec![(area, self)];
        };
        let fixed = |part: &ViewNode| -> Option<u16> {
            match part {
                Self::Buttons(_) | Self::Input { .. } => Some(1),
                Self::Lines(lines) => Some(u16::try_from(lines.len()).unwrap_or(u16::MAX)),
                Self::Column(_) => Some(0),
                Self::List { .. } | Self::Rows { .. } => None,
            }
        };
        let wanted: u16 = parts.iter().filter_map(fixed).fold(0, u16::saturating_add);
        let flexible = parts.iter().filter(|part| fixed(part).is_none()).count();
        let spare = area.height.saturating_sub(wanted);
        let share = spare / flexible.max(1) as u16;
        let mut extra = spare - share * flexible as u16;
        let mut y = area.y;
        let mut left = area.height;
        let mut leaves = Vec::new();
        for part in parts {
            let height = match fixed(part) {
                Some(height) => height,
                None => {
                    // What does not divide goes to the first of them.
                    let more = std::mem::take(&mut extra);
                    share + more
                }
            }
            .min(left);
            if height == 0 {
                continue;
            }
            leaves.push((Rect::new(area.x, y, area.width, height), part));
            y += height;
            left -= height;
        }
        leaves
    }
}

/// Where each button of a row is drawn, as (offset from the left edge, width): a
/// space, the label and a space, with one column between buttons.
pub fn button_cells(buttons: &[ViewButton]) -> Vec<(u16, u16)> {
    let mut at = 0u16;
    buttons
        .iter()
        .map(|button| {
            let width = u16::try_from(button.label.width() + 2).unwrap_or(u16::MAX);
            let cell = (at, width);
            at = at.saturating_add(width).saturating_add(1);
            cell
        })
        .collect()
}

/// The button a click `offset` columns from the left edge of the row is on.
pub fn button_at(buttons: &[ViewButton], offset: u16) -> Option<usize> {
    button_cells(buttons)
        .iter()
        .position(|&(start, width)| offset >= start && offset < start.saturating_add(width))
}

/// The character of `value` a click `offset` columns from the left edge of a line
/// to type into is on, after the prompt: the end of the text for a click past it,
/// and 0 for a click on the prompt.
pub fn input_column_at(prompt: &str, value: &str, offset: u16) -> usize {
    let mut x = prompt.width();
    let offset = usize::from(offset);
    if offset < x {
        return 0;
    }
    for (index, ch) in value.chars().enumerate() {
        let width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        // A click on the left half of a wide character is before it.
        if offset < x + width.max(1) {
            return index;
        }
        x += width;
    }
    value.chars().count()
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
