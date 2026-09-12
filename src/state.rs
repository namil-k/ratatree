//! The picker's state: what it is showing, where the cursor is, and what the user has chosen.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use crate::entry::{read_entries, Entry, EntryKind};
use crate::theme::FilePickerTheme;
use crate::view::{ListViewState, TreeViewState, ViewState};

/// An optional predicate deciding which files to show, as stored on the picker.
///
/// Set it through [`FilePickerBuilder::filter`]. It is only consulted for files and symlinks; directories always pass so that their contents stay reachable.
pub type FilterFn = Option<Box<dyn Fn(&Path) -> bool>>;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// What the picker is allowed to return.
///
/// This constrains confirming and multi-selecting, not navigation: directories are always listed and always enterable, whatever the mode. Symlinks are always selectable, because resolving them to decide would mean touching the filesystem on every keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    /// Only files may be picked. `Enter` on a directory enters it instead of returning it.
    FilesOnly,
    /// Only directories may be picked. `Enter` on a file does nothing.
    ///
    /// The listing starts with a `.` entry standing for the directory being browsed, so that directory can be picked itself, even when it is empty. `Enter` on it confirms; it cannot be entered or expanded.
    DirsOnly,
    /// Files and directories may both be picked.
    Both,
}

/// Which view the picker starts in, as passed to [`FilePickerBuilder::view`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// One directory at a time.
    List,
    /// Directories expand in place.
    Tree,
}

/// What the user has done so far, as reported by [`FilePickerState::result`].
///
/// Poll this after handing the picker an event. It stays [`Selected`](Self::Selected) or [`Cancelled`](Self::Cancelled) once set, so the application decides when to drop the picker rather than the picker resetting itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerResult {
    /// Still browsing. Keep rendering and feeding it events.
    Pending,
    /// The user confirmed these paths, sorted. Holds the whole multi-selection if there was one, otherwise the single entry under the cursor.
    Selected(Vec<PathBuf>),
    /// The user pressed `Esc` or `q`.
    Cancelled,
}

/// Whether keystrokes are commands or search text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Keys act as commands. `/` switches to [`Search`](Self::Search).
    Normal,
    /// Printable keys append to the query, including `j` and `k`. Move through results with the arrow keys, `Ctrl+N`/`Ctrl+P` or `Ctrl+J`/`Ctrl+K`.
    Search,
}

// ---------------------------------------------------------------------------
// CommonState
// ---------------------------------------------------------------------------

