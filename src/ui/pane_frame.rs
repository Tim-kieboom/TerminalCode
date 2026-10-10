//! The frame drawn around a component: its border and its title.

use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, BorderType, Borders};
use serde::Deserialize;

use crate::components::ComponentKind;
use crate::ui::theme::Theme;

/// Theme slot of the border by default; the border of a focused pane uses
/// the slot with `.focused` appended when the theme has one.
const DEFAULT_BORDER_SLOT: &str = "pane.border";
const DEFAULT_TITLE_SLOT: &str = "pane.title";

/// An edge of a frame that has a border line. A layout file lists them in
/// `sides`; an empty list means no border at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum Side {
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
pub(crate) enum BorderStyle {
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
pub(crate) enum Title {
    /// Nothing is shown.
    Hidden,
    /// The component's own name: `Explorer`, the plugin's id, the file name
    /// for an editor.
    #[default]
    Name,
    Text(Box<str>),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub(crate) enum TitleAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What a layout file says about a component's frame. Anything left out takes
/// the component's default (see [`PaneFrame::default_for`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct FrameSpec {
    /// The edges that have a border line; empty for none.
    pub(crate) sides: Option<Vec<Side>>,
    pub(crate) border: Option<BorderStyle>,
    pub(crate) title: Option<Title>,
    pub(crate) title_align: Option<TitleAlign>,
    /// Theme slot that styles the title.
    pub(crate) title_slot: Option<Box<str>>,
    /// Theme slot that styles the border.
    pub(crate) border_slot: Option<Box<str>>,
}

/// A component's frame with everything filled in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneFrame {
    sides: Borders,
    border: BorderStyle,
    title: Title,
    title_align: TitleAlign,
    title_slot: Box<str>,
    border_slot: Box<str>,
}

impl PaneFrame {
    /// How a component is framed when the layout does not say: panes get a
    /// plain border and their name; the editor shows file names in its tabs
    /// instead of a title; the status bar is a bare line.
    pub(crate) fn default_for(kind: &ComponentKind) -> Self {
        let (sides, title) = match kind {
            ComponentKind::Editor => (Borders::ALL, Title::Hidden),
            ComponentKind::StatusBar => (Borders::NONE, Title::Hidden),
            _ => (Borders::ALL, Title::Name),
        };
        Self {
            sides,
            border: BorderStyle::default(),
            title,
            title_align: TitleAlign::Left,
            title_slot: DEFAULT_TITLE_SLOT.into(),
            border_slot: DEFAULT_BORDER_SLOT.into(),
        }
    }

    /// `spec` laid over the defaults of `kind`.
    pub(crate) fn resolve(spec: &FrameSpec, kind: &ComponentKind) -> Self {
        let defaults = Self::default_for(kind);
        Self {
            sides: spec.sides.as_deref().map_or(defaults.sides, borders_of),
            border: spec.border.unwrap_or(defaults.border),
            title: spec.title.clone().unwrap_or(defaults.title),
            title_align: spec.title_align.unwrap_or(defaults.title_align),
            title_slot: spec.title_slot.clone().unwrap_or(defaults.title_slot),
            border_slot: spec.border_slot.clone().unwrap_or(defaults.border_slot),
        }
    }

    #[cfg(test)]
    pub(crate) fn sides(&self) -> Borders {
        self.sides
    }

    #[cfg(test)]
    pub(crate) fn border(&self) -> BorderStyle {
        self.border
    }

    #[cfg(test)]
    pub(crate) fn title(&self) -> &Title {
        &self.title
    }

    #[cfg(test)]
    pub(crate) fn title_align(&self) -> TitleAlign {
        self.title_align
    }

    #[cfg(test)]
    pub(crate) fn title_slot(&self) -> &str {
        &self.title_slot
    }

    #[cfg(test)]
    pub(crate) fn border_slot(&self) -> &str {
        &self.border_slot
    }

    /// Whether the frame shows a title at all.
    pub(crate) fn shows_title(&self) -> bool {
        self.title != Title::Hidden
    }

    /// The title to draw, given the component's own `name`.
    pub(crate) fn title_text<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        match &self.title {
            Title::Hidden => None,
            Title::Name => Some(name),
            Title::Text(text) => Some(text),
        }
    }

    /// The frame as a widget. `title` is already resolved (see
    /// [`PaneFrame::title_text`]). A focused frame uses the `.focused`
    /// variant of its border slot when the theme defines one.
    pub(crate) fn block<'a>(
        &self,
        theme: &Theme,
        title: Option<&'a str>,
        focused: bool,
    ) -> Block<'a> {
        let mut block = self
            .shell()
            .border_style(theme.style(&self.border_style_slot(theme, focused)));

        if let Some(title) = title {
            block = block
                .title(title)
                .title_alignment(self.alignment())
                .title_style(theme.style(&self.title_slot));
        }
        block
    }

    /// The area left for the content inside the frame, as [`PaneFrame::block`]
    /// draws it. A title costs a row even without a border.
    pub(crate) fn inner(&self, area: Rect) -> Rect {
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
