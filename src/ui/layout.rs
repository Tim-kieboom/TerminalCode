use std::fmt;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::Deserialize;
use thiserror::Error;

use crate::components::ComponentKind;
use crate::ui::pane_frame::{BorderStyle, FrameSpec, PaneFrame, Side, Title, TitleAlign};

/// One step of a [`LayoutPath`]: a node and, when its parent holds several,
/// its position there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Step {
    name: &'static str,
    index: Option<usize>,
}

/// Where in the layout tree a problem is, from the root down:
/// `Col > Row[0] > Fixed[1] > Pane`. A step is a node and its position among
/// its siblings; a size wrapper has one child, which has no position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutPath(Vec<Step>);

impl fmt::Display for LayoutPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, step) in self.0.iter().enumerate() {
            if position > 0 {
                f.write_str(" > ")?;
            }
            f.write_str(step.name)?;
            if let Some(index) = step.index {
                write!(f, "[{index}]")?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("layout is not valid RON: {0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("layout has no editor component")]
    MissingEditor,
    #[error("{path}: a Row or Col needs at least one child")]
    EmptyContainer { path: LayoutPath },
    #[error("{path}: percentage size must be at most 100, got {percent}")]
    InvalidPercent { path: LayoutPath, percent: u16 },
    #[error("{path}: a size cannot wrap another size")]
    NestedSize { path: LayoutPath },
    #[error("{path}: the whole layout cannot have a size, only a child of a Row or Col can")]
    RootSize { path: LayoutPath },
    #[error(
        "{path}: {count} children have no size and would all fill the space; \
         at most one may, give the others Fixed or Percent"
    )]
    AmbiguousFill { path: LayoutPath, count: usize },
}

/// Direction in which panes are laid out; the workspace splits its editor panes along one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// What the layout file says about one component: which one, and its frame.
/// Anything left out of the frame takes the component's default.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "PaneFields")]
pub(crate) struct PaneSpec {
    pub(super) view: ComponentKind,
    pub(super) frame: FrameSpec,
}

/// The fields of a `Pane(..)` as the file spells them, flat. They are read here
/// and not through `#[serde(flatten)]` on a [`FrameSpec`], which would stop a
/// misspelled field from being an error.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaneFields {
    view: ComponentKind,
    sides: Option<Vec<Side>>,
    border: Option<BorderStyle>,
    title: Option<Title>,
    title_align: Option<TitleAlign>,
    title_slot: Option<Box<str>>,
    border_slot: Option<Box<str>>,
}

impl From<PaneFields> for PaneSpec {
    fn from(fields: PaneFields) -> Self {
        Self {
            view: fields.view,
            frame: FrameSpec {
                sides: fields.sides,
                border: fields.border,
                title: fields.title,
                title_align: fields.title_align,
                title_slot: fields.title_slot,
                border_slot: fields.border_slot,
            },
        }
    }
}

/// A node of the layout tree. `Fixed` and `Percent` give the node they wrap a
/// size inside a `Row` or `Col`; a node without one fills what is left.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) enum LayoutNode {
    /// Children side by side.
    Row(Vec<LayoutNode>),
    /// Children stacked.
    Col(Vec<LayoutNode>),
    /// The node takes this many cells of its parent's direction.
    Fixed(u16, Box<LayoutNode>),
    /// The node takes this percentage of its parent's direction.
    Percent(u16, Box<LayoutNode>),
    Pane(PaneSpec),
}

