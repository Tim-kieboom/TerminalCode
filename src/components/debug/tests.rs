use super::model::*;

fn var(name: &str, value: &str, reference: i64) -> Incoming {
    Incoming {
        name: name.to_owned(),
        value: value.to_owned(),
        type_name: None,
        reference,
        total: None,
    }
}

fn long(name: &str, reference: i64, total: i64) -> Incoming {
    Incoming {
        total: Some(total),
        ..var(name, "Vec", reference)
    }
}

fn names(tree: &Tree) -> Vec<String> {
    tree.rows()
        .iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
        .collect()
}

fn tree() -> Tree {
    let mut tree = Tree::default();
    tree.set_scopes(&[("Locals".to_owned(), 7), ("Globals".to_owned(), 8)]);
    tree
}

fn id_of(tree: &Tree, at: usize) -> u64 {
    tree.rows()[at].id
}

#[test]
fn the_scopes_are_the_first_rows_and_closed() {
    let tree = tree();

    assert_eq!(names(&tree), ["Locals", "Globals"]);
    assert!(
        tree.rows()
            .iter()
            .all(|row| !row.expanded && row.depth == 0)
    );
}

#[test]
fn opening_a_scope_asks_for_its_variables_and_they_come_below_it() {
    let mut tree = tree();

    let read = tree.toggle(0).unwrap();

    assert_eq!((read.reference, read.page), (7, None));
    assert!(tree.rows()[0].expanded && tree.rows()[0].loading);
    tree.fill(read.row, 0, vec![var("a", "1", 0), var("b", "2", 9)]);
    assert_eq!(names(&tree), ["Locals", "  a", "  b", "Globals"]);
    assert!(!tree.rows()[0].loading);
}

#[test]
fn a_variable_with_children_opens_to_them_a_level_further_in() {
    let mut tree = tree();
    let scope = tree.toggle(0).unwrap();
    tree.fill(
        scope.row,
        0,
        vec![var("point", "Point", 9), var("n", "3", 0)],
    );

    let read = tree.toggle(1).unwrap();
    tree.fill(read.row, 0, vec![var("x", "1", 0), var("y", "2", 0)]);

    assert_eq!(
        names(&tree),
        ["Locals", "  point", "    x", "    y", "  n", "Globals"]
    );
}

#[test]
fn a_variable_without_children_does_not_open() {
    let mut tree = tree();
    let scope = tree.toggle(0).unwrap();
    tree.fill(scope.row, 0, vec![var("n", "3", 0)]);

    assert_eq!(tree.toggle(1), None);
    assert!(!tree.rows()[1].expanded);
    assert_eq!(tree.toggle(9), None, "no such row");
}

#[test]
fn closing_a_row_forgets_everything_below_it() {
    let mut tree = tree();
    let scope = tree.toggle(0).unwrap();
    tree.fill(scope.row, 0, vec![var("point", "Point", 9)]);
    let point = tree.toggle(1).unwrap();
    tree.fill(point.row, 0, vec![var("x", "1", 0)]);
    assert_eq!(tree.rows().len(), 4);

    assert_eq!(tree.toggle(0), None, "a row that is open closes");

    assert_eq!(names(&tree), ["Locals", "Globals"]);
    // Opening it again asks again: what was shown may be old.
    assert!(tree.toggle(0).is_some());
}

#[test]
fn what_arrives_for_a_row_that_was_closed_or_is_gone_is_dropped() {
    let mut tree = tree();
    let read = tree.toggle(0).unwrap();
    tree.toggle(0);

    tree.fill(read.row, 0, vec![var("a", "1", 0)]);
    tree.fill(9999, 0, vec![var("b", "1", 0)]);

    assert_eq!(names(&tree), ["Locals", "Globals"]);
}

#[test]
fn a_refused_read_closes_the_row_again() {
    let mut tree = tree();
    let read = tree.toggle(1).unwrap();

    tree.failed(read.row);

    assert!(!tree.rows()[1].expanded && !tree.rows()[1].loading);
}

