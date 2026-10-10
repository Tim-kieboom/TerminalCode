use ratatui::layout::Rect;

use crate::components::{ComponentKind, PluginViewId};

use super::super::layout::*;
use ratatui::widgets::Borders;

use crate::ui::pane_frame::{BorderStyle, FrameSpec, PaneFrame, Title, TitleAlign};

fn editor_only() -> LayoutNode {
    new_leaf(ComponentKind::Editor)
}

fn new_leaf(kind: ComponentKind) -> LayoutNode {
    LayoutNode::Pane(PaneSpec {
        view: kind,
        frame: FrameSpec::default(),
    })
}

fn fixed(cells: u16, node: LayoutNode) -> LayoutNode {
    LayoutNode::Fixed(cells, Box::new(node))
}

fn percent(percent: u16, node: LayoutNode) -> LayoutNode {
    LayoutNode::Percent(percent, Box::new(node))
}

/// The message of the error `source` is rejected with.
fn rejection(source: &str) -> String {
    LayoutTree::from_ron(source).unwrap_err().to_string()
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

// ---- rejections, each with the place in the tree it is in

#[test]
fn layout_without_editor_is_rejected() {
    let root = new_leaf(ComponentKind::Terminal);

    let result = LayoutTree::new(root);

    assert!(matches!(result, Err(LayoutError::MissingEditor)));
}

#[test]
fn an_empty_row_or_col_is_rejected_with_its_path() {
    assert_eq!(
        rejection("Col([Row([])])"),
        "Col > Row[0]: a Row or Col needs at least one child"
    );
}

#[test]
fn percentage_above_hundred_is_rejected_with_its_path() {
    let message = rejection("Row([Percent(101, Pane(view: Editor))])");

    assert_eq!(
        message,
        "Row > Percent[0]: percentage size must be at most 100, got 101"
    );
}

#[test]
fn two_children_without_a_size_are_rejected_at_their_parent() {
    let message = rejection("Col([Row([Pane(view: Explorer), Pane(view: Editor)])])");

    assert!(
        message.starts_with("Col > Row[0]: 2 children have no size"),
        "{message}"
    );
}

#[test]
fn a_child_that_is_itself_a_row_counts_as_without_a_size() {
    let message = rejection("Row([Row([Pane(view: Editor)]), Pane(view: Explorer)])");

    assert!(
        message.starts_with("Row: 2 children have no size"),
        "{message}"
    );
}

#[test]
fn one_child_without_a_size_is_fine_however_many_have_one() {
    let source = "Row([Fixed(5, Pane(view: Explorer)), Percent(20, Pane(view: Terminal)), Pane(view: Editor)])";

    assert!(LayoutTree::from_ron(source).is_ok());
}

#[test]
fn a_size_inside_a_size_is_rejected_with_its_path() {
    let message = rejection("Row([Fixed(5, Fixed(5, Pane(view: Editor)))])");

    assert_eq!(
        message,
        "Row > Fixed[0] > Fixed: a size cannot wrap another size"
    );
}

#[test]
fn a_size_on_the_whole_layout_is_rejected() {
    let message = rejection("Fixed(30, Pane(view: Editor))");

    assert_eq!(
        message,
        "Fixed: the whole layout cannot have a size, only a child of a Row or Col can"
    );
}

#[test]
fn the_path_names_the_position_of_each_step() {
    let message = rejection(
        "Col([Fixed(1, Pane(view: StatusBar)), Row([Fixed(5, Pane(view: Editor)), Col([])])])",
    );

    assert_eq!(
        message,
        "Col > Row[1] > Col[1]: a Row or Col needs at least one child"
    );
}

#[test]
fn the_old_syntax_is_a_parse_error() {
    let old = "Split(direction: Horizontal, children: [(size: Fill, node: Leaf(Editor))])";

    assert!(matches!(
        LayoutTree::from_ron(old),
        Err(LayoutError::Parse(_))
    ));
}

#[test]
fn resolve_gives_fixed_sizes_and_fills_the_rest() {
    let root = LayoutNode::Row(vec![
        fixed(10, new_leaf(ComponentKind::Explorer)),
        editor_only(),
    ]);
    let tree = LayoutTree::new(root).unwrap();

    let placements = tree.resolve(Rect::new(0, 0, 100, 20));

    assert_eq!(placements.len(), 2);
    assert_eq!(placements[0].area, Rect::new(0, 0, 10, 20));
    assert_eq!(placements[1].area, Rect::new(10, 0, 90, 20));
}

#[test]
fn a_percent_child_takes_that_share() {
    let root = LayoutNode::Row(vec![
        percent(30, new_leaf(ComponentKind::Explorer)),
        editor_only(),
    ]);
    let tree = LayoutTree::new(root).unwrap();

    let placements = tree.resolve(Rect::new(0, 0, 100, 20));

    assert_eq!(placements[0].area.width, 30);
    assert_eq!(placements[1].area.width, 70);
}

#[test]
fn from_ron_accepts_a_plugin_component() {
    let source = r#"Row([
        Percent(30, Pane(view: Plugin("debugger.stack"))),
        Pane(view: Editor),
    ])"#;

    let tree = LayoutTree::from_ron(source).unwrap();

    let placements = tree.resolve(Rect::new(0, 0, 100, 10));
    assert_eq!(
        placements[0].kind,
        ComponentKind::Plugin(PluginViewId::new("debugger.stack"))
    );
}

