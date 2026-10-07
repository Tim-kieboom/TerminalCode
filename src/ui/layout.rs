use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::Deserialize;
use thiserror::Error;

use crate::component::ComponentKind;

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
    size: Size,
    node: LayoutNode,
}
impl Child {
    pub(crate) fn new(size: Size, node: LayoutNode) -> Self {
        Self { size, node }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) enum LayoutNode {
    Leaf(ComponentKind),
    Split {
        direction: Axis,
        children: Vec<Child>,
    },
}
impl LayoutNode {
    pub(crate) fn leaf(component: ComponentKind) -> Self {
        Self::Leaf(component)
    }

    pub(crate) fn split(direction: Axis, children: Vec<Child>) -> Self {
        Self::Split {
            direction,
            children,
        }
    }
}

/// Where a component ended up on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) kind: ComponentKind,
    pub(crate) area: Rect,
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
        let root: LayoutNode = ron::from_str(source)?;
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

    pub(crate) fn resolve(&self, area: Rect) -> Vec<Placement> {
        let mut placements = Vec::new();
        place(&self.root, area, &mut placements);
        placements
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::from_ron(DEFAULT_LAYOUT_RON)
            .expect("defaults/default_layout.ron must be a valid layout")
    }
}

fn validate(node: &LayoutNode, has_editor: &mut bool) -> Result<(), LayoutError> {
    match node {
        LayoutNode::Leaf(component) => {
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

fn place(node: &LayoutNode, area: Rect, out: &mut Vec<Placement>) {
    match node {
        LayoutNode::Leaf(component) => out.push(Placement {
            kind: component.clone(),
            area,
        }),
        LayoutNode::Split {
            direction,
            children,
        } => {
            let constraints = children.iter().map(|child| Constraint::from(child.size));
            let areas = Layout::default()
                .direction(Direction::from(*direction))
                .constraints(constraints)
                .split(area);
            for (child, child_area) in children.iter().zip(areas.iter()) {
                place(&child.node, *child_area, out);
            }
        }
    }
}
