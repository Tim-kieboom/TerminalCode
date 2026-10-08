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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub(crate) enum Border {
    /// No border: the content gets the whole area. (Not called `None`, which a
    /// layout file would read as "not set".)
    Off,
    #[default]
    Plain,
    Rounded,
    Double,
    Thick,
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
    pub(crate) border: Option<Border>,
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
    border: Border,
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
        let (border, title) = match kind {
            ComponentKind::Editor => (Border::Plain, Title::Hidden),
            ComponentKind::StatusBar => (Border::Off, Title::Hidden),
            _ => (Border::Plain, Title::Name),
        };
        Self {
            border,
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
            border: spec.border.unwrap_or(defaults.border),
            title: spec.title.clone().unwrap_or(defaults.title),
            title_align: spec.title_align.unwrap_or(defaults.title_align),
            title_slot: spec.title_slot.clone().unwrap_or(defaults.title_slot),
            border_slot: spec.border_slot.clone().unwrap_or(defaults.border_slot),
        }
    }

    #[cfg(test)]
    pub(crate) fn border(&self) -> Border {
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
        let borders = match self.border {
            Border::Off => Borders::NONE,
            _ => Borders::ALL,
        };
        let shell = Block::new().borders(borders);
        match self.border {
            Border::Off | Border::Plain => shell,
            Border::Rounded => shell.border_type(BorderType::Rounded),
            Border::Double => shell.border_type(BorderType::Double),
            Border::Thick => shell.border_type(BorderType::Thick),
        }
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
