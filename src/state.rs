use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use crate::entry::{read_entries, Entry, EntryKind};
use crate::theme::FilePickerTheme;
use crate::view::{ListViewState, TreeViewState, ViewState};

/// Type alias for an optional boxed filter predicate to avoid type complexity warnings.
pub type FilterFn = Option<Box<dyn Fn(&Path) -> bool>>;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    FilesOnly,
    DirsOnly,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    List,
    Tree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerResult {
    Pending,
    Selected(Vec<PathBuf>),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Search,
}

// ---------------------------------------------------------------------------
// CommonState
// ---------------------------------------------------------------------------

pub struct CommonState {
    pub current_dir: PathBuf,
    pub entries: Vec<Entry>,
    pub filtered_indices: Option<Vec<usize>>,
    pub selected: HashSet<PathBuf>,
    pub show_hidden: bool,
    pub mode: PickerMode,
    pub input_mode: InputMode,
    pub search_query: String,
    pub pending_key: Option<(char, Instant)>,
    pub error_message: Option<String>,
    pub result: PickerResult,
    pub filter: FilterFn,
    pub theme: FilePickerTheme,
    /// Screen area of the entry list from the last render. Mouse clicks and
    /// page movements are resolved against it. Zero-sized until first render.
    pub list_area: Rect,
}

// Manual Debug because filter is not Debug.
impl std::fmt::Debug for CommonState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommonState")
            .field("current_dir", &self.current_dir)
            .field("entries_len", &self.entries.len())
            .field("filtered_indices", &self.filtered_indices)
            .field("selected", &self.selected)
            .field("show_hidden", &self.show_hidden)
            .field("mode", &self.mode)
            .field("input_mode", &self.input_mode)
            .field("search_query", &self.search_query)
            .field("result", &self.result)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// FilePickerState
// ---------------------------------------------------------------------------

pub struct FilePickerState {
    pub common: CommonState,
    pub view: ViewState,
}

impl FilePickerState {
    pub fn builder() -> FilePickerBuilder {
        FilePickerBuilder::default()
    }

    // --- Result ---

    pub fn result(&self) -> PickerResult {
        self.common.result.clone()
    }

    // --- Visible entries helpers ---

    pub fn visible_entries(&self) -> Vec<&Entry> {
        match &self.common.filtered_indices {
            Some(indices) => indices
                .iter()
                .filter_map(|&i| self.common.entries.get(i))
                .collect(),
            None => self.common.entries.iter().collect(),
        }
    }

    pub fn visible_count(&self) -> usize {
        match &self.common.filtered_indices {
            Some(indices) => indices.len(),
            None => self.common.entries.len(),
        }
    }

    fn actual_index(&self, visible_idx: usize) -> Option<usize> {
        match &self.common.filtered_indices {
            Some(indices) => indices.get(visible_idx).copied(),
            None => {
                if visible_idx < self.common.entries.len() {
                    Some(visible_idx)
                } else {
                    None
                }
            }
        }
    }

    pub fn current_entry(&self) -> Option<&Entry> {
        let cursor = self.view.cursor();
        let actual = self.actual_index(cursor)?;
        self.common.entries.get(actual)
    }

    // --- Directory refresh ---

    pub fn refresh_entries(&mut self) {
        let dir = self.common.current_dir.clone();
        let show_hidden = self.common.show_hidden;
        let filter = self.common.filter.as_deref();
        self.common.entries = match &self.view {
            ViewState::List(_) => read_entries(&dir, show_hidden, filter),
            ViewState::Tree(tree) => tree.build_tree_entries(&dir, show_hidden, filter),
        };
        self.common.filtered_indices = None;
        self.clamp_cursor();
    }

    // --- Tree view ---

    fn is_expanded(&self, path: &Path) -> bool {
        match &self.view {
            ViewState::Tree(tree) => tree.is_expanded(path),
            ViewState::List(_) => false,
        }
    }

