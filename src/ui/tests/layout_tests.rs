use ratatui::layout::Rect;

use crate::components::{ComponentKind, PluginViewId};

use super::super::layout::*;
use crate::ui::pane_frame::{Border, FrameSpec, PaneFrame, Title, TitleAlign};

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

// ---- frames

fn first_placement(source: &str) -> Placement {
    let tree = LayoutTree::from_ron(source).unwrap();
    tree.resolve(Rect::new(0, 0, 100, 40)).remove(0)
}

#[test]
fn a_plain_leaf_gets_its_components_default_frame() {
    let placement = first_placement(
        "Split(direction: Horizontal, children: [(size: Fill, node: Leaf(Editor))])",
    );

    assert_eq!(
        placement.frame,
        PaneFrame::default_for(&ComponentKind::Editor)
    );
}

#[test]
fn a_framed_leaf_overrides_what_it_names() {
    let source = r#"
        Split(direction: Horizontal, children: [
            (size: Fill, node: Framed(
                component: Editor,
                frame: (border: Rounded, title: Text("Code"), title_align: Center),
            )),
        ])
    "#;

    let placement = first_placement(source);

    let expected = PaneFrame::resolve(
        &FrameSpec {
            border: Some(Border::Rounded),
            title: Some(Title::Text("Code".into())),
            title_align: Some(TitleAlign::Center),
            ..FrameSpec::default()
        },
        &ComponentKind::Editor,
    );
    assert_eq!(placement.frame, expected);
    assert_eq!(placement.kind, ComponentKind::Editor);
}

#[test]
fn the_frame_can_be_empty_and_take_every_default() {
    let source = "Split(direction: Horizontal, children: [(size: Fill, node: Framed(component: Editor, frame: ()))])";

    let placement = first_placement(source);

    assert_eq!(
        placement.frame,
        PaneFrame::default_for(&ComponentKind::Editor)
    );
}

#[test]
fn the_frame_field_itself_may_be_left_out() {
    let source =
        "Split(direction: Horizontal, children: [(size: Fill, node: Framed(component: Editor))])";

    assert!(LayoutTree::from_ron(source).is_ok());
}

#[test]
fn style_slots_are_plain_names() {
    let source = r#"
        Split(direction: Horizontal, children: [
            (size: Fill, node: Framed(
                component: Editor,
                frame: (title_slot: "files.title", border_slot: "files.border"),
            )),
        ])
    "#;

    assert!(LayoutTree::from_ron(source).is_ok());
}

#[test]
fn an_unknown_border_or_title_kind_is_rejected() {
    let bad_border = "Framed(component: Editor, frame: (border: Wavy))";
    let bad_title = "Framed(component: Editor, frame: (title: Loud))";
    let bad_field = "Framed(component: Editor, frame: (colour: Red))";

    for source in [bad_border, bad_title, bad_field] {
        assert!(
            matches!(LayoutTree::from_ron(source), Err(LayoutError::Parse(_))),
            "{source}"
        );
    }
}

#[test]
fn a_framed_editor_satisfies_the_editor_requirement() {
    let ok = "Framed(component: Editor, frame: (border: Off))";
    let missing = "Framed(component: Terminal, frame: (border: Off))";

    assert!(LayoutTree::from_ron(ok).is_ok());
    assert!(matches!(
        LayoutTree::from_ron(missing),
        Err(LayoutError::MissingEditor)
    ));
}
