use std::{
    fs,
    path::{Path, PathBuf},
};

use vecmap::{IdGenerator, VecMap, impl_vecmap_ids};

pub struct FileNode {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_loaded: bool,
    is_expanded: bool,
    children: Vec<FileIndex>,
}
impl FileNode {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    pub fn is_expanded(&self) -> bool {
        self.is_expanded
    }
}

impl_vecmap_ids!(FileIndex);
pub struct FileTree {
    arena: VecMap<FileIndex, FileNode>,
    alloc: IdGenerator<FileIndex>,
    root: FileIndex,
}
impl FileTree {
    pub fn new(root_path: PathBuf) -> Self {
        let mut arena = VecMap::new();
        let mut alloc = IdGenerator::new();
        let name = file_name_str(&root_path).unwrap_or("<root>").to_string();

        let root = alloc.alloc();
        let root_node = FileNode {
            name,
            path: root_path,
            is_dir: true,
            is_loaded: false,
            is_expanded: true,
            children: Vec::new(),
        };

        arena.insert(root, root_node);

        let mut tree = Self { arena, alloc, root };
        tree.load_children(tree.root);
        tree
    }

    pub fn node(&self, index: FileIndex) -> &FileNode {
        &self.arena[index]
    }

    pub fn toggle(&mut self, index: FileIndex) {
        if !self.arena[index].is_dir {
            return;
        }
        if !self.arena[index].is_loaded {
            self.load_children(index);
        }
        self.arena[index].is_expanded = !self.arena[index].is_expanded;
    }

    pub fn visible(&self) -> Vec<VisibleIndex> {
        let mut visible = Vec::new();

        let first = VisibleIndex::new(self.root, 0);
        let mut stack = vec![first];
        while let Some(index) = stack.pop() {
            let VisibleIndex { file, depth } = index;
            visible.push(VisibleIndex::new(file, depth));
            let node = &self.arena[file];
            if !node.is_expanded || !node.is_dir {
                continue;
            }

            for &child in node.children.iter().rev() {
                stack.push(VisibleIndex::new(child, depth + 1));
            }
        }
        visible
    }

    fn alloc(&mut self, name: String, path: PathBuf, is_dir: bool) -> FileIndex {
        let index = self.alloc.alloc();
        self.arena.insert(
            index,
            FileNode {
                name,
                path,
                is_dir,
                is_loaded: false,
                is_expanded: false,
                children: Vec::new(),
            },
        );

        index
    }

    fn load_children(&mut self, index: FileIndex) {
        let node = &self.arena[index];
        if node.is_loaded || !node.is_dir {
            return;
        }

        let path = node.path.clone();
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        let is_loaded = self.fill_entries(&path, &mut dirs, &mut files);
        dirs.sort();
        files.sort();

        self.arena[index].is_loaded = is_loaded;
        for name in dirs {
            let child = self.alloc(name.clone(), self.arena[index].path.join(&name), true);
            self.arena[index].children.push(child);
        }
        for name in files {
            let child = self.alloc(name.clone(), self.arena[index].path.join(&name), false);
            self.arena[index].children.push(child);
        }
    }

    fn fill_entries(&self, path: &Path, dirs: &mut Vec<String>, files: &mut Vec<String>) -> bool {
        let Ok(entries) = fs::read_dir(path) else {
            return false;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }

            if file_type.is_dir() {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }

        true
    }
}

pub struct VisibleIndex {
    pub file: FileIndex,
    pub depth: usize,
}
impl VisibleIndex {
    pub fn new(file: FileIndex, depth: usize) -> Self {
        Self { file, depth }
    }
}

fn file_name_str(path: &Path) -> Option<&str> {
    path.file_name().and_then(|os_str| os_str.to_str())
}