#[test]
fn a_second_first_page_replaces_what_was_shown() {
    let mut tree = tree();
    let read = tree.toggle(0).unwrap();
    tree.fill(read.row, 0, vec![var("old", "1", 0)]);

    tree.fill(read.row, 0, vec![var("new", "2", 0)]);

    assert_eq!(names(&tree), ["Locals", "  new", "Globals"]);
}

#[test]
fn a_long_list_is_read_a_page_at_a_time_with_a_row_that_says_how_much_is_left() {
    let mut tree = tree();
    let scope = tree.toggle(0).unwrap();
    tree.fill(scope.row, 0, vec![long("items", 9, 250)]);

    let first = tree.toggle(1).unwrap();
    assert_eq!(first.page, Some((0, PAGE)));
    let page: Vec<Incoming> = (0..PAGE).map(|n| var(&format!("[{n}]"), "0", 0)).collect();
    tree.fill(first.row, 0, page);
    let more = tree
        .rows()
        .iter()
        .position(|row| matches!(row.kind, RowKind::More { .. }))
        .unwrap();
    assert_eq!(tree.rows()[more].value, "… 150 more");
    assert_eq!(tree.rows()[more].depth, 2);

    // Opening that row reads the next page, which is added and the row says less.
    let next = tree.toggle(more).unwrap();
    assert_eq!((next.reference, next.page), (9, Some((100, 100))));
    let page: Vec<Incoming> = (100..200).map(|n| var(&format!("[{n}]"), "0", 0)).collect();
    tree.fill(next.row, 100, page);
    let more = tree
        .rows()
        .iter()
        .position(|row| matches!(row.kind, RowKind::More { .. }))
        .unwrap();
    assert_eq!(tree.rows()[more].value, "… 50 more");
    assert_eq!(tree.rows().iter().filter(|row| row.depth == 2).count(), 201);

    // The last page is short: no row is left.
    let last = tree.toggle(more).unwrap();
    assert_eq!(last.page, Some((200, 50)));
    let page: Vec<Incoming> = (200..250).map(|n| var(&format!("[{n}]"), "0", 0)).collect();
    tree.fill(last.row, 200, page);
    assert!(
        tree.rows()
            .iter()
            .all(|row| matches!(row.kind, RowKind::Variable))
    );
    assert_eq!(tree.rows().iter().filter(|row| row.depth == 2).count(), 250);
    assert_eq!(names(&tree).last().unwrap(), "Globals");
}

#[test]
fn a_list_that_is_not_long_is_read_whole() {
    let mut tree = tree();
    let scope = tree.toggle(0).unwrap();
    tree.fill(scope.row, 0, vec![long("short", 9, PAGE)]);

    let read = tree.toggle(1).unwrap();

    assert_eq!(read.page, None);
}

#[test]
fn rows_have_ids_that_stay_the_same_while_the_rows_move() {
    let mut tree = tree();
    let globals = id_of(&tree, 1);

    let read = tree.toggle(0).unwrap();
    tree.fill(read.row, 0, vec![var("a", "1", 0), var("b", "2", 0)]);

    assert_eq!(tree.rows()[3].id, globals, "it moved down by two rows");
}

#[test]
fn a_session_is_active_from_the_start_until_it_ends() {
    assert!(!Status::Idle.is_active());
    assert!(Status::Starting.is_active());
    assert!(Status::Running.is_active());
    assert!(
        Status::Stopped {
            reason: "step".into(),
            description: None
        }
        .is_active()
    );
    assert!(!Status::Ended("done".into()).is_active());
}

#[test]
fn the_end_of_a_stop_clears_what_it_showed() {
    let mut data = DebugData::default();
    data.frames.push(Frame {
        id: 1,
        name: "main".into(),
        path: None,
        line: 3,
        column: 0,
    });
    data.tree.set_scopes(&[("Locals".to_owned(), 7)]);
    assert!(data.frame().is_some());

    data.forget_stop();

    assert!(data.frame().is_none() && data.tree.rows().is_empty());
}