/// Everything the picker tracks that does not depend on which view is active.
///
/// Both views read and write this same struct, which is why toggling views keeps the selection, the search query and the current directory intact. The fields are public so an application can inspect or drive the picker without going through the key map.
///
/// The struct is `#[non_exhaustive]`, so it can only be created through [`FilePickerState::builder`]. That is what lets a later release add a field without breaking existing code.
///
/// ```compile_fail
/// let state: ratatree::CommonState = ratatree::CommonState { ..unimplemented!() };
/// ```
#[non_exhaustive]
pub struct CommonState {
    /// The directory being listed. Canonicalized by [`FilePickerBuilder::build`] and kept canonical thereafter.
    pub current_dir: PathBuf,
    /// The rows to draw. In list view these are the entries of [`current_dir`](Self::current_dir); in tree view they are that directory plus every expanded descendant, flattened in display order. In [`PickerMode::DirsOnly`] the first entry is `.`, whose `path` is `current_dir` itself.
    pub entries: Vec<Entry>,
    /// Indices into [`entries`](Self::entries) matching the search query, or `None` when no search is active.
    pub filtered_indices: Option<Vec<usize>>,
    /// Paths toggled with `Space`. Confirming returns these instead of the entry under the cursor. Survives navigation, so a selection can span directories.
    pub selected: HashSet<PathBuf>,
    /// Whether dotfiles are listed.
    pub show_hidden: bool,
    /// What the picker is allowed to return.
    pub mode: PickerMode,
    /// Whether keys are commands or search text.
    pub input_mode: InputMode,
    /// The current search query. Empty when no search is active.
    pub search_query: String,
    /// First half of a pending two-key sequence such as `gg`, with the time it was pressed so a stale prefix expires instead of arming forever.
    pub pending_key: Option<(char, Instant)>,
    /// A one-off message shown in place of the status bar, such as `Circular symlink`. Cleared on the next handled event.
    pub error_message: Option<String>,
    /// Why [`current_dir`](Self::current_dir) could not be listed, such as `Cannot read directory: permission denied`, or `None` when the last read succeeded. Unlike [`error_message`](Self::error_message) this stays until a directory read succeeds, so the status bar keeps explaining an empty listing for as long as it is empty.
    pub read_error: Option<String>,
    /// What the user has done so far.
    pub result: PickerResult,
    /// The file filter, if one was set.
    pub filter: FilterFn,
    /// Styles used when drawing.
    pub theme: FilePickerTheme,
    /// Screen area of the entry list from the last render. Mouse clicks and page movements are resolved against it. Zero-sized until first render.
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
            .field("error_message", &self.error_message)
            .field("read_error", &self.read_error)
            .field("result", &self.result)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// FilePickerState
// ---------------------------------------------------------------------------

/// The picker itself: hand it events, hand it to the widget, read its result.
///
/// Build one with [`builder`](Self::builder), then per frame render it with [`FilePicker`](crate::FilePicker), pass the terminal event to [`handle_event`](Self::handle_event), and check [`result`](Self::result).
///
/// ```
/// use ratatree::{FilePickerState, PickerMode};
///
/// let state = FilePickerState::builder()
///     .start_dir(".")
///     .mode(PickerMode::Both)
///     .build();
///
/// assert!(state.visible_count() > 0);
/// ```
///
/// To use a different key map, ignore [`handle_event`](Self::handle_event) and call the movement and action methods directly.
///
/// Like [`CommonState`] this is `#[non_exhaustive]`: build one with the builder, not a struct literal, and destructure it with `..` so a future field does not break the pattern.
///
/// ```compile_fail
/// let state = ratatree::FilePickerState::builder().build();
/// let ratatree::FilePickerState { common, view } = state;
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct FilePickerState {
    /// State shared by both views.
    pub common: CommonState,
    /// The active view and its cursor, scroll and expansion state.
    pub view: ViewState,
}

impl FilePickerState {
    /// Starts building a picker. See [`FilePickerBuilder`] for the options.
    pub fn builder() -> FilePickerBuilder {
        FilePickerBuilder::default()
    }

    // --- Result ---

    /// What the user has done so far. Poll this after each event.
    pub fn result(&self) -> PickerResult {
        self.common.result.clone()
    }

    // --- Visible entries helpers ---

    /// The entries currently on screen, which is the search matches while a search is active and every entry otherwise.
    ///
    /// The cursor indexes into this list, not into [`CommonState::entries`].
    pub fn visible_entries(&self) -> Vec<&Entry> {
        match &self.common.filtered_indices {
            Some(indices) => indices
                .iter()
                .filter_map(|&i| self.common.entries.get(i))
                .collect(),
            None => self.common.entries.iter().collect(),
        }
    }

    /// How many entries are currently visible, without building the list.
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

    /// The entry under the cursor, or `None` if the listing is empty.
    pub fn current_entry(&self) -> Option<&Entry> {
        let cursor = self.view.cursor();
        let actual = self.actual_index(cursor)?;
        self.common.entries.get(actual)
    }

    // --- Directory refresh ---

