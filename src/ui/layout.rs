use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::Deserialize;
use thiserror::Error;

use crate::components::ComponentKind;
use crate::ui::pane_frame::{FrameSpec, PaneFrame};

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("layout is not valid RON: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("layout has no editor component")]
    MissingEditor,
    #[error("layout contains a split without children")]
    EmptySplit,
    #[error("percentage size must be at most 100, got {0}")]
    InvalidPercent(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum Size {
    Fixed(u16),
    Percent(u16),
    Fill,
}

impl From<Size> for Constraint {
    fn from(size: Size) -> Self {
        match size {
            Size::Fixed(cells) => Constraint::Length(cells),
            Size::Percent(percent) => Constraint::Percentage(percent),
            Size::Fill => Constraint::Fill(1),
        }
    }
}

/// Direction in which a split lays out its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum Axis {
    Horizontal,
    Vertical,
}

impl From<Axis> for Direction {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::Horizontal => Direction::Horizontal,
            Axis::Vertical => Direction::Vertical,
        }
    }
}

/// A child of a split together with the space it asks for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Child {
    pub(super) size: Size,
    pub(super) node: LayoutNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) enum LayoutNode {
    Leaf(ComponentKind),
    /// A component with its frame (border and title) set explicitly. What the
    /// spec leaves out takes the component's default.
    Framed {
        component: ComponentKind,
        #[serde(default)]
        frame: FrameSpec,
    },
    Split {
        direction: Axis,
        children: Vec<Child>,
    },
}

impl LayoutNode {
    /// Whether this node has a component that is shown.
    fn has_visible(&self, visible: &dyn Fn(&ComponentKind) -> bool) -> bool {
        match self {
            Self::Leaf(component) | Self::Framed { component, .. } => visible(component),
            Self::Split { children, .. } => {
                children.iter().any(|child| child.node.has_visible(visible))
            }
        }
    }

    fn contains(&self, kind: &ComponentKind) -> bool {
        match self {
            Self::Leaf(component) | Self::Framed { component, .. } => component == kind,
            Self::Split { children, .. } => children.iter().any(|child| child.node.contains(kind)),
        }
    }
}

/// Where a component ended up on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) kind: ComponentKind,
    pub(crate) area: Rect,
    pub(crate) frame: PaneFrame,
}

impl Placement {
    /// A placement with the component's default frame.
    pub(crate) fn new(kind: ComponentKind, area: Rect) -> Self {
        let frame = PaneFrame::default_for(&kind);
        Self { kind, area, frame }
    }
}

/// Built-in layout, embedded at compile time.
const DEFAULT_LAYOUT_RON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_layout.ron"
));

/// Declarative layout. Today it only has a built-in default; a settings file
/// can produce the same structure after 0.1.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayoutTree {
    pub(super) root: LayoutNode,
}
impl LayoutTree {
    /// Parses and validates a layout described as RON.
    pub(crate) fn from_ron(source: &str) -> Result<Self, LayoutError> {
        let root: LayoutNode = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .from_str(source)?;
        Self::new(root)
    }

    /// Rejects layouts the editor cannot work with.
    pub(crate) fn new(root: LayoutNode) -> Result<Self, LayoutError> {
        let mut has_editor = false;
        validate(&root, &mut has_editor)?;
        if !has_editor {
            return Err(LayoutError::MissingEditor);
        }
        Ok(Self { root })
    }

    /// Whether the layout has a component of this kind.
    pub(crate) fn contains(&self, kind: &ComponentKind) -> bool {
        self.root.contains(kind)
    }

    /// Where every component goes.
    #[cfg(test)]
    pub(crate) fn resolve(&self, area: Rect) -> Vec<Placement> {
        self.resolve_visible(area, &|_| true)
    }

    /// Where the components for which `visible` says yes go. A hidden
    /// component takes no space: the others in its split share it, and a split
    /// whose components are all hidden takes none either.
    pub(crate) fn resolve_visible(
        &self,
        area: Rect,
        visible: &dyn Fn(&ComponentKind) -> bool,
    ) -> Vec<Placement> {
        let mut placements = Vec::new();
        place(&self.root, area, visible, &mut placements);
        placements
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::from_ron(DEFAULT_LAYOUT_RON).unwrap_or_else(|err| {
            panic!("while tyring to parse defaults/default_layout.ron: {}", err)
        })
    }
}

fn validate(node: &LayoutNode, has_editor: &mut bool) -> Result<(), LayoutError> {
    match node {
        LayoutNode::Leaf(component) | LayoutNode::Framed { component, .. } => {
            *has_editor |= *component == ComponentKind::Editor;
            Ok(())
        }
        LayoutNode::Split { children, .. } => {
            if children.is_empty() {
                return Err(LayoutError::EmptySplit);
            }
            for child in children {
                if let Size::Percent(percent) = child.size
                    && percent > 100
                {
                    return Err(LayoutError::InvalidPercent(percent));
                }
                validate(&child.node, has_editor)?;
            }
            Ok(())
        }
    }
}

fn place(
    node: &LayoutNode,
    area: Rect,
    visible: &dyn Fn(&ComponentKind) -> bool,
    out: &mut Vec<Placement>,
) {
    match node {
        LayoutNode::Leaf(component) | LayoutNode::Framed { component, .. }
            if !visible(component) => {}

        LayoutNode::Leaf(component) => out.push(Placement::new(component.clone(), area)),
        LayoutNode::Framed { component, frame } => out.push(Placement {
            kind: component.clone(),
            area,
            frame: PaneFrame::resolve(frame, component),
        }),
        LayoutNode::Split {
            direction,
            children,
        } => {
            let shown: Vec<&Child> = children
                .iter()
                .filter(|child| child.node.has_visible(visible))
                .collect();

            let constraints = shown.iter().map(|child| Constraint::from(child.size));
            let areas = Layout::default()
                .direction(Direction::from(*direction))
                .constraints(constraints)
                .split(area);

            for (child, child_area) in shown.iter().zip(areas.iter()) {
                place(&child.node, *child_area, visible, out);
            }
        }
    }
}
