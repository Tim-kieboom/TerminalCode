use std::fmt;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::Deserialize;
use thiserror::Error;

use crate::ComponentKind;
use crate::pane_frame::{BorderStyle, FrameSpec, PaneFrame, Side, Title, TitleAlign};

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
    #[error("{path}: a bar needs at least one view")]
    EmptyBar { path: LayoutPath },
    #[error(
        "{path}: `{view}` cannot be a view of a bar, only Explorer, Terminal and Plugin views can"
    )]
    NotABarView { path: LayoutPath, view: Box<str> },
    #[error("{path}: `{view}` is named twice; a view is in one place in the layout")]
    DuplicateView { path: LayoutPath, view: Box<str> },
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
pub enum Axis {
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
pub struct PaneSpec {
    pub(super) view: ComponentKind,
    pub(super) frame: FrameSpec,
    /// Whether the component starts hidden (`hidden: true`) or shown
    /// (`hidden: false`), when it is not remembered from the last run. Left
    /// out, a component starts the way it always did.
    pub(super) hidden: Option<bool>,
}

/// The fields of a `Pane(..)` as the file spells them, flat. They are read here
/// and not through `#[serde(flatten)]` on a [`FrameSpec`], which would stop a
/// misspelled field from being an error.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaneFields {
    view: ComponentKind,
    hidden: Option<bool>,
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
            hidden: fields.hidden,
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

/// What the layout file says about a bar: a `SideBar` or a `BottomBar`, which
/// holds a list of views and shows one of them at a time, with a tab strip in its
/// title when it has more than one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "BarFields")]
pub struct BarSpec {
    pub(super) views: Vec<ComponentKind>,
    pub(super) frame: FrameSpec,
    /// Whether the bar starts hidden, when that is not remembered; the first
    /// view is the one it starts with.
    pub(super) hidden: Option<bool>,
}

/// The fields of a `SideBar(..)` or `BottomBar(..)` as the file spells them.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BarFields {
    views: Vec<ComponentKind>,
    hidden: Option<bool>,
    sides: Option<Vec<Side>>,
    border: Option<BorderStyle>,
    title: Option<Title>,
    title_align: Option<TitleAlign>,
    title_slot: Option<Box<str>>,
    border_slot: Option<Box<str>>,
}

impl From<BarFields> for BarSpec {
    fn from(fields: BarFields) -> Self {
        Self {
            views: fields.views,
            hidden: fields.hidden,
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

/// Which views a bar has and which one it shows, for the tab strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarTabs {
    pub views: Vec<ComponentKind>,
    /// What each tab says, in the order of `views`: the view's name, which the
    /// app may add to (the count on the Errors tab).
    pub labels: Vec<String>,
    pub active: usize,
}

/// What a layout is resolved against: which components are shown, and which view
/// each bar shows (the bar is named by its first view).
pub struct Scene<'a> {
    pub visible: &'a dyn Fn(&ComponentKind) -> bool,
    pub active: &'a dyn Fn(&ComponentKind) -> usize,
}

impl BarSpec {
    /// The view the bar shows given the index the app remembers for it.
    fn shown_view(&self, scene: &Scene<'_>) -> usize {
        let key = &self.views[0];
        (scene.active)(key).min(self.views.len() - 1)
    }
}

/// A node of the layout tree. `Fixed` and `Percent` give the node they wrap a
/// size inside a `Row` or `Col`; a node without one fills what is left.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum LayoutNode {
    /// Children side by side.
    Row(Vec<LayoutNode>),
    /// Children stacked.
    Col(Vec<LayoutNode>),
    /// The node takes this many cells of its parent's direction.
    Fixed(u16, Box<LayoutNode>),
    /// The node takes this percentage of its parent's direction.
    Percent(u16, Box<LayoutNode>),
    Pane(PaneSpec),
    /// A bar on the side of the editor, with views such as the explorer.
    SideBar(BarSpec),
    /// A bar under the editor, with views such as the terminal.
    BottomBar(BarSpec),
    /// A bar to the right of the editor, for views that want width and to stay in
    /// sight while the text is edited, such as a plugin's preview. It is left out when
    /// the screen is narrower than [`RIGHT_BAR_MIN_WIDTH`].
    RightBar(BarSpec),
}