    /// Tree view only: show the children of the directory under the cursor.
    pub fn expand_current(&mut self) {
        self.set_expanded_current(true);
    }

    /// Tree view only: hide the children of the directory under the cursor.
    pub fn collapse_current(&mut self) {
        self.set_expanded_current(false);
    }

    /// Tree view only: expand or collapse the directory under the cursor.
    pub fn toggle_expand_current(&mut self) {
        let Some(entry) = self.current_entry() else {
            return;
        };
        let expanded = self.is_expanded(&entry.path);
        self.set_expanded_current(!expanded);
    }

    fn set_expanded_current(&mut self, expanded: bool) {
        let Some(entry) = self.current_entry() else {
            return;
        };
        if entry.kind != EntryKind::Directory {
            return;
        }
        let path = entry.path.clone();
        let ViewState::Tree(tree) = &mut self.view else {
            return;
        };
        if expanded {
            tree.expanded.insert(path.clone());
        } else {
            tree.expanded.remove(&path);
        }
        self.refresh_entries();
        self.move_cursor_to_path(&path);
    }

    /// Moves the cursor to the visible entry with this path.
    /// Returns false (leaving the cursor alone) if it is not visible.
    fn move_cursor_to_path(&mut self, path: &Path) -> bool {
        let found = self.visible_entries().iter().position(|e| e.path == path);
        match found {
            Some(idx) => {
                *self.view.cursor_mut() = idx;
                true
            }
            None => false,
        }
    }

    fn move_to_first_child(&mut self) {
        let cursor = self.view.cursor();
        let has_child = {
            let entries = self.visible_entries();
            match (entries.get(cursor), entries.get(cursor + 1)) {
                (Some(cur), Some(next)) => next.depth == cur.depth + 1,
                _ => false,
            }
        };
        if has_child {
            *self.view.cursor_mut() = cursor + 1;
        }
    }

    fn move_to_parent_node(&mut self) {
        let cursor = self.view.cursor();
        let parent = {
            let entries = self.visible_entries();
            match entries.get(cursor).map(|e| e.depth) {
                Some(depth) if depth > 0 => {
                    entries[..cursor].iter().rposition(|e| e.depth == depth - 1)
                }
                _ => None,
            }
        };
        if let Some(idx) = parent {
            *self.view.cursor_mut() = idx;
        }
    }

    /// Moves "into" the entry under the cursor.
    /// List view: enters the directory. Tree view: expands a collapsed
    /// directory, moves to the first child of an expanded one, and enters a
    /// symlink as the new root because symlinks are never expanded in place.
    pub fn descend(&mut self) {
        if matches!(self.view, ViewState::List(_)) {
            self.enter_directory();
            return;
        }
        let Some(entry) = self.current_entry() else {
            return;
        };
        let kind = entry.kind.clone();
        let expanded = self.is_expanded(&entry.path);
        match kind {
            EntryKind::Directory if expanded => self.move_to_first_child(),
            EntryKind::Directory => self.expand_current(),
            EntryKind::Symlink => self.enter_directory(),
            EntryKind::File => {}
        }
    }

    /// Moves "out of" the entry under the cursor.
    /// List view: goes to the parent directory. Tree view: collapses an
    /// expanded directory, moves a nested entry to its parent node, and goes
    /// to the parent directory from a top-level entry.
    pub fn ascend(&mut self) {
        if matches!(self.view, ViewState::List(_)) {
            self.go_parent();
            return;
        }
        let Some(entry) = self.current_entry() else {
            self.go_parent();
            return;
        };
        let kind = entry.kind.clone();
        let depth = entry.depth;
        let expanded = self.is_expanded(&entry.path);
        if kind == EntryKind::Directory && expanded {
            self.collapse_current();
        } else if depth > 0 {
            self.move_to_parent_node();
        } else {
            self.go_parent();
        }
    }

    // --- Toggles ---

    pub fn toggle_hidden(&mut self) {
        self.common.show_hidden = !self.common.show_hidden;
        self.refresh_entries();
    }

