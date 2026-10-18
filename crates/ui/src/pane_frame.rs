//! The frame drawn around a component: its border and its title.

use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, BorderType, Borders};
use serde::Deserialize;

use crate::ComponentKind;
use crate::theme::Theme;

/// Theme slot of the border by default; the border of a focused pane uses
/// the slot with `.focused` appended when the theme has one.
const DEFAULT_BORDER_SLOT: &str = "pane.border";
const DEFAULT_TITLE_SLOT: &str = "pane.title";

/// An edge of a frame that has a border line. A layout file lists them in
/// `sides`; an empty list means no border at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
    /// All four edges.
    All,
}

impl Side {
    fn borders(self) -> Borders {
        match self {
            Self::Top => Borders::TOP,
            Self::Right => Borders::RIGHT,
            Self::Bottom => Borders::BOTTOM,
            Self::Left => Borders::LEFT,
            Self::All => Borders::ALL,
        }
    }
}

fn borders_of(sides: &[Side]) -> Borders {
    sides
        .iter()
        .fold(Borders::NONE, |all, side| all | side.borders())
}

/// How the border lines are drawn: one of ratatui's border types.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum BorderStyle {
    #[default]
    Plain,
    Rounded,
    Double,
    Thick,
    LightDoubleDashed,
    HeavyDoubleDashed,
    LightTripleDashed,
    HeavyTripleDashed,
    LightQuadrupleDashed,
    HeavyQuadrupleDashed,
    QuadrantInside,
    QuadrantOutside,
}