impl LayoutNode {
    fn name(&self) -> &'static str {
        match self {
            Self::Row(_) => "Row",
            Self::Col(_) => "Col",
            Self::Fixed(..) => "Fixed",
            Self::Percent(..) => "Percent",
            Self::Pane(_) => "Pane",
        }
    }

    fn is_sized(&self) -> bool {
        matches!(self, Self::Fixed(..) | Self::Percent(..))
    }

    /// The space this node asks of its parent.
    fn constraint(&self) -> Constraint {
        match self {
            Self::Fixed(cells, _) => Constraint::Length(*cells),
            Self::Percent(percent, _) => Constraint::Percentage(*percent),
            _ => Constraint::Fill(1),
        }
    }

    /// The node without its size wrapper.
    fn content(&self) -> &LayoutNode {
        match self {
            Self::Fixed(_, node) | Self::Percent(_, node) => node,
            other => other,
        }
    }

    /// Whether this node has a component that is shown.
    fn has_visible(&self, visible: &dyn Fn(&ComponentKind) -> bool) -> bool {
        match self.content() {
            Self::Pane(pane) => visible(&pane.view),
            Self::Row(children) | Self::Col(children) => {
                children.iter().any(|child| child.has_visible(visible))
            }
            Self::Fixed(..) | Self::Percent(..) => false,
        }
    }

    fn contains(&self, kind: &ComponentKind) -> bool {
        match self.content() {
            Self::Pane(pane) => pane.view == *kind,
            Self::Row(children) | Self::Col(children) => {
                children.iter().any(|child| child.contains(kind))
            }
            Self::Fixed(..) | Self::Percent(..) => false,
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
    #[cfg(test)]
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
            .with_default_extension(ron::extensions::Extensions::UNWRAP_VARIANT_NEWTYPES)
            .from_str(source)?;
        Self::new(root)
    }

    /// Rejects layouts the editor cannot work with.
    pub(crate) fn new(root: LayoutNode) -> Result<Self, LayoutError> {
        let mut path = vec![Step {
            name: root.name(),
            index: None,
        }];
        if root.is_sized() {
            return Err(LayoutError::RootSize {
                path: LayoutPath(path),
            });
        }

        let mut has_editor = false;
        validate(&root, &mut path, &mut has_editor)?;
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
            panic!("while trying to parse defaults/default_layout.ron: {}", err)
        })
    }
}

/// Checks `node`, whose own step is the last one of `path`.
fn validate(
    node: &LayoutNode,
    path: &mut Vec<Step>,
    has_editor: &mut bool,
) -> Result<(), LayoutError> {
    match node {
        LayoutNode::Pane(pane) => {
            *has_editor |= pane.view == ComponentKind::Editor;
            Ok(())
        }
        LayoutNode::Fixed(_, inner) | LayoutNode::Percent(_, inner) => {
            if let LayoutNode::Percent(percent, _) = node
                && *percent > 100
            {
                return Err(LayoutError::InvalidPercent {
                    path: LayoutPath(path.clone()),
                    percent: *percent,
                });
            }
            validate_child(inner, None, path, has_editor)
        }
        LayoutNode::Row(children) | LayoutNode::Col(children) => {
            if children.is_empty() {
                return Err(LayoutError::EmptyContainer {
                    path: LayoutPath(path.clone()),
                });
            }
            let filling = children.iter().filter(|child| !child.is_sized()).count();
            if filling > 1 {
                return Err(LayoutError::AmbiguousFill {
                    path: LayoutPath(path.clone()),
                    count: filling,
                });
            }
            for (index, child) in children.iter().enumerate() {
                validate_child(child, Some(index), path, has_editor)?;
            }
            Ok(())
        }
    }
}

fn validate_child(
    child: &LayoutNode,
    index: Option<usize>,
    path: &mut Vec<Step>,
    has_editor: &mut bool,
) -> Result<(), LayoutError> {
    let parent_is_size = index.is_none();
    path.push(Step {
        name: child.name(),
        index,
    });
    if parent_is_size && child.is_sized() {
        return Err(LayoutError::NestedSize {
            path: LayoutPath(path.clone()),
        });
    }
    validate(child, path, has_editor)?;
    path.pop();
    Ok(())
}

fn place(
    node: &LayoutNode,
    area: Rect,
    visible: &dyn Fn(&ComponentKind) -> bool,
    out: &mut Vec<Placement>,
) {
    match node.content() {
        LayoutNode::Pane(pane) if !visible(&pane.view) => {}
        LayoutNode::Pane(pane) => out.push(Placement {
            kind: pane.view.clone(),
            area,
            frame: PaneFrame::resolve(&pane.frame, &pane.view),
        }),
        LayoutNode::Row(children) => {
            place_children(Direction::Horizontal, children, area, visible, out)
        }
        LayoutNode::Col(children) => {
            place_children(Direction::Vertical, children, area, visible, out)
        }
        // `content` has taken the size wrapper off.
        LayoutNode::Fixed(..) | LayoutNode::Percent(..) => {}
    }
}

fn place_children(
    direction: Direction,
    children: &[LayoutNode],
    area: Rect,
    visible: &dyn Fn(&ComponentKind) -> bool,
    out: &mut Vec<Placement>,
) {
    let shown: Vec<&LayoutNode> = children
        .iter()
        .filter(|child| child.has_visible(visible))
        .collect();

    let areas = Layout::default()
        .direction(direction)
        .constraints(shown.iter().map(|child| child.constraint()))
        .split(area);

    for (child, child_area) in shown.iter().zip(areas.iter()) {
        place(child, *child_area, visible, out);
    }
}