impl LayoutNode {
    fn name(&self) -> &'static str {
        match self {
            Self::Row(_) => "Row",
            Self::Col(_) => "Col",
            Self::Fixed(..) => "Fixed",
            Self::Percent(..) => "Percent",
            Self::Pane(_) => "Pane",
            Self::SideBar(_) => "SideBar",
            Self::BottomBar(_) => "BottomBar",
            Self::RightBar(_) => "RightBar",
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
    fn has_visible(&self, scene: &Scene<'_>) -> bool {
        match self.content() {
            Self::Pane(pane) => (scene.visible)(&pane.view),
            Self::SideBar(bar) | Self::BottomBar(bar) | Self::RightBar(bar) => {
                (scene.visible)(&bar.views[bar.shown_view(scene)])
            }
            Self::Row(children) | Self::Col(children) => {
                children.iter().any(|child| child.has_visible(scene))
            }
            Self::Fixed(..) | Self::Percent(..) => false,
        }
    }

    fn starts_hidden(&self, kind: &ComponentKind) -> Option<bool> {
        match self.content() {
            Self::Pane(pane) if pane.view == *kind => pane.hidden,
            Self::SideBar(bar) | Self::BottomBar(bar) | Self::RightBar(bar)
                if bar.views[0] == *kind =>
            {
                bar.hidden
            }
            Self::Pane(_) | Self::SideBar(_) | Self::BottomBar(_) | Self::RightBar(_) => None,
            Self::Fixed(..) | Self::Percent(..) => None,
            Self::Row(children) | Self::Col(children) => {
                children.iter().find_map(|child| child.starts_hidden(kind))
            }
        }
    }

    fn contains(&self, kind: &ComponentKind) -> bool {
        match self.content() {
            Self::Pane(pane) => pane.view == *kind,
            Self::SideBar(bar) | Self::BottomBar(bar) | Self::RightBar(bar) => {
                bar.views.contains(kind)
            }
            Self::Row(children) | Self::Col(children) => {
                children.iter().any(|child| child.contains(kind))
            }
            Self::Fixed(..) | Self::Percent(..) => false,
        }
    }

    /// Adds `view` at the end of the bar that has `anchor` among its views.
    /// Returns whether there was such a bar.
    fn push_to_bar_of(&mut self, anchor: &ComponentKind, view: &ComponentKind) -> bool {
        match self {
            Self::SideBar(bar) | Self::BottomBar(bar) | Self::RightBar(bar)
                if bar.views.contains(anchor) =>
            {
                bar.views.push(view.clone());
                true
            }
            Self::Row(children) | Self::Col(children) => children
                .iter_mut()
                .any(|child| child.push_to_bar_of(anchor, view)),
            Self::Fixed(_, node) | Self::Percent(_, node) => node.push_to_bar_of(anchor, view),
            _ => false,
        }
    }

    /// The first view of the first bar to the right of the editor.
    fn right_bar_anchor(&self) -> Option<ComponentKind> {
        match self.content() {
            Self::RightBar(bar) => bar.views.first().cloned(),
            Self::Row(children) | Self::Col(children) => {
                children.iter().find_map(|child| child.right_bar_anchor())
            }
            _ => None,
        }
    }

    /// Replaces the editor's pane by a row of it and `bar`, which takes `percent` of
    /// the width. Returns whether there was an editor pane.
    fn put_beside_editor(&mut self, bar: LayoutNode, percent: u16) -> bool {
        match self {
            Self::Pane(pane) if pane.view == ComponentKind::Editor => {
                let editor = std::mem::replace(self, Self::Row(Vec::new()));
                *self = Self::Row(vec![editor, Self::Percent(percent, Box::new(bar))]);
                true
            }
            Self::Row(children) | Self::Col(children) => children
                .iter_mut()
                .any(|child| child.put_beside_editor(bar.clone(), percent)),
            Self::Fixed(_, node) | Self::Percent(_, node) => node.put_beside_editor(bar, percent),
            _ => false,
        }
    }

    /// The views of the bar that has `kind` among them.
    fn bar_of(&self, kind: &ComponentKind) -> Option<&[ComponentKind]> {
        match self.content() {
            Self::SideBar(bar) | Self::BottomBar(bar) | Self::RightBar(bar)
                if bar.views.contains(kind) =>
            {
                Some(&bar.views)
            }
            Self::Row(children) | Self::Col(children) => {
                children.iter().find_map(|child| child.bar_of(kind))
            }
            _ => None,
        }
    }
}

/// The narrowest screen, in columns, a [`LayoutNode::RightBar`] is drawn on: on a
/// narrower one the editor keeps the room.
pub const RIGHT_BAR_MIN_WIDTH: u16 = 100;

/// Where a component ended up on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub kind: ComponentKind,
    pub area: Rect,
    pub frame: PaneFrame,
    /// The views of the bar this component is the shown one of, when it has
    /// more than one (the tab strip is drawn for them).
    pub bar: Option<BarTabs>,
}

impl Placement {
    /// A placement with the component's default frame.
    pub fn new(kind: ComponentKind, area: Rect) -> Self {
        let frame = PaneFrame::default_for(&kind);
        Self {
            kind,
            area,
            frame,
            bar: None,
        }
    }
}

