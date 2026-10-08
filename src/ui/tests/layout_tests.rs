use ratatui::layout::Rect;

use crate::components::{ComponentKind, PluginViewId};

use super::super::layout::*;

fn editor_only() -> LayoutNode {
    LayoutNode::Leaf(ComponentKind::Editor)
}

fn new_leaf(kind: ComponentKind) -> LayoutNode {
    LayoutNode::Leaf(kind)
}

fn new_child(size: Size, node: LayoutNode) -> Child {
    Child { size, node }
}

fn new_split(direction: Axis, children: Vec<Child>) -> LayoutNode {
    LayoutNode::Split {
        direction,
        children,
    }
}

#[test]
fn default_layout_is_valid() {
    let default = LayoutTree::default();

    let revalidated = LayoutTree::new(default.root.clone()).unwrap();

    assert_eq!(revalidated, default);
}

#[test]
fn default_layout_places_every_builtin_component() {
    let placements = LayoutTree::default().resolve(Rect::new(0, 0, 100, 40));

    let kinds: Vec<_> = placements.into_iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            ComponentKind::Explorer,
            ComponentKind::Editor,
            ComponentKind::Terminal,
            ComponentKind::StatusBar,
        ]
    );
}

#[test]
fn layout_without_editor_is_rejected() {
    let root = new_leaf(ComponentKind::Terminal);

    let result = LayoutTree::new(root);

    assert!(matches!(result, Err(LayoutError::MissingEditor)));
}

#[test]
fn empty_split_is_rejected() {
    let root = new_split(Axis::Horizontal, Vec::new());

    let result = LayoutTree::new(root);

    assert!(matches!(result, Err(LayoutError::EmptySplit)));
}

#[test]
fn percentage_above_hundred_is_rejected() {
    let root = new_split(
        Axis::Horizontal,
        vec![new_child(Size::Percent(101), editor_only())],
    );

    let result = LayoutTree::new(root);

    assert!(matches!(result, Err(LayoutError::InvalidPercent(101))));
}

#[test]
fn resolve_gives_fixed_sizes_and_fills_the_rest() {
    let root = new_split(
        Axis::Horizontal,
        vec![
            new_child(Size::Fixed(10), new_leaf(ComponentKind::Explorer)),
            new_child(Size::Fill, editor_only()),
        ],
    );
    let tree = LayoutTree::new(root).unwrap();

    let placements = tree.resolve(Rect::new(0, 0, 100, 20));

    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0].area, Rect::new(0, 0, 10, 20));
    assert_eq!(placements[1].area, Rect::new(10, 0, 90, 20));
}

#[test]
fn from_ron_accepts_a_plugin_component() {
    let source = r#"Split(
        direction: Horizontal,
        children: [
            (size: Percent(30), node: Leaf(Plugin("debugger.stack"))),
            (size: Fill, node: Leaf(Editor)),
        ],
    )"#;

    let tree = LayoutTree::from_ron(source).unwrap();

    let placements = tree.resolve(Rect::new(0, 0, 100, 10));
    assert_eq!(
        placements[0].kind,
        ComponentKind::Plugin(PluginViewId::new("debugger.stack"))
    );
}

#[test]
fn from_ron_rejects_malformed_input() {
    let result = LayoutTree::from_ron("Split((");

    assert!(matches!(result, Err(LayoutError::Parse(_))));
}

#[test]
fn from_ron_rejects_unknown_component() {
    let source = r#"Leaf(Minimap)"#;

    let result = LayoutTree::from_ron(source);

    assert!(matches!(result, Err(LayoutError::Parse(_))));
}

#[test]
fn from_ron_runs_validation() {
    let source = r#"Leaf(Terminal)"#;

    let result = LayoutTree::from_ron(source);

    assert!(matches!(result, Err(LayoutError::MissingEditor)));
}