    pub fn toggle_select(&mut self) {
        let entry = match self.current_entry() {
            Some(e) => e,
            None => return,
        };
        let kind = entry.kind.clone();
        let path = entry.path.clone();

        if !self.is_selectable(&kind) {
            return;
        }

        if self.common.selected.contains(&path) {
            self.common.selected.remove(&path);
        } else {
            self.common.selected.insert(path);
        }
    }

    /// Whether an entry of this kind may be picked under the current `PickerMode`.
    /// Symlinks are always allowed because their target kind is not known here.
    fn is_selectable(&self, kind: &EntryKind) -> bool {
        match self.common.mode {
            PickerMode::FilesOnly => *kind != EntryKind::Directory,
            PickerMode::DirsOnly => *kind != EntryKind::File,
            PickerMode::Both => true,
        }
    }

    // --- Confirm / Cancel ---

    pub fn confirm(&mut self) {
        if !self.common.selected.is_empty() {
            let mut paths: Vec<PathBuf> = self.common.selected.iter().cloned().collect();
            paths.sort();
            self.common.result = PickerResult::Selected(paths);
            return;
        }

        match self.current_entry() {
            Some(entry) if entry.kind == EntryKind::Directory => {
                if matches!(self.view, ViewState::Tree(_)) {
                    self.toggle_expand_current();
                } else {
                    self.enter_directory();
                }
            }
            Some(entry) if self.is_selectable(&entry.kind) => {
                let path = entry.path.clone();
                self.common.result = PickerResult::Selected(vec![path]);
            }
            _ => {}
        }
    }

    pub fn cancel(&mut self) {
        self.common.result = PickerResult::Cancelled;
    }

    // --- Directory navigation ---

    pub fn enter_directory(&mut self) {
        let entry = match self.current_entry() {
            Some(e) => e,
            None => return,
        };

        if entry.kind == EntryKind::File {
            return;
        }
        let is_symlink = entry.kind == EntryKind::Symlink;
        let Ok(canonical) = entry.path.canonicalize() else {
            return;
        };
        if !canonical.is_dir() {
            return;
        }

        // A symlink that resolves to the current directory or one of its
        // ancestors would only lead back to where we already are.
        if is_symlink && self.canonical_current_dir().starts_with(&canonical) {
            self.common.error_message = Some("Circular symlink".to_string());
            return;
        }

        self.common.current_dir = canonical;
        *self.view.cursor_mut() = 0;
        *self.view.scroll_offset_mut() = 0;
        self.refresh_entries();
    }

    fn canonical_current_dir(&self) -> PathBuf {
        self.common
            .current_dir
            .canonicalize()
            .unwrap_or_else(|_| self.common.current_dir.clone())
    }

    pub fn go_parent(&mut self) {
        if let Some(parent) = self.common.current_dir.parent().map(|p| p.to_path_buf()) {
            self.common.current_dir = parent;
            *self.view.cursor_mut() = 0;
            *self.view.scroll_offset_mut() = 0;
            self.refresh_entries();
        }
    }

    pub fn go_home(&mut self) {
        if let Some(home) = dirs::home_dir() {
            self.common.current_dir = home;
            *self.view.cursor_mut() = 0;
            *self.view.scroll_offset_mut() = 0;
            self.refresh_entries();
        }
    }

    // --- Cursor movement ---