/// Built-in layout, embedded at compile time.
pub const DEFAULT_LAYOUT_RON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_layout.ron"
));

/// Declarative layout. Today it only has a built-in default; a settings file
/// can produce the same structure after 0.1.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutTree {
    pub(super) root: LayoutNode,
}
impl LayoutTree {
    /// Parses and validates a layout described as RON.
    pub fn from_ron(source: &str) -> Result<Self, LayoutError> {
        let root: LayoutNode = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME)
            .with_default_extension(ron::extensions::Extensions::UNWRAP_VARIANT_NEWTYPES)
            .from_str(source)?;
        Self::new(root)
    }

    /// Rejects layouts the editor cannot work with.
    pub fn new(root: LayoutNode) -> Result<Self, LayoutError> {
        let mut path = vec![Step {
            name: root.name(),
            index: None,
        }];
        if root.is_sized() {
            return Err(LayoutError::RootSize {
                path: LayoutPath(path),
            });
        }

        let mut checks = Checks {
            has_editor: false,
            bar_views: bar_views_of(&root),
            seen_in_bars: Vec::new(),
        };
        validate(&root, &mut path, &mut checks)?;
        if !checks.has_editor {
            return Err(LayoutError::MissingEditor);
        }
        Ok(Self { root })
    }

    /// Whether the layout has a component of this kind.
    pub fn contains(&self, kind: &ComponentKind) -> bool {
        self.root.contains(kind)
    }

    /// The views of the bar that holds `kind`, in the order the layout lists them;
    /// `None` when `kind` is not in a bar.
    pub fn bar_views(&self, kind: &ComponentKind) -> Option<&[ComponentKind]> {
        self.root.bar_of(kind)
    }

    /// This layout with `view` added to the bar that holds `anchor`, the layout
    /// itself being left as it is. `None` when no bar holds `anchor`, or the layout
    /// already has `view` somewhere (a view is in one place).
    pub fn with_view_in_bar_of(
        &self,
        anchor: &ComponentKind,
        view: &ComponentKind,
    ) -> Option<LayoutTree> {
        if self.root.contains(view) {
            return None;
        }
        let mut root = self.root.clone();
        root.push_to_bar_of(anchor, view).then_some(Self { root })
    }

    /// The first view of a bar to the right of the editor, when the layout has one.
    pub fn right_bar_anchor(&self) -> Option<ComponentKind> {
        self.root.right_bar_anchor()
    }

    /// This layout with `view` in a bar to the right of the editor, the layout itself
    /// being left as it is: in the bar the layout has there, or in a new one that takes
    /// `percent` of the width of the editor's row. `None` when the layout already has
    /// `view` somewhere.
    pub fn with_view_beside_editor(
        &self,
        view: &ComponentKind,
        percent: u16,
    ) -> Option<LayoutTree> {
        if self.root.contains(view) {
            return None;
        }
        if let Some(anchor) = self.right_bar_anchor() {
            return self.with_view_in_bar_of(&anchor, view);
        }
        let mut root = self.root.clone();
        let bar = LayoutNode::RightBar(BarSpec {
            views: vec![view.clone()],
            frame: FrameSpec::default(),
            hidden: None,
        });
        root.put_beside_editor(bar, percent)
            .then_some(Self { root })
    }

    /// What the layout says about the component of this kind starting hidden,
    /// `None` when it has no such component or does not say.
    pub fn starts_hidden(&self, kind: &ComponentKind) -> Option<bool> {
        self.root.starts_hidden(kind)
    }

    /// Where every component goes.
    pub fn resolve(&self, area: Rect) -> Vec<Placement> {
        self.resolve_visible(area, &|_| true)
    }

    /// Where the components for which `visible` says yes go. A hidden
    /// component takes no space: the others in its split share it, and a split
    /// whose components are all hidden takes none either.
    pub fn resolve_visible(
        &self,
        area: Rect,
        visible: &dyn Fn(&ComponentKind) -> bool,
    ) -> Vec<Placement> {
        self.resolve_scene(
            area,
            &Scene {
                visible,
                active: &|_| 0,
            },
        )
    }

    /// Where the components of `scene` go: those it shows, and for a bar the
    /// view it shows.
    pub fn resolve_scene(&self, area: Rect, scene: &Scene<'_>) -> Vec<Placement> {
        let mut placements = Vec::new();
        let wide = area.width >= RIGHT_BAR_MIN_WIDTH;
        place(&self.root, area, scene, wide, &mut placements);
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

/// What checking a layout collects as it goes.
struct Checks {
    has_editor: bool,
    /// Every view some bar lists, for a pane that shows one of them again.
    bar_views: Vec<ComponentKind>,
    /// The views of the bars checked so far, for a view listed twice.
    seen_in_bars: Vec<ComponentKind>,
}

/// Every view the bars of `node` list.
fn bar_views_of(node: &LayoutNode) -> Vec<ComponentKind> {
    match node.content() {
        LayoutNode::SideBar(bar) | LayoutNode::BottomBar(bar) | LayoutNode::RightBar(bar) => {
            bar.views.clone()
        }
        LayoutNode::Row(children) | LayoutNode::Col(children) => {
            children.iter().flat_map(bar_views_of).collect()
        }
        _ => Vec::new(),
    }
}

/// Checks `node`, whose own step is the last one of `path`.
fn validate(
    node: &LayoutNode,
    path: &mut Vec<Step>,
    checks: &mut Checks,
) -> Result<(), LayoutError> {
    match node {
        LayoutNode::Pane(pane) => {
            checks.has_editor |= pane.view == ComponentKind::Editor;
            if checks.bar_views.contains(&pane.view) {
                return Err(LayoutError::DuplicateView {
                    path: LayoutPath(path.clone()),
                    view: pane.view.title().into(),
                });
            }
            Ok(())
        }
        LayoutNode::SideBar(bar) | LayoutNode::BottomBar(bar) | LayoutNode::RightBar(bar) => {
            if bar.views.is_empty() {
                return Err(LayoutError::EmptyBar {
                    path: LayoutPath(path.clone()),
                });
            }
            if let Some(view) = bar.views.iter().find(|view| {
                matches!(
                    view,
                    ComponentKind::Editor | ComponentKind::StatusBar | ComponentKind::MenuBar
                )
            }) {
                return Err(LayoutError::NotABarView {
                    path: LayoutPath(path.clone()),
                    view: view.title().into(),
                });
            }
            for view in &bar.views {
                if checks.seen_in_bars.contains(view) {
                    return Err(LayoutError::DuplicateView {
                        path: LayoutPath(path.clone()),
                        view: view.title().into(),
                    });
                }
                checks.seen_in_bars.push(view.clone());
            }
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
            validate_child(inner, None, path, checks)
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
                validate_child(child, Some(index), path, checks)?;
            }
            Ok(())
        }
    }
}

