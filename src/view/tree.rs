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
    ///
    /// Only a failure to read `root` itself is an error. An expanded subdirectory that cannot be read is skipped, because a large tree often contains a few and failing on each would lose the whole listing.
    pub fn build_tree_entries(
        &self,
        root: &Path,
        show_hidden: bool,
        filter: Option<&dyn Fn(&Path) -> bool>,
    ) -> std::io::Result<Vec<Entry>> {
        let mut result = Vec::new();
        let entries = read_entries(root, show_hidden, filter)?;
        self.push_entries(entries, 0, show_hidden, filter, &mut result);
        Ok(result)
    }

    fn collect_entries(
        &self,
        dir: &Path,
        depth: usize,
        show_hidden: bool,
        filter: Option<&dyn Fn(&Path) -> bool>,
        result: &mut Vec<Entry>,
    ) {
        // A subdirectory we cannot read is skipped rather than aborting the whole
        // tree; a large tree often contains a few of them and erroring on each
        // would drown out the listing.
        let Ok(entries) = read_entries(dir, show_hidden, filter) else {
            return;
        };
        self.push_entries(entries, depth, show_hidden, filter, result);
    }

    fn push_entries(
        &self,
        entries: Vec<Entry>,
        depth: usize,
        show_hidden: bool,
        filter: Option<&dyn Fn(&Path) -> bool>,
        result: &mut Vec<Entry>,
    ) {
        for mut entry in entries {
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
    #[cfg(unix)]
    #[test]
    fn an_unreadable_subdirectory_is_skipped_without_losing_its_siblings() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        let locked = root.join("a_locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::create_dir(root.join("b_open")).unwrap();
        std::fs::write(root.join("b_open/child.txt"), b"").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

        if std::fs::read_dir(&locked).is_ok() {
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let mut tree = TreeViewState::new();
        tree.toggle_expand(&locked);
        tree.toggle_expand(&root.join("b_open"));
        let entries = tree.build_tree_entries(root, false, None).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["a_locked", "b_open", "child.txt"]);
    }

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
        let tree = state.build_tree_entries(tmp.path(), false, None).unwrap();
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
        let tree = state.build_tree_entries(tmp.path(), false, None).unwrap();
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
