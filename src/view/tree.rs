//! Flattening an expanded directory tree into the picker's entry list.

use std::path::Path;

use super::TreeViewState;
use crate::entry::{read_entries, Entry, EntryKind};

impl TreeViewState {
    /// Expands `path` if it is collapsed, and collapses it if it is expanded.
    ///
    /// The path is not checked against the filesystem, so expanding something that is not a directory simply adds an entry that [`build_tree_entries`](Self::build_tree_entries) never uses.
    pub fn toggle_expand(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
        }
    }

    /// Whether this path's children are currently shown.
    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    /// Flattens `root` and every expanded directory below it into a single list in display order, with each entry's [`depth`](crate::Entry::depth) set to its nesting level.
    ///
    /// Symlinks are never expanded, which also rules out cycles. `show_hidden` and `filter` apply at every level, exactly as when listing a single directory.
    pub fn build_tree_entries(
        &self,
        root: &Path,
        show_hidden: bool,
        filter: Option<&dyn Fn(&Path) -> bool>,
    ) -> Vec<Entry> {
        let mut result = Vec::new();
        self.collect_entries(root, 0, show_hidden, filter, &mut result);
        result
    }

    fn collect_entries(
        &self,
        dir: &Path,
        depth: usize,
        show_hidden: bool,
        filter: Option<&dyn Fn(&Path) -> bool>,
        result: &mut Vec<Entry>,
    ) {
        for mut entry in read_entries(dir, show_hidden, filter) {
            entry.depth = depth;
            let expand = entry.kind == EntryKind::Directory && self.is_expanded(&entry.path);
            let path = entry.path.clone();
            result.push(entry);
            if expand {
                self.collect_entries(&path, depth + 1, show_hidden, filter, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn toggle_expand() {
        let mut state = TreeViewState::new();
        let path = PathBuf::from("/some/dir");
        assert!(!state.is_expanded(&path));
        state.toggle_expand(&path);
        assert!(state.is_expanded(&path));
        state.toggle_expand(&path);
        assert!(!state.is_expanded(&path));
    }

    #[test]
    fn build_tree_flat_when_nothing_expanded() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();
        fs::write(tmp.path().join("subdir").join("b.txt"), "").unwrap();
        let state = TreeViewState::new();
        let tree = state.build_tree_entries(tmp.path(), false, None);
        assert_eq!(tree.len(), 2); // subdir + a.txt
        assert!(tree.iter().all(|e| e.depth == 0));
    }

    #[test]
    fn build_tree_with_expanded_dir() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let subdir = tmp.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("b.txt"), "").unwrap();
        let mut state = TreeViewState::new();
        state.toggle_expand(&subdir);
        let tree = state.build_tree_entries(tmp.path(), false, None);
        assert_eq!(tree.len(), 3); // subdir, b.txt (inside), a.txt
        let sub_entry = tree.iter().find(|e| e.name == "b.txt").unwrap();
        assert_eq!(sub_entry.depth, 1);
        let names: Vec<&str> = tree.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["subdir", "b.txt", "a.txt"],
            "children follow their parent"
        );
    }
}