fn validate_child(
    child: &LayoutNode,
    index: Option<usize>,
    path: &mut Vec<Step>,
    checks: &mut Checks,
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
    validate(child, path, checks)?;
    path.pop();
    Ok(())
}

fn place(node: &LayoutNode, area: Rect, scene: &Scene<'_>, wide: bool, out: &mut Vec<Placement>) {
    match node.content() {
        LayoutNode::Pane(pane) if !(scene.visible)(&pane.view) => {}
        LayoutNode::Pane(pane) => out.push(Placement {
            kind: pane.view.clone(),
            area,
            frame: PaneFrame::resolve(&pane.frame, &pane.view),
            bar: None,
        }),
        LayoutNode::SideBar(bar) | LayoutNode::BottomBar(bar) | LayoutNode::RightBar(bar) => {
            let active = bar.shown_view(scene);
            let kind = &bar.views[active];
            if !(scene.visible)(kind) {
                return;
            }
            let tabbed = bar.views.len() > 1;
            let mut frame = PaneFrame::resolve(&bar.frame, kind);
            if tabbed {
                frame = frame.with_tab_row();
            }
            out.push(Placement {
                kind: kind.clone(),
                area,
                frame,
                bar: tabbed.then(|| BarTabs {
                    views: bar.views.clone(),
                    labels: bar
                        .views
                        .iter()
                        .map(|view| view.title().to_owned())
                        .collect(),
                    active,
                }),
            });
        }
        LayoutNode::Row(children) => {
            place_children(Direction::Horizontal, children, area, scene, wide, out)
        }
        LayoutNode::Col(children) => {
            place_children(Direction::Vertical, children, area, scene, wide, out)
        }
        // `content` has taken the size wrapper off.
        LayoutNode::Fixed(..) | LayoutNode::Percent(..) => {}
    }
}

fn place_children(
    direction: Direction,
    children: &[LayoutNode],
    area: Rect,
    scene: &Scene<'_>,
    wide: bool,
    out: &mut Vec<Placement>,
) {
    let shown: Vec<&LayoutNode> = children
        .iter()
        .filter(|child| child.has_visible(scene))
        // A bar to the right of the editor gives way on a narrow screen.
        .filter(|child| wide || !matches!(child.content(), LayoutNode::RightBar(_)))
        .collect();

    let areas = Layout::default()
        .direction(direction)
        .constraints(shown.iter().map(|child| child.constraint()))
        .split(area);

    for (child, child_area) in shown.iter().zip(areas.iter()) {
        place(child, *child_area, scene, wide, out);
    }
}