    /// Re-reads the current directory from disk and rebuilds the entry list.
    ///
    /// In list view that is the directory's own entries; in tree view it is the directory plus every expanded descendant, flattened. Clears any active search filter and pulls the cursor back into range. Call this after changing the filesystem behind the picker's back.
    pub fn refresh_entries(&mut self) {
        let dir = self.common.current_dir.clone();
        let show_hidden = self.common.show_hidden;
        let filter = self.common.filter.as_deref();
        let read = match &self.view {
            ViewState::List(_) => read_entries(&dir, show_hidden, filter),
            ViewState::Tree(tree) => tree.build_tree_entries(&dir, show_hidden, filter),
        };
        self.common.entries = match read {
            Ok(mut entries) => {
                self.common.read_error = None;
                if self.common.mode == PickerMode::DirsOnly {
                    entries.insert(0, current_dir_entry(&dir));
                }
                entries
            }
            Err(err) => {
                self.common.read_error = Some(read_failure_message(&err));
                Vec::new()
            }
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

    /// Tree view only: shows the children of the directory under the cursor, keeping the cursor on it. Does nothing on a file or symlink.
    pub fn expand_current(&mut self) {
        self.set_expanded_current(true);
    }

    /// Tree view only: hides the children of the directory under the cursor, keeping the cursor on it. Does nothing on a file or symlink.
    pub fn collapse_current(&mut self) {
        self.set_expanded_current(false);
    }

    /// Tree view only: expands the directory under the cursor if it is collapsed, and collapses it if it is expanded.
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
        // Expanding the `.` entry would list the current directory inside itself.
        if entry.path == self.common.current_dir {
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

    /// Moves the cursor to the visible entry with this path. Returns false, leaving the cursor alone, if it is not visible.
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
    ///
    /// In list view this enters the directory. In tree view it expands a collapsed directory, moves to the first child of an expanded one, and enters a symlink as the new root because symlinks are never expanded in place. Files are ignored in both views.
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
    ///
    /// In list view this goes to the parent directory. In tree view it collapses an expanded directory, moves a nested entry to its parent node, and goes to the parent directory from a top-level entry.
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

    /// Shows or hides dotfiles, then re-reads the listing.
    pub fn toggle_hidden(&mut self) {
        self.common.show_hidden = !self.common.show_hidden;
        self.refresh_entries();
    }

    /// Adds or removes the entry under the cursor from the multi-selection.
    ///
    /// Does nothing if the current [`PickerMode`] would not allow picking that kind of entry.
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

    /// Whether an entry of this kind may be picked under the current [`PickerMode`]. Symlinks are always allowed because their target kind is not known here.
    fn is_selectable(&self, kind: &EntryKind) -> bool {
        match self.common.mode {
            PickerMode::FilesOnly => *kind != EntryKind::Directory,
            PickerMode::DirsOnly => *kind != EntryKind::File,
            PickerMode::Both => true,
        }
    }

    // --- Confirm / Cancel ---

    /// Acts on `Enter`.
    ///
    /// A non-empty multi-selection wins and is returned as [`PickerResult::Selected`], sorted. Otherwise, a directory under the cursor is entered in list view or expanded and collapsed in tree view, and any other entry the current [`PickerMode`] permits is returned on its own.
    pub fn confirm(&mut self) {
        if !self.common.selected.is_empty() {
            let mut paths: Vec<PathBuf> = self.common.selected.iter().cloned().collect();
            paths.sort();
            self.common.result = PickerResult::Selected(paths);
            return;
        }

        match self.current_entry() {
            // The `.` entry stands for the directory being browsed: it is picked, never entered.
            Some(entry)
                if entry.kind == EntryKind::Directory && entry.path == self.common.current_dir =>
            {
                let path = entry.path.clone();
                self.common.result = PickerResult::Selected(vec![path]);
            }
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

    /// Sets the result to [`PickerResult::Cancelled`].
    pub fn cancel(&mut self) {
        self.common.result = PickerResult::Cancelled;
    }

    /// Puts a finished picker back to [`PickerResult::Pending`] so it can be shown again.
    ///
    /// Clears the selection, the search query and filter, the pending key prefix and the error message, and returns to [`InputMode::Normal`] if the picker was in search mode. The current directory, cursor, scroll and tree expansion are kept, so the user resumes where they left off. Nothing is re-read from disk; call [`refresh_entries`](Self::refresh_entries) for that.
    pub fn reset(&mut self) {
        // The cursor indexes the filtered list while a search is active, so it has to be re-found by path once the filter is gone.
        let keep = self.current_entry().map(|e| e.path.clone());
        self.common.result = PickerResult::Pending;
        self.common.selected.clear();
        self.common.input_mode = InputMode::Normal;
        self.common.search_query.clear();
        self.common.filtered_indices = None;
        self.common.pending_key = None;
        self.common.error_message = None;
        let kept = keep.map(|p| self.move_cursor_to_path(&p)).unwrap_or(false);
        if !kept {
            self.clamp_cursor();
        }
    }

    // --- Directory navigation ---

    /// Makes the directory under the cursor the new current directory, resetting the cursor and scroll.
    ///
    /// Files are ignored. Symlinks are followed, except one resolving to the current directory or an ancestor of it, which would only lead back to where the user already is; that sets `Circular symlink` in [`CommonState::error_message`] instead.
    pub fn enter_directory(&mut self) {
        let entry = match self.current_entry() {
            Some(e) => e,
            None => return,
        };

        if entry.kind == EntryKind::File {
            return;
        }
        // The `.` entry already is the current directory.
        if entry.path == self.common.current_dir {
            return;
        }
        let is_symlink = entry.kind == EntryKind::Symlink;
        let Ok(canonical) = dunce::canonicalize(&entry.path) else {
            return;
        };
        if !canonical.is_dir() {
            return;
        }

        // A symlink that resolves to the current directory or one of its ancestors would only lead back to where we already are.
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
        dunce::canonicalize(&self.common.current_dir)
            .unwrap_or_else(|_| self.common.current_dir.clone())
    }

    /// Moves to the parent of the current directory, resetting the cursor and scroll. Does nothing at the filesystem root.
    pub fn go_parent(&mut self) {
        if let Some(parent) = self.common.current_dir.parent().map(|p| p.to_path_buf()) {
            self.common.current_dir = parent;
            *self.view.cursor_mut() = 0;
            *self.view.scroll_offset_mut() = 0;
            self.refresh_entries();
        }
    }

    /// Moves to the user's home directory. Does nothing if there is no home directory to find.
    pub fn go_home(&mut self) {
        if let Some(home) = dirs::home_dir() {
            self.common.current_dir = home;
            *self.view.cursor_mut() = 0;
            *self.view.scroll_offset_mut() = 0;
            self.refresh_entries();
        }
    }

    // --- Cursor movement ---

    /// Moves the cursor one entry down, stopping at the last one.
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

    /// Moves the cursor one entry up, stopping at the first one.
    pub fn move_cursor_up(&mut self) {
        let cursor = self.view.cursor_mut();
        if *cursor > 0 {
            *cursor -= 1;
        }
    }

    /// Moves the cursor to the first entry.
    pub fn move_to_top(&mut self) {
        *self.view.cursor_mut() = 0;
    }

    /// Moves the cursor to the last entry.
    pub fn move_to_bottom(&mut self) {
        let count = self.visible_count();
        if count > 0 {
            *self.view.cursor_mut() = count - 1;
        }
    }

    /// Number of entry rows shown by the last render, or `20` before the widget has been drawn.
    pub fn page_height(&self) -> usize {
        match self.common.list_area.height {
            0 => 20,
            h => h as usize,
        }
    }

    /// Moves the cursor down half of `page_height` entries, stopping at the last one. Pass [`page_height`](Self::page_height) for the height actually on screen.
    pub fn move_half_page_down(&mut self, page_height: usize) {
        let half = page_height / 2;
        let count = self.visible_count();
        if count == 0 {
            return;
        }
        let cursor = self.view.cursor_mut();
        *cursor = (*cursor + half).min(count - 1);
    }

    /// Moves the cursor up half of `page_height` entries, stopping at the first one.
    pub fn move_half_page_up(&mut self, page_height: usize) {
        let half = page_height / 2;
        let cursor = self.view.cursor_mut();
        *cursor = cursor.saturating_sub(half);
    }

    // --- View toggle ---

    /// Switches between the list and tree views.
    ///
    /// Rebuilds the entry list for the new view and keeps the cursor on the same path where that path is still visible, falling back to the first entry. Expansion state is not carried across, so returning to the tree view starts collapsed.
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

    /// Pulls the cursor back into range after the visible entries changed underneath it.
    ///
    /// The navigation methods call this themselves. It is public for applications that edit [`CommonState::entries`] or [`CommonState::filtered_indices`] directly.
    pub fn clamp_cursor(&mut self) {
        let count = self.visible_count();
        let cursor = self.view.cursor_mut();
        if count == 0 {
            *cursor = 0;
        } else if *cursor >= count {
            *cursor = count - 1;
        }
    }

    // --- Event handling ---

    /// Applies the crate's default key and mouse map to one terminal event.
    ///
    /// The event type comes from [`ratatree::crossterm`](crate::crossterm), the re-export this crate was built against. Key releases are ignored, so terminals that report them do not act twice. Mouse clicks are resolved against [`CommonState::list_area`] and are therefore ignored before the first render.
    ///
    /// Applications wanting their own bindings can skip this and call the state methods directly.
    pub fn handle_event(&mut self, event: ratatui::crossterm::event::Event) {
        crate::event::handle_event(self, event);
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builds a [`FilePickerState`], created by [`FilePickerState::builder`].
///
/// Every option has a default, so `FilePickerState::builder().build()` browses the process's current directory for files and directories alike.
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
    /// Where to start browsing. Defaults to the process's current directory.
    ///
    /// A leading `~` is expanded to the home directory and the result is canonicalized, so `"."` and `"~/projects"` both become absolute paths. A path that does not exist is expanded but otherwise left alone.
    pub fn start_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.start_dir = Some(dir.into());
        self
    }

    /// What the picker may return. Defaults to [`PickerMode::Both`].
    pub fn mode(mut self, mode: PickerMode) -> Self {
        self.mode = mode;
        self
    }

    /// Which view to start in. Defaults to [`ViewMode::List`].
    pub fn view(mut self, view_mode: ViewMode) -> Self {
        self.view_mode = view_mode;
        self
    }

    /// Shows only the files this predicate accepts.
    ///
    /// Directories are never passed to it and always remain visible, so filtering by extension does not make subdirectories unreachable.
    ///
    /// ```
    /// use ratatree::FilePickerState;
    ///
    /// let state = FilePickerState::builder()
    ///     .filter(|path| path.extension().is_some_and(|e| e == "rs"))
    ///     .build();
    /// ```
    pub fn filter(mut self, f: impl Fn(&Path) -> bool + 'static) -> Self {
        self.filter = Some(Box::new(f));
        self
    }

    /// Colors and modifiers to draw with. Defaults to [`FilePickerTheme::default`].
    pub fn theme(mut self, theme: FilePickerTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Whether to list dotfiles. Defaults to `false`.
    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    /// Builds the picker and reads the starting directory.
    pub fn build(self) -> FilePickerState {
        let start_dir = self
            .start_dir
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let current_dir = resolve_start_dir(start_dir);

        let (mut entries, read_error) =
            match read_entries(&current_dir, self.show_hidden, self.filter.as_deref()) {
                Ok(entries) => (entries, None),
                Err(err) => (Vec::new(), Some(read_failure_message(&err))),
            };
        if read_error.is_none() && self.mode == PickerMode::DirsOnly {
            entries.insert(0, current_dir_entry(&current_dir));
        }

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
            read_error,
            result: PickerResult::Pending,
            filter: self.filter,
            theme: self.theme,
            list_area: Rect::default(),
        };

        FilePickerState { common, view }
    }
}

/// The `.` entry that heads a `DirsOnly` listing so the directory being browsed can itself be picked, including when it is empty.
fn current_dir_entry(current_dir: &Path) -> Entry {
    Entry {
        name: ".".to_string(),
        path: current_dir.to_path_buf(),
        kind: EntryKind::Directory,
        is_hidden: false,
        depth: 0,
    }
}

/// Turns a failed directory read into something short enough for the status bar.
fn read_failure_message(err: &std::io::Error) -> String {
    format!("Cannot read directory: {}", err.kind())
}

/// Expands a leading `~` to the home directory and resolves the result to an absolute path with symlinks removed. A path that does not exist is kept as is, after tilde expansion, so the picker can still show it in the path bar.
fn resolve_start_dir(dir: PathBuf) -> PathBuf {
    let expanded = expand_tilde(dir);
    dunce::canonicalize(&expanded).unwrap_or(expanded)
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
        let root = dunce::canonicalize(tmp.path()).unwrap().join("root");
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
        state
            .visible_entries()
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    #[test]
    fn tree_toggle_expand_shows_and_hides_children() {
        let (_tmp, root) = make_tree_dir();
        let mut state = tree_state(&root);
        assert_eq!(names(&state), ["a_dir", "b_dir", "top.txt"]);

        state.toggle_expand_current();
        assert_eq!(
            names(&state),
            ["a_dir", "nested", "inner.txt", "b_dir", "top.txt"]
        );
        assert_eq!(state.visible_entries()[1].depth, 1);
        assert_eq!(
            state.view.cursor(),
            0,
            "cursor stays on the expanded directory"
        );

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
        assert_eq!(
            state.common.current_dir, root,
            "root is unchanged in tree view"
        );
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
        assert_eq!(
            state.common.current_dir,
            dunce::canonicalize(root.parent().unwrap()).unwrap()
        );
    }

    #[test]
    fn descend_and_ascend_change_root_in_list_view() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder().start_dir(&root).build();

        state.descend();
        assert!(state.common.current_dir.ends_with("a_dir"));
        state.ascend();
        assert_eq!(
            state.common.current_dir,
            dunce::canonicalize(&root).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn descend_on_symlink_in_tree_view_follows_it() {
        let (_tmp, root) = make_tree_dir();
        std::os::unix::fs::symlink(root.join("b_dir"), root.join("link")).unwrap();
        let mut state = tree_state(&root);
        let idx = state
            .common
            .entries
            .iter()
            .position(|e| e.name == "link")
            .unwrap();
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
        let state = FilePickerState::builder().start_dir(dir.path()).build();

        assert_eq!(state.common.mode, PickerMode::Both);
        assert!(!state.common.show_hidden);
        assert_eq!(state.common.result, PickerResult::Pending);
        assert!(matches!(state.view, ViewState::List(_)));
        assert_eq!(state.common.input_mode, InputMode::Normal);
    }

    #[test]
    fn builder_canonicalizes_relative_start_dir() {
        let cwd = dunce::canonicalize(std::env::current_dir().unwrap()).unwrap();
        let mut state = FilePickerState::builder().start_dir(".").build();

        assert_eq!(state.common.current_dir, cwd);

        // Going up from a resolved path lands in the real parent, not "".
        state.go_parent();
        assert_eq!(state.common.current_dir, cwd.parent().unwrap());
        assert!(
            state.visible_count() > 0,
            "parent directory should list entries"
        );
    }

    #[test]
    fn builder_expands_tilde() {
        let home = dirs::home_dir().unwrap();

        let state = FilePickerState::builder().start_dir("~").build();
        assert_eq!(
            state.common.current_dir,
            dunce::canonicalize(&home).unwrap()
        );

        // "~/" prefix is expanded; a nonexistent target keeps the expanded path, so it is compared against the plain home rather than a canonicalized one.
        let state = FilePickerState::builder()
            .start_dir("~/ratatree-nonexistent-dir")
            .build();
        assert_eq!(
            state.common.current_dir,
            home.join("ratatree-nonexistent-dir")
        );
    }

    /// std's canonicalize yields `\\?\C:\...` on Windows. That prefix must not leak into current_dir, because every entry path and every returned path is derived from it.
    #[cfg(windows)]
    #[test]
    fn windows_paths_carry_no_verbatim_prefix() {
        fn is_verbatim(path: &Path) -> bool {
            path.to_string_lossy().starts_with(r"\\?\")
        }
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();
        assert!(
            !is_verbatim(&state.common.current_dir),
            "start_dir: {:?}",
            state.common.current_dir
        );

        state.enter_directory(); // subdir sorts first
        assert!(state.common.current_dir.ends_with("subdir"));
        assert!(
            !is_verbatim(&state.common.current_dir),
            "enter_directory: {:?}",
            state.common.current_dir
        );
    }

    #[test]
    fn unreadable_directory_reports_why_instead_of_looking_empty() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("no-such-directory");

        let state = FilePickerState::builder().start_dir(&missing).build();

        assert!(state.common.entries.is_empty());
        assert!(
            state.common.read_error.is_some(),
            "an unreadable directory must say why, not just render as empty"
        );
        assert_eq!(
            state.common.error_message, None,
            "the read failure lives in read_error only, not duplicated into the transient message"
        );
    }

    #[test]
    fn read_error_clears_once_a_directory_can_be_read() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("marker.txt"), b"").unwrap();
        let missing = dir.path().join("no-such-directory");
        let mut state = FilePickerState::builder().start_dir(&missing).build();
        assert!(state.common.read_error.is_some());

        state.go_parent();

        assert_eq!(state.common.read_error, None);
        assert_eq!(
            state.common.entries.len(),
            1,
            "the parent was actually read"
        );
    }

    #[test]
    fn tree_view_reports_an_unreadable_root_too() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .view(ViewMode::Tree)
            .build();
        assert_eq!(state.common.read_error, None);

        // Navigate into a directory that cannot be read, the way go_parent or enter_directory would.
        state.common.current_dir = dir.path().join("no-such-directory");
        state.refresh_entries();

        assert!(state.common.entries.is_empty());
        assert!(
            state.common.read_error.is_some(),
            "the tree view must not swallow a root read failure"
        );
    }

    #[test]
    fn tree_view_clears_read_error_once_root_can_be_read() {
        let dir = make_dir_with_files();
        let missing = dir.path().join("no-such-directory");
        let mut state = FilePickerState::builder()
            .start_dir(&missing)
            .view(ViewMode::Tree)
            .build();
        assert!(state.common.read_error.is_some());

        state.go_parent();

        assert_eq!(state.common.read_error, None);
        assert_eq!(
            state.common.entries.len(),
            3,
            "the parent was actually read"
        );
    }

    #[test]
    fn read_error_survives_key_presses() {
        use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("no-such-directory");
        let mut state = FilePickerState::builder().start_dir(&missing).build();
        let before = state.common.read_error.clone();
        assert!(before.is_some());

        state.handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));

        assert_eq!(
            state.common.read_error, before,
            "a directory that is still unreadable keeps saying so"
        );
    }

    #[test]
    fn dirs_only_lists_the_current_directory_first() {
        let dir = make_dir_with_files();
        let state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();

        let first = &state.common.entries[0];
        assert_eq!(first.name, ".");
        assert_eq!(first.path, state.common.current_dir);
        assert_eq!(first.kind, EntryKind::Directory);
        assert_eq!(first.depth, 0);
        assert_eq!(
            state.common.entries.len(),
            4,
            "subdir, alpha.txt, beta.rs plus the dot entry"
        );
    }

    #[test]
    fn other_modes_do_not_list_the_current_directory() {
        let dir = make_dir_with_files();
        for mode in [PickerMode::FilesOnly, PickerMode::Both] {
            let state = FilePickerState::builder()
                .start_dir(dir.path())
                .mode(mode)
                .build();
            assert_ne!(state.common.entries[0].name, ".", "{mode:?}");
            assert_eq!(state.common.entries.len(), 3, "{mode:?}");
        }
    }

    #[test]
    fn dirs_only_lists_the_current_directory_even_when_empty() {
        let dir = TempDir::new().unwrap();
        let state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();

        assert_eq!(state.common.entries.len(), 1);
        assert_eq!(state.common.entries[0].name, ".");
    }

    #[test]
    fn dirs_only_has_no_current_directory_entry_when_the_read_fails() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("no-such-directory");
        let state = FilePickerState::builder()
            .start_dir(&missing)
            .mode(PickerMode::DirsOnly)
            .build();

        assert!(
            state.common.entries.is_empty(),
            "a directory that cannot be read must not be offered for picking"
        );
        assert!(state.common.read_error.is_some());
    }

