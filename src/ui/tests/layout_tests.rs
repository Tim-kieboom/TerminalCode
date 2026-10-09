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

fn placed(tree: &LayoutTree, hidden: &[ComponentKind]) -> Vec<Placement> {
    tree.resolve_visible(Rect::new(0, 0, 100, 40), &|kind| !hidden.contains(kind))
}

fn area_of(placements: &[Placement], kind: &ComponentKind) -> Rect {
    placements.iter().find(|p| &p.kind == kind).unwrap().area
}

#[test]
fn resolving_with_everything_visible_is_the_same_as_resolve() {
    let tree = LayoutTree::default();

    assert_eq!(placed(&tree, &[]), tree.resolve(Rect::new(0, 0, 100, 40)));
}

#[test]
fn a_hidden_component_is_not_placed_and_its_space_goes_to_its_neighbors() {
    let tree = LayoutTree::default();
    let before = placed(&tree, &[]);

    let after = placed(&tree, &[ComponentKind::Explorer]);

    assert!(after.iter().all(|p| p.kind != ComponentKind::Explorer));
    let editor = |placements: &[Placement]| area_of(placements, &ComponentKind::Editor);
    assert_eq!(editor(&after).width, editor(&before).width + 30);
    assert_eq!(editor(&after).x, 0);
}

#[test]
fn hiding_the_status_bar_gives_its_row_to_the_editor_column() {
    let tree = LayoutTree::default();
    let before = placed(&tree, &[]);

    let after = placed(&tree, &[ComponentKind::StatusBar]);

    let terminal = |placements: &[Placement]| area_of(placements, &ComponentKind::Terminal);
    assert_eq!(terminal(&after).bottom(), 40);
    assert_eq!(terminal(&before).bottom(), 39);
}

#[test]
fn a_split_whose_components_are_all_hidden_takes_no_space() {
    let tree = LayoutTree::new(new_split(
        Axis::Horizontal,
        vec![
            new_child(Size::Fixed(10), new_leaf(ComponentKind::Explorer)),
            new_child(
                Size::Fixed(20),
                new_split(
                    Axis::Vertical,
                    vec![
                        new_child(Size::Fill, new_leaf(ComponentKind::Terminal)),
                        new_child(Size::Fixed(1), new_leaf(ComponentKind::StatusBar)),
                    ],
                ),
            ),
            new_child(Size::Fill, editor_only()),
        ],
    ))
    .unwrap();

    let after = placed(&tree, &[ComponentKind::Terminal, ComponentKind::StatusBar]);

    assert_eq!(area_of(&after, &ComponentKind::Explorer).width, 10);
    assert_eq!(area_of(&after, &ComponentKind::Editor).width, 90);
}

#[test]
fn a_hidden_framed_component_is_not_placed_either() {
    let tree = LayoutTree::from_ron(
        "Split(direction: Horizontal, children: [
            (size: Fixed(10), node: Framed(component: Explorer, frame: (border: Rounded))),
            (size: Fill, node: Leaf(Editor)),
        ])",
    )
    .unwrap();

    let after = placed(&tree, &[ComponentKind::Explorer]);

    assert_eq!(after.len(), 1);
    assert_eq!(after[0].area, Rect::new(0, 0, 100, 40));
}

#[test]
fn hidden_plugin_views_are_left_out_by_their_id() {
    let id = PluginViewId::new("test.view");
    let kind = ComponentKind::Plugin(id.clone());
    let tree = LayoutTree::new(new_split(
        Axis::Horizontal,
        vec![
            new_child(Size::Fill, editor_only()),
            new_child(Size::Fixed(20), new_leaf(kind.clone())),
        ],
    ))
    .unwrap();

    assert_eq!(placed(&tree, &[]).len(), 2);
    assert_eq!(placed(&tree, &[kind]).len(), 1);
    assert_eq!(placed(&tree, &[ComponentKind::Explorer]).len(), 2);
}
