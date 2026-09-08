//! Per-view state: what the list view and the tree view each need to remember.
//!
//! Both views keep only a cursor and a scroll offset; the tree view adds the set of directories the user has expanded. Everything else, including the entry list itself, lives in [`CommonState`](crate::CommonState), which is why switching views preserves the selection and the search query.

/// List view helpers.
pub mod list;
/// Tree view flattening and expansion tracking.
pub mod tree;

use std::collections::HashSet;
use std::path::PathBuf;

/// Cursor and scroll position for the list view.
#[derive(Debug, Clone)]
pub struct ListViewState {
    /// Index into the visible entries, which is the filtered list while a search is active.
    pub cursor: usize,
    /// Index of the first entry drawn. Updated at render time to keep the cursor on screen.
    pub scroll_offset: usize,
}

impl Default for ListViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl ListViewState {
    /// A fresh list view, scrolled to the top with the cursor on the first entry.
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll_offset: 0,
        }
    }
}

/// Cursor, scroll position and expansion set for the tree view.
#[derive(Debug, Clone)]
pub struct TreeViewState {
    /// Index into the visible entries of the flattened tree.
    pub cursor: usize,
    /// Index of the first entry drawn. Updated at render time to keep the cursor on screen.
    pub scroll_offset: usize,
    /// Directories whose children are currently shown. Paths not in this set are drawn collapsed.
    pub expanded: HashSet<PathBuf>,
}

impl Default for TreeViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeViewState {
    /// A fresh tree view with nothing expanded, so only the root's own entries are listed.
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll_offset: 0,
            expanded: HashSet::new(),
        }
    }
}

/// Which view the picker is in, holding that view's state.
#[derive(Debug, Clone)]
pub enum ViewState {
    /// One directory at a time. `l` and `h` change which directory that is.
    List(ListViewState),
    /// Directories expand in place. `l` and `h` expand and collapse them.
    Tree(TreeViewState),
}

impl ViewState {
    /// Switches to the other view, discarding the old view's cursor, scroll and expansion set.
    ///
    /// Callers normally want [`FilePickerState::toggle_view`](crate::FilePickerState::toggle_view) instead, which also rebuilds the entry list and carries the cursor over by path.
    pub fn toggle(self) -> Self {
        match self {
            ViewState::List(_) => ViewState::Tree(TreeViewState::new()),
            ViewState::Tree(_) => ViewState::List(ListViewState::new()),
        }
    }

    /// The cursor index of whichever view is active.
    pub fn cursor(&self) -> usize {
        match self {
            ViewState::List(s) => s.cursor,
            ViewState::Tree(s) => s.cursor,
        }
    }

    /// Mutable access to the active view's cursor. Callers are responsible for keeping it in range.
    pub fn cursor_mut(&mut self) -> &mut usize {
        match self {
            ViewState::List(s) => &mut s.cursor,
            ViewState::Tree(s) => &mut s.cursor,
        }
    }

    /// The scroll offset of whichever view is active.
    pub fn scroll_offset(&self) -> usize {
        match self {
            ViewState::List(s) => s.scroll_offset,
            ViewState::Tree(s) => s.scroll_offset,
        }
    }

    /// Mutable access to the active view's scroll offset. The widget sets this at render time.
    pub fn scroll_offset_mut(&mut self) -> &mut usize {
        match self {
            ViewState::List(s) => &mut s.scroll_offset,
            ViewState::Tree(s) => &mut s.scroll_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_view_state_default() {
        let state = ListViewState::new();
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn tree_view_state_default() {
        let state = TreeViewState::new();
        assert_eq!(state.cursor, 0);
        assert_eq!(state.scroll_offset, 0);
        assert!(state.expanded.is_empty());
    }

    #[test]
    fn view_state_toggle() {
        let view = ViewState::List(ListViewState::new());
        let toggled = view.toggle();
        assert!(matches!(toggled, ViewState::Tree(_)));
        let back = toggled.toggle();
        assert!(matches!(back, ViewState::List(_)));
    }
}