    pub fn move_cursor_down(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            return;
        }
        let cursor = self.view.cursor_mut();
        if *cursor + 1 < count {
            *cursor += 1;
        }
    }

    pub fn move_cursor_up(&mut self) {
        let cursor = self.view.cursor_mut();
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    pub fn move_to_top(&mut self) {
        *self.view.cursor_mut() = 0;
    }

    pub fn move_to_bottom(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            *self.view.cursor_mut() = count - 1;
        }
    }

    /// Number of entry rows shown by the last render, or a fallback before
    /// the widget has been drawn.
    pub fn page_height(&self) -> usize {
        match self.common.list_area.height {
            0 => 20,
            h => h as usize,
        }
    }

    pub fn move_half_page_down(&mut self, page_height: usize) {
        let half = page_height / 2;
        let count = self.visible_count();
        if count == 0 {
            return;
        }
        let cursor = self.view.cursor_mut();
        *cursor = (*cursor + half).min(count - 1);
    }

    pub fn move_half_page_up(&mut self, page_height: usize) {
        let half = page_height / 2;
        let cursor = self.view.cursor_mut();
        *cursor = cursor.saturating_sub(half);
    }

    // --- View toggle ---

    pub fn toggle_view(&mut self) {
        let keep = self.current_entry().map(|e| e.path.clone());
        self.view = self.view.clone().toggle();
        self.refresh_entries();
        let kept = keep.map(|p| self.move_cursor_to_path(&p)).unwrap_or(false);
        if !kept {
            *self.view.cursor_mut() = 0;
        }
    }

    // --- Cursor clamping ---

    fn clamp_cursor(&mut self) {
        let count = self.visible_count();
        let cursor = self.view.cursor_mut();
        if count == 0 {
            *cursor = 0;
        } else if *cursor >= count {
            *cursor = count - 1;
        }
    }

    pub fn clamp_cursor_pub(&mut self) {
        self.clamp_cursor();
    }

    // --- Event handling ---

    pub fn handle_event(&mut self, event: crossterm::event::Event) {
        crate::event::handle_event(self, event);
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct FilePickerBuilder {
    start_dir: Option<PathBuf>,
    mode: PickerMode,
    view_mode: ViewMode,
    filter: FilterFn,
    theme: FilePickerTheme,
    show_hidden: bool,
}

impl Default for FilePickerBuilder {
    fn default() -> Self {
        Self {
            start_dir: None,
            mode: PickerMode::Both,
            view_mode: ViewMode::List,
            filter: None,
            theme: FilePickerTheme::default(),
            show_hidden: false,
        }
    }
}

impl FilePickerBuilder {
    pub fn start_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.start_dir = Some(dir.into());
        self
    }

    pub fn mode(mut self, mode: PickerMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn view(mut self, view_mode: ViewMode) -> Self {
        self.view_mode = view_mode;
        self
    }

    pub fn filter(mut self, f: impl Fn(&Path) -> bool + 'static) -> Self {
        self.filter = Some(Box::new(f));
        self
    }

    pub fn theme(mut self, theme: FilePickerTheme) -> Self {
        self.theme = theme;
        self
    }

    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    pub fn build(self) -> FilePickerState {
        let start_dir = self
            .start_dir
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let current_dir = resolve_start_dir(start_dir);

        let entries = read_entries(
            &current_dir,
            self.show_hidden,
            self.filter.as_deref(),
        );

        let view = match self.view_mode {
            ViewMode::List => ViewState::List(ListViewState::new()),
            ViewMode::Tree => ViewState::Tree(TreeViewState::new()),
        };

        let common = CommonState {
            current_dir,
            entries,
            filtered_indices: None,
            selected: HashSet::new(),
            show_hidden: self.show_hidden,
            mode: self.mode,
            input_mode: InputMode::Normal,
            search_query: String::new(),
            pending_key: None,
            error_message: None,
            result: PickerResult::Pending,
            filter: self.filter,
            theme: self.theme,
            list_area: Rect::default(),
        };

        FilePickerState { common, view }
    }
}

/// Expands a leading `~` to the home directory and resolves the result to an
/// absolute path with symlinks removed. A path that does not exist is kept as
/// is (after tilde expansion) so the picker can still show it in the path bar.
fn resolve_start_dir(dir: PathBuf) -> PathBuf {
    let expanded = expand_tilde(dir);
    expanded.canonicalize().unwrap_or(expanded)
}

fn expand_tilde(dir: PathBuf) -> PathBuf {
    let Some(home) = dirs::home_dir() else {
        return dir;
    };
    match dir.strip_prefix("~") {
        Ok(rest) if rest.as_os_str().is_empty() => home,
        Ok(rest) => home.join(rest),
        Err(_) => dir,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_dir_with_files() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("alpha.txt"), b"").unwrap();
        fs::write(dir.path().join("beta.rs"), b"").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        dir
    }

    /// root/
    ///   a_dir/
    ///     nested/
    ///       deep.txt
    ///     inner.txt
    ///   b_dir/
    ///   top.txt
    /// Root order: a_dir, b_dir, top.txt. Inside a_dir: nested, inner.txt.
    fn make_tree_dir() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap().join("root");
        fs::create_dir_all(root.join("a_dir").join("nested")).unwrap();
        fs::write(root.join("a_dir").join("nested").join("deep.txt"), b"").unwrap();
        fs::write(root.join("a_dir").join("inner.txt"), b"").unwrap();
        fs::create_dir(root.join("b_dir")).unwrap();
        fs::write(root.join("top.txt"), b"").unwrap();
        (tmp, root)
    }

    fn tree_state(root: &Path) -> FilePickerState {
        FilePickerState::builder()
            .start_dir(root)
            .view(ViewMode::Tree)
            .build()
    }

    fn names(state: &FilePickerState) -> Vec<String> {
        state.visible_entries().iter().map(|e| e.name.clone()).collect()
    }

    #[test]
    fn tree_toggle_expand_shows_and_hides_children() {
        let (_tmp, root) = make_tree_dir();
        let mut state = tree_state(&root);
        assert_eq!(names(&state), ["a_dir", "b_dir", "top.txt"]);

        state.toggle_expand_current();
        assert_eq!(names(&state), ["a_dir", "nested", "inner.txt", "b_dir", "top.txt"]);
        assert_eq!(state.visible_entries()[1].depth, 1);
        assert_eq!(state.view.cursor(), 0, "cursor stays on the expanded directory");

        state.toggle_expand_current();
        assert_eq!(names(&state), ["a_dir", "b_dir", "top.txt"]);
    }

    #[test]
    fn expand_is_noop_in_list_view() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder().start_dir(&root).build();

        state.toggle_expand_current();

        assert_eq!(names(&state), ["a_dir", "b_dir", "top.txt"]);
    }

    #[test]
    fn descend_expands_then_moves_into_children() {
        let (_tmp, root) = make_tree_dir();
        let mut state = tree_state(&root);

        state.descend(); // expand a_dir
        assert_eq!(state.visible_count(), 5);
        assert_eq!(state.view.cursor(), 0);

        state.descend(); // already expanded: first child
        assert_eq!(state.current_entry().unwrap().name, "nested");

        state.descend(); // expand nested
        assert_eq!(state.visible_count(), 6);
        state.descend(); // first child of nested
        assert_eq!(state.current_entry().unwrap().name, "deep.txt");

        state.descend(); // file: nothing happens
        assert_eq!(state.current_entry().unwrap().name, "deep.txt");
        assert_eq!(state.common.current_dir, root, "root is unchanged in tree view");
    }

    #[test]
    fn ascend_collapses_then_moves_to_parent_then_leaves_root() {
        let (_tmp, root) = make_tree_dir();
        let mut state = tree_state(&root);
        for _ in 0..4 {
            state.descend();
        }
        assert_eq!(state.current_entry().unwrap().name, "deep.txt");

        state.ascend(); // file at depth 2: go to parent node
        assert_eq!(state.current_entry().unwrap().name, "nested");
        state.ascend(); // expanded dir: collapse it
        assert_eq!(state.current_entry().unwrap().name, "nested");
        assert_eq!(state.visible_count(), 5);
        state.ascend(); // collapsed dir at depth 1: go to parent node
        assert_eq!(state.current_entry().unwrap().name, "a_dir");
        state.ascend(); // expanded dir: collapse it
        assert_eq!(state.visible_count(), 3);
        state.ascend(); // collapsed dir at depth 0: leave root
        assert_eq!(state.common.current_dir, root.parent().unwrap().canonicalize().unwrap());
    }

    #[test]
    fn descend_and_ascend_change_root_in_list_view() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder().start_dir(&root).build();

        state.descend();
        assert!(state.common.current_dir.ends_with("a_dir"));
        state.ascend();
        assert_eq!(state.common.current_dir, root.canonicalize().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn descend_on_symlink_in_tree_view_follows_it() {
        let (_tmp, root) = make_tree_dir();
        std::os::unix::fs::symlink(root.join("b_dir"), root.join("link")).unwrap();
        let mut state = tree_state(&root);
        let idx = state.common.entries.iter().position(|e| e.name == "link").unwrap();
        *state.view.cursor_mut() = idx;

        state.descend();

        assert!(state.common.current_dir.ends_with("b_dir"));
    }

    #[test]
    fn confirm_toggles_directory_in_tree_view() {
        let (_tmp, root) = make_tree_dir();
        let mut state = tree_state(&root);

        state.confirm();
        assert_eq!(state.visible_count(), 5);
        assert_eq!(state.result(), PickerResult::Pending);
        assert_eq!(state.common.current_dir, root);

        state.confirm();
        assert_eq!(state.visible_count(), 3);
    }

    #[test]
    fn toggle_view_keeps_cursor_on_same_entry() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder().start_dir(&root).build();
        *state.view.cursor_mut() = 2;

        state.toggle_view();

        assert!(matches!(state.view, ViewState::Tree(_)));
        assert_eq!(state.current_entry().unwrap().name, "top.txt");
    }

    #[test]
    fn toggle_view_falls_back_to_top_when_entry_disappears() {
        let (_tmp, root) = make_tree_dir();
        let mut state = tree_state(&root);
        state.descend();
        *state.view.cursor_mut() = 2;
        assert_eq!(state.current_entry().unwrap().name, "inner.txt");

        state.toggle_view();

        assert!(matches!(state.view, ViewState::List(_)));
        assert_eq!(names(&state), ["a_dir", "b_dir", "top.txt"]);
        assert_eq!(state.view.cursor(), 0);
    }

    #[test]
    fn builder_defaults() {
        let dir = make_dir_with_files();
        let state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        assert_eq!(state.common.mode, PickerMode::Both);
        assert!(!state.common.show_hidden);
        assert_eq!(state.common.result, PickerResult::Pending);
        assert!(matches!(state.view, ViewState::List(_)));
        assert_eq!(state.common.input_mode, InputMode::Normal);
    }

    #[test]
    fn builder_canonicalizes_relative_start_dir() {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let mut state = FilePickerState::builder().start_dir(".").build();

        assert_eq!(state.common.current_dir, cwd);

        // Going up from a resolved path lands in the real parent, not "".
        state.go_parent();
        assert_eq!(state.common.current_dir, cwd.parent().unwrap());
        assert!(state.visible_count() > 0, "parent directory should list entries");
    }

    #[test]
    fn builder_expands_tilde() {
        let home = dirs::home_dir().unwrap().canonicalize().unwrap();

        let state = FilePickerState::builder().start_dir("~").build();
        assert_eq!(state.common.current_dir, home);

        // "~/" prefix is expanded; a nonexistent target keeps the expanded path.
        let state = FilePickerState::builder()
            .start_dir("~/ratatree-nonexistent-dir")
            .build();
        assert_eq!(state.common.current_dir, home.join("ratatree-nonexistent-dir"));
    }

    #[test]
    fn builder_with_tree_view() {
        let dir = make_dir_with_files();
        let state = FilePickerState::builder()
            .start_dir(dir.path())
            .view(ViewMode::Tree)
            .build();

        assert!(matches!(state.view, ViewState::Tree(_)));
    }

    #[test]
    fn builder_with_filter() {
        let dir = make_dir_with_files();
        let state = FilePickerState::builder()
            .start_dir(dir.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
            .build();

        // Only alpha.txt and subdir (dirs pass the filter always) should be present
        let names: Vec<&str> = state.common.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"alpha.txt"), "alpha.txt should be included");
        assert!(!names.contains(&"beta.rs"), "beta.rs should be filtered out");
        assert!(names.contains(&"subdir"), "dirs always pass filter");
    }

    #[test]
    fn picker_mode_files_only() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::FilesOnly)
            .build();

        // Move cursor to a directory entry (dirs come first after sorting)
        // First entry is "subdir" (Directory), attempt to select it
        *state.view.cursor_mut() = 0;
        // Make sure first entry is a directory
        let first_kind = state.current_entry().map(|e| e.kind.clone());
        assert_eq!(first_kind, Some(EntryKind::Directory));

        state.toggle_select();
        assert!(state.common.selected.is_empty(), "should not select a directory in FilesOnly mode");
    }

    #[test]
    fn dirs_only_confirm_on_file_does_nothing() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("file.txt"), b"").unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();

        assert_eq!(state.current_entry().unwrap().kind, EntryKind::File);
        state.confirm();
        assert_eq!(state.result(), PickerResult::Pending);
    }

    #[test]
    fn toggle_hidden() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("visible.txt"), b"").unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        assert_eq!(state.visible_count(), 1);

        state.toggle_hidden();
        assert_eq!(state.visible_count(), 2);

        state.toggle_hidden();
        assert_eq!(state.visible_count(), 1);
    }

    #[test]
    fn multi_select_toggle() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        // Find a file entry (not directory)
        let file_idx = state
            .common
            .entries
            .iter()
            .position(|e| e.kind == EntryKind::File)
            .expect("should have a file entry");
        *state.view.cursor_mut() = file_idx;

        state.toggle_select();
        assert_eq!(state.common.selected.len(), 1);

        // Toggle same entry again - should deselect
        state.toggle_select();
        assert_eq!(state.common.selected.len(), 0);
    }

    #[test]
    fn confirm_returns_cursor_when_no_selection() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("only_file.txt"), b"").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        // Cursor is at a file with empty selected set
        *state.view.cursor_mut() = 0;
        state.confirm();

        match state.result() {
            PickerResult::Selected(paths) => {
                assert_eq!(paths.len(), 1);
                assert!(paths[0].ends_with("only_file.txt"));
            }
            other => panic!("expected Selected, got {:?}", other),
        }
    }

    #[test]
    fn confirm_returns_selected_set() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        // Select two files
        let file_indices: Vec<usize> = state
            .common
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == EntryKind::File)
            .map(|(i, _)| i)
            .collect();

        assert!(file_indices.len() >= 2, "need at least 2 files for this test");

        *state.view.cursor_mut() = file_indices[0];
        state.toggle_select();
        *state.view.cursor_mut() = file_indices[1];
        state.toggle_select();

        assert_eq!(state.common.selected.len(), 2);

        state.confirm();

        match state.result() {
            PickerResult::Selected(paths) => {
                assert_eq!(paths.len(), 2);
            }
            other => panic!("expected Selected, got {:?}", other),
        }
    }

    #[test]
    fn reenter_directory_after_go_parent() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        // subdir is first (dirs sort first)
        state.enter_directory();
        assert!(state.common.current_dir.ends_with("subdir"));

        state.go_parent();
        assert!(!state.common.current_dir.ends_with("subdir"));

        // Entering the same directory a second time must work and must not
        // be mistaken for a circular symlink.
        state.enter_directory();
        assert!(
            state.common.current_dir.ends_with("subdir"),
            "expected to re-enter subdir, got {:?} (error: {:?})",
            state.common.current_dir,
            state.common.error_message
        );
        assert_eq!(state.common.error_message, None);
    }

    #[test]
    fn cancel_returns_cancelled() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        assert_eq!(state.result(), PickerResult::Pending);
        state.cancel();
        assert_eq!(state.result(), PickerResult::Cancelled);
    }
}