impl From<BorderStyle> for BorderType {
    fn from(style: BorderStyle) -> Self {
        match style {
            BorderStyle::Plain => Self::Plain,
            BorderStyle::Rounded => Self::Rounded,
            BorderStyle::Double => Self::Double,
            BorderStyle::Thick => Self::Thick,
            BorderStyle::LightDoubleDashed => Self::LightDoubleDashed,
            BorderStyle::HeavyDoubleDashed => Self::HeavyDoubleDashed,
            BorderStyle::LightTripleDashed => Self::LightTripleDashed,
            BorderStyle::HeavyTripleDashed => Self::HeavyTripleDashed,
            BorderStyle::LightQuadrupleDashed => Self::LightQuadrupleDashed,
            BorderStyle::HeavyQuadrupleDashed => Self::HeavyQuadrupleDashed,
            BorderStyle::QuadrantInside => Self::QuadrantInside,
            BorderStyle::QuadrantOutside => Self::QuadrantOutside,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub enum Title {
    /// Nothing is shown.
    Hidden,
    /// The component's own name: `Explorer`, the plugin's id, the file name
    /// for an editor.
    #[default]
    Name,
    Text(Box<str>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum TitleAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What a layout file says about a component's frame. Anything left out takes
/// the component's default (see [`PaneFrame::default_for`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FrameSpec {
    /// The edges that have a border line; empty for none.
    pub sides: Option<Vec<Side>>,
    pub border: Option<BorderStyle>,
    pub title: Option<Title>,
    pub title_align: Option<TitleAlign>,
    /// Theme slot that styles the title.
    pub title_slot: Option<Box<str>>,
    /// Theme slot that styles the border.
    pub border_slot: Option<Box<str>>,
}

/// A component's frame with everything filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneFrame {
    sides: Borders,
    border: BorderStyle,
    title: Title,
    title_align: TitleAlign,
    title_slot: Box<str>,
    border_slot: Box<str>,
    /// The title row holds the tab strip of a bar with several views, drawn by
    /// the bar and not as a title.
    tabs: bool,
}

impl PaneFrame {
    /// How a component is framed when the layout does not say: panes get a
    /// plain border and their name; the editor shows file names in its tabs
    /// instead of a title; the status bar is a bare line.
    pub fn default_for(kind: &ComponentKind) -> Self {
        let (sides, title) = match kind {
            ComponentKind::Editor => (Borders::ALL, Title::Hidden),
            ComponentKind::StatusBar | ComponentKind::MenuBar => (Borders::NONE, Title::Hidden),
            _ => (Borders::ALL, Title::Name),
        };
        Self {
            sides,
            border: BorderStyle::default(),
            title,
            title_align: TitleAlign::Left,
            title_slot: DEFAULT_TITLE_SLOT.into(),
            border_slot: DEFAULT_BORDER_SLOT.into(),
            tabs: false,
        }
    }

    /// `spec` laid over the defaults of `kind`.
    pub fn resolve(spec: &FrameSpec, kind: &ComponentKind) -> Self {
        let defaults = Self::default_for(kind);
        Self {
            sides: spec.sides.as_deref().map_or(defaults.sides, borders_of),
            border: spec.border.unwrap_or(defaults.border),
            title: spec.title.clone().unwrap_or(defaults.title),
            title_align: spec.title_align.unwrap_or(defaults.title_align),
            title_slot: spec.title_slot.clone().unwrap_or(defaults.title_slot),
            border_slot: spec.border_slot.clone().unwrap_or(defaults.border_slot),
            tabs: false,
        }
    }

    pub fn sides(&self) -> Borders {
        self.sides
    }

    pub fn border(&self) -> BorderStyle {
        self.border
    }

    pub fn title(&self) -> &Title {
        &self.title
    }

    pub fn title_align(&self) -> TitleAlign {
        self.title_align
    }

    pub fn title_slot(&self) -> &str {
        &self.title_slot
    }

    pub fn border_slot(&self) -> &str {
        &self.border_slot
    }

    /// This frame with its title row given to a bar's tab strip.
    pub fn with_tab_row(mut self) -> Self {
        self.tabs = true;
        self
    }

    /// Whether the frame has a title row, for a title or for tabs.
    pub fn shows_title(&self) -> bool {
        self.tabs || self.title != Title::Hidden
    }

    /// Where a bar's tab strip may go: the title row, between the corners, as the
    /// columns `from..to` of row `row`.
    pub fn tab_row(&self, area: Rect) -> (u16, u16, u16) {
        let from = area.x + u16::from(self.sides.contains(Borders::LEFT));
        let to = area
            .right()
            .saturating_sub(u16::from(self.sides.contains(Borders::RIGHT)));
        (area.y, from, to.max(from))
    }

    /// How the strip is aligned in the title row.
    pub fn tab_align(&self) -> TitleAlign {
        self.title_align
    }

    /// The title to draw, given the component's own `name`.
    pub fn title_text<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        match &self.title {
            Title::Hidden => None,
            Title::Name => Some(name),
            Title::Text(text) => Some(text),
        }
    }

    /// The frame as a widget. `title` is already resolved (see
    /// [`PaneFrame::title_text`]). A focused frame uses the `.focused`
    /// variant of its border slot when the theme defines one.
    pub fn block<'a>(&self, theme: &Theme, title: Option<&'a str>, focused: bool) -> Block<'a> {
        let mut block = self
            .shell()
            .border_style(theme.style(&self.border_style_slot(theme, focused)));

        if let Some(title) = title.filter(|_| !self.tabs) {
            block = block
                .title(title)
                .title_alignment(self.alignment())
                .title_style(theme.style(&self.title_slot));
        }
        // The tab strip is drawn over the title row afterwards. Without a top border
        // that row is the first row of the content, so the block says it is taken
        // (as `inner` does), or whoever lays out the content from the block's own
        // `inner` puts it under the strip.
        if self.tabs && !self.sides.contains(Borders::TOP) {
            block = block.title(" ");
        }
        block
    }

    /// The area left for the content inside the frame, as [`PaneFrame::block`]
    /// draws it. A title costs a row even without a border.
    pub fn inner(&self, area: Rect) -> Rect {
        let mut shell = self.shell();
        if self.shows_title() {
            shell = shell.title(" ");
        }
        shell.inner(area)
    }

    /// The borders without any styling or title.
    fn shell(&self) -> Block<'static> {
        Block::new()
            .borders(self.sides)
            .border_type(self.border.into())
    }

    fn border_style_slot(&self, theme: &Theme, focused: bool) -> String {
        let focused_slot = format!("{}.focused", self.border_slot);
        if focused && theme.contains(&focused_slot) {
            focused_slot
        } else {
            self.border_slot.to_string()
        }
    }

    fn alignment(&self) -> Alignment {
        match self.title_align {
            TitleAlign::Left => Alignment::Left,
            TitleAlign::Center => Alignment::Center,
            TitleAlign::Right => Alignment::Right,
        }
    }
}