#[test]
fn from_ron_rejects_malformed_input() {
    let result = LayoutTree::from_ron("Row((");

    assert!(matches!(result, Err(LayoutError::Parse(_))));
}

#[test]
fn from_ron_rejects_unknown_component() {
    let source = r#"Pane(view: Minimap)"#;

    let result = LayoutTree::from_ron(source);

    assert!(matches!(result, Err(LayoutError::Parse(_))));
}

#[test]
fn from_ron_runs_validation() {
    let source = r#"Pane(view: Terminal)"#;

    let result = LayoutTree::from_ron(source);

    assert!(matches!(result, Err(LayoutError::MissingEditor)));
}

// ---- frames

fn first_placement(source: &str) -> Placement {
    let tree = LayoutTree::from_ron(source).unwrap();
    tree.resolve(Rect::new(0, 0, 100, 40)).remove(0)
}

#[test]
fn a_pane_with_only_a_view_gets_its_components_default_frame() {
    let placement = first_placement("Pane(view: Editor)");

    assert_eq!(
        placement.frame,
        PaneFrame::default_for(&ComponentKind::Editor)
    );
}

#[test]
fn a_pane_overrides_what_it_names() {
    let source = r#"Pane(view: Editor, border: Rounded, title: Text("Code"), title_align: Center)"#;

    let placement = first_placement(source);

    let expected = PaneFrame::resolve(
        &FrameSpec {
            border: Some(BorderStyle::Rounded),
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
fn style_slots_are_plain_names() {
    let source = r#"Pane(view: Editor, title_slot: "files.title", border_slot: "files.border")"#;

    assert!(LayoutTree::from_ron(source).is_ok());
}

#[test]
fn an_unknown_border_title_or_field_is_rejected() {
    let bad_border = "Pane(view: Editor, border: Wavy)";
    let bad_title = "Pane(view: Editor, title: Loud)";
    let bad_field = "Pane(view: Editor, colour: Red)";
    let no_view = "Pane(sides: [])";

    for source in [bad_border, bad_title, bad_field, no_view] {
        assert!(
            matches!(LayoutTree::from_ron(source), Err(LayoutError::Parse(_))),
            "{source}"
        );
    }
}

#[test]
fn an_editor_pane_satisfies_the_editor_requirement() {
    let ok = "Pane(view: Editor, sides: [])";
    let missing = "Pane(view: Terminal, sides: [])";

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
    let tree = LayoutTree::new(LayoutNode::Row(vec![
        fixed(10, new_leaf(ComponentKind::Explorer)),
        fixed(
            20,
            LayoutNode::Col(vec![
                new_leaf(ComponentKind::Terminal),
                fixed(1, new_leaf(ComponentKind::StatusBar)),
            ]),
        ),
        editor_only(),
    ]))
    .unwrap();

    let after = placed(&tree, &[ComponentKind::Terminal, ComponentKind::StatusBar]);

    assert_eq!(area_of(&after, &ComponentKind::Explorer).width, 10);
    assert_eq!(area_of(&after, &ComponentKind::Editor).width, 90);
}

#[test]
fn a_hidden_pane_with_a_frame_is_not_placed_either() {
    let tree = LayoutTree::from_ron(
        "Row([
            Fixed(10, Pane(view: Explorer, border: Rounded)),
            Pane(view: Editor),
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
    let tree = LayoutTree::new(LayoutNode::Row(vec![
        editor_only(),
        fixed(20, new_leaf(kind.clone())),
    ]))
    .unwrap();

    assert_eq!(placed(&tree, &[]).len(), 2);
    assert_eq!(placed(&tree, &[kind]).len(), 1);
    assert_eq!(placed(&tree, &[ComponentKind::Explorer]).len(), 2);
}

// ---- sides and border style

fn frame_of(source: &str) -> PaneFrame {
    first_placement(&format!("Row([{source}, Fixed(5, Pane(view: Editor))])")).frame
}

#[test]
fn sides_and_border_style_are_set_apart() {
    let frame = frame_of("Pane(view: Explorer, sides: [Top], border: Rounded)");

    assert_eq!(frame.sides(), Borders::TOP);
    assert_eq!(frame.border(), BorderStyle::Rounded);
}

#[test]
fn sides_combine_and_all_means_every_edge() {
    let two = frame_of("Pane(view: Explorer, sides: [Top, Left])");
    let all = frame_of("Pane(view: Explorer, sides: [All])");
    let none = frame_of("Pane(view: Explorer, sides: [])");

    assert_eq!(two.sides(), Borders::TOP | Borders::LEFT);
    assert_eq!(all.sides(), Borders::ALL);
    assert_eq!(none.sides(), Borders::NONE);
}

#[test]
fn omitted_sides_and_border_take_the_components_default() {
    let explorer = frame_of("Pane(view: Explorer)");
    let status = frame_of("Pane(view: StatusBar)");
    let boxed_status = frame_of("Pane(view: StatusBar, border: Double)");
    let one_side = frame_of("Pane(view: Explorer, sides: [Right])");

    assert_eq!(explorer.sides(), Borders::ALL);
    assert_eq!(explorer.border(), BorderStyle::Plain);
    assert_eq!(status.sides(), Borders::NONE);
    assert_eq!(
        boxed_status.sides(),
        Borders::NONE,
        "a style alone adds no edge"
    );
    assert_eq!(boxed_status.border(), BorderStyle::Double);
    assert_eq!(one_side.border(), BorderStyle::Plain);
}

#[test]
fn every_border_style_can_be_named() {
    let styles = [
        "Plain",
        "Rounded",
        "Double",
        "Thick",
        "LightDoubleDashed",
        "HeavyDoubleDashed",
        "LightTripleDashed",
        "HeavyTripleDashed",
        "LightQuadrupleDashed",
        "HeavyQuadrupleDashed",
        "QuadrantInside",
        "QuadrantOutside",
    ];

    for style in styles {
        let source = format!("Pane(view: Editor, border: {style})");
        assert!(LayoutTree::from_ron(&source).is_ok(), "{style}");
    }
}

#[test]
fn the_old_border_names_are_rejected() {
    for old in ["Off", "TopOnly", "RightOnly"] {
        let source = format!("Pane(view: Editor, border: {old})");
        assert!(
            matches!(LayoutTree::from_ron(&source), Err(LayoutError::Parse(_))),
            "{old}"
        );
    }
    let unknown_side = "Pane(view: Editor, sides: [Middle])";
    assert!(matches!(
        LayoutTree::from_ron(unknown_side),
        Err(LayoutError::Parse(_))
    ));
}
