use std::fs;

use super::{FileIndex, FileTree, VisibleIndex};

fn temp_tree(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("file_tree_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("a_dir")).unwrap();
    fs::write(root.join("a_dir/file1.rs"), "").unwrap();
    fs::write(root.join("z_file.rs"), "").unwrap();
    fs::write(root.join("b_file.rs"), "").unwrap();
    root
}

fn names<'a>(tree: &'a FileTree, visible: &[VisibleIndex]) -> Vec<&'a str> {
    visible
        .iter()
        .map(|entry| tree.node(entry.file).name())
        .collect()
}

fn find_node(tree: &FileTree, name: &str) -> FileIndex {
    tree.visible()
        .into_iter()
        .find(|entry| tree.node(entry.file).name() == name)
        .map(|entry| entry.file)
        .unwrap()
}

#[test]
fn new_lists_dirs_before_files_sorted() {
    let root = temp_tree("listing");
    let tree = FileTree::new(root.clone());
    let root_name = root.file_name().unwrap().to_str().unwrap();

    assert_eq!(
        names(&tree, &tree.visible()),
        vec![root_name, "a_dir", "b_file.rs", "z_file.rs"]
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn toggle_expands_and_collapses_dir() {
    let root = temp_tree("toggle");
    let root_name = root.file_name().unwrap().to_str().unwrap();
    let mut tree = FileTree::new(root.clone());

    let a_dir = find_node(&tree, "a_dir");
    assert!(!tree.node(a_dir).is_expanded());

    tree.toggle(a_dir);
    assert!(tree.node(a_dir).is_expanded());
    assert_eq!(
        names(&tree, &tree.visible()),
        vec![root_name, "a_dir", "file1.rs", "b_file.rs", "z_file.rs"]
    );

    tree.toggle(a_dir);
    assert!(!tree.node(a_dir).is_expanded());
    let visible = names(&tree, &tree.visible());
    assert!(!visible.contains(&"file1.rs"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn toggle_file_is_noop() {
    let root = temp_tree("file_noop");
    let mut tree = FileTree::new(root.clone());

    let b_file = find_node(&tree, "b_file.rs");
    assert!(!tree.node(b_file).is_dir());

    tree.toggle(b_file);
    assert_eq!(tree.visible().len(), 4);

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn nested_items_have_increasing_depth() {
    let root = temp_tree("depth");
    let mut tree = FileTree::new(root.clone());

    let a_dir = find_node(&tree, "a_dir");
    tree.toggle(a_dir);

    let file1 = find_node(&tree, "file1.rs");
    let a_depth = tree
        .visible()
        .iter()
        .find(|e| e.file == a_dir)
        .unwrap()
        .depth;
    let file_depth = tree
        .visible()
        .iter()
        .find(|e| e.file == file1)
        .unwrap()
        .depth;
    assert_eq!(a_depth, 1);
    assert_eq!(file_depth, 2);

    let _ = fs::remove_dir_all(&root);
}