    #[test]
    fn dirs_only_keeps_exactly_one_current_directory_entry_after_refresh() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();

        state.toggle_hidden(); // goes through refresh_entries
        state.refresh_entries();

        let dots = state
            .common
            .entries
            .iter()
            .filter(|e| e.name == ".")
            .count();
        assert_eq!(dots, 1);
        assert_eq!(state.common.entries[0].name, ".");
    }

    #[test]
    fn tree_view_lists_the_current_directory_once_at_the_root() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder()
            .start_dir(&root)
            .mode(PickerMode::DirsOnly)
            .view(ViewMode::Tree)
            .build();
        assert_eq!(state.common.entries[0].name, ".");

        *state.view.cursor_mut() = 1; // a_dir
        state.expand_current();

        let dots: Vec<&Entry> = state
            .common
            .entries
            .iter()
            .filter(|e| e.name == ".")
            .collect();
        assert_eq!(
            dots.len(),
            1,
            "expanded subdirectories must not get their own dot entry"
        );
        assert_eq!(dots[0].depth, 0);
        assert!(
            state.common.entries.iter().any(|e| e.name == "inner.txt"),
            "a_dir was expanded"
        );
    }

    #[test]
    fn confirming_the_current_directory_entry_returns_the_current_directory() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();
        assert_eq!(state.current_entry().unwrap().name, ".");

        state.confirm();

        assert_eq!(
            state.result(),
            PickerResult::Selected(vec![state.common.current_dir.clone()])
        );
    }

    #[test]
    fn confirming_the_current_directory_entry_works_in_an_empty_directory() {
        let dir = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();

        state.confirm();

        assert_eq!(
            state.result(),
            PickerResult::Selected(vec![state.common.current_dir.clone()])
        );
    }

    #[test]
    fn the_current_directory_entry_can_be_multi_selected_across_directories() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder()
            .start_dir(&root)
            .mode(PickerMode::DirsOnly)
            .build();

        state.toggle_select(); // "." in root
        *state.view.cursor_mut() = 1; // a_dir
        state.enter_directory();
        assert_eq!(state.current_entry().unwrap().name, ".");
        state.toggle_select(); // "." in a_dir
        state.confirm();

        assert_eq!(
            state.result(),
            PickerResult::Selected(vec![root.clone(), root.join("a_dir")])
        );
    }

    #[test]
    fn the_current_directory_entry_cannot_be_entered() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();
        let before = state.common.current_dir.clone();
        let count = state.common.entries.len();
        // Entering a directory resets the scroll offset; a real no-op leaves it alone.
        *state.view.scroll_offset_mut() = 1;

        state.enter_directory();
        state.descend();

        assert_eq!(state.common.current_dir, before);
        assert_eq!(state.common.entries.len(), count);
        assert_eq!(state.current_entry().unwrap().name, ".");
        assert_eq!(
            state.view.scroll_offset(),
            1,
            "enter_directory must not run at all on ."
        );
    }

    #[test]
    fn the_current_directory_entry_cannot_be_expanded() {
        let (_tmp, root) = make_tree_dir();
        let mut state = FilePickerState::builder()
            .start_dir(&root)
            .mode(PickerMode::DirsOnly)
            .view(ViewMode::Tree)
            .build();
        let count = state.common.entries.len();

        state.expand_current();
        state.toggle_expand_current();
        state.descend();

        assert_eq!(
            state.common.entries.len(),
            count,
            "expanding . would list the root twice"
        );
        let ViewState::Tree(tree) = &state.view else {
            panic!("tree view expected");
        };
        assert!(!tree.is_expanded(&state.common.current_dir));
    }

    #[test]
    fn entering_a_directory_puts_the_cursor_on_its_current_directory_entry() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();
        *state.view.cursor_mut() = 1; // subdir

        state.enter_directory();

        assert!(state.common.current_dir.ends_with("subdir"));
        assert_eq!(state.current_entry().unwrap().name, ".");
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
        let names: Vec<&str> = state
            .common
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names.contains(&"alpha.txt"), "alpha.txt should be included");
        assert!(
            !names.contains(&"beta.rs"),
            "beta.rs should be filtered out"
        );
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
        assert!(
            state.common.selected.is_empty(),
            "should not select a directory in FilesOnly mode"
        );
    }

    #[test]
    fn dirs_only_confirm_on_file_does_nothing() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("file.txt"), b"").unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .mode(PickerMode::DirsOnly)
            .build();

        *state.view.cursor_mut() = 1; // past the `.` entry
        assert_eq!(state.current_entry().unwrap().kind, EntryKind::File);
        state.confirm();
        assert_eq!(state.result(), PickerResult::Pending);
    }

    #[test]
    fn toggle_hidden() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("visible.txt"), b"").unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"").unwrap();

        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        assert_eq!(state.visible_count(), 1);

        state.toggle_hidden();
        assert_eq!(state.visible_count(), 2);

        state.toggle_hidden();
        assert_eq!(state.visible_count(), 1);
    }

    #[test]
    fn multi_select_toggle() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

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

        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

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
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        // Select two files
        let file_indices: Vec<usize> = state
            .common
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == EntryKind::File)
            .map(|(i, _)| i)
            .collect();

        assert!(
            file_indices.len() >= 2,
            "need at least 2 files for this test"
        );

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
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        // subdir is first (dirs sort first)
        state.enter_directory();
        assert!(state.common.current_dir.ends_with("subdir"));

        state.go_parent();
        assert!(!state.common.current_dir.ends_with("subdir"));

        // Entering the same directory a second time must work and must not be mistaken for a circular symlink.
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
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        assert_eq!(state.result(), PickerResult::Pending);
        state.cancel();
        assert_eq!(state.result(), PickerResult::Cancelled);
    }
}
