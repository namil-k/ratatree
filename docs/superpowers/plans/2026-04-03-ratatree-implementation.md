# ratatree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a ratatui file/directory picker widget crate with list+tree views, multi-select, fuzzy search, vim+arrow key bindings, and basic mouse support.

**Architecture:** Enum-based view dispatch with shared common state. `FilePickerState` owns `CommonState` (path, entries, selection, search) and `ViewState` (List or Tree variant). `FilePicker` implements `StatefulWidget` for rendering. Builder pattern for configuration.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28, dirs 5, tempfile 3 (dev)

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib.rs` | Public module declarations and re-exports |
| `src/entry.rs` | `Entry` struct, `EntryKind` enum, directory reading with symlink/hidden handling |
| `src/theme.rs` | `FilePickerTheme` with ratatui `Style` fields and `Default` impl |
| `src/search.rs` | Fuzzy matching: score function and filter-by-query |
| `src/view/mod.rs` | `ViewState` enum, `ListViewState`, `TreeViewState` structs |
| `src/view/list.rs` | List view rendering and cursor navigation |
| `src/view/tree.rs` | Tree view rendering, expand/collapse, cursor navigation |
| `src/state.rs` | `FilePickerState`, `CommonState`, `InputMode`, `PickerResult`, `PickerMode`, `ViewMode`, builder |
| `src/event.rs` | `handle_event`: key dispatch, mouse dispatch, gg sequence, search input |
| `src/widget.rs` | `FilePicker` struct, `StatefulWidget` impl (path bar, file list, status bar) |
| `examples/basic.rs` | Runnable example with full event loop |

---

### Task 1: Project Setup & Entry Type

**Files:**
- Modify: `Cargo.toml`
- Create: `src/entry.rs`
- Create: `src/lib.rs` (replace existing)

- [ ] **Step 1: Add dev-dependency to Cargo.toml**

```toml
[dev-dependencies]
tempfile = "3"
```

Add after the `[dependencies]` section in `Cargo.toml`.

- [ ] **Step 2: Write tests for Entry type and directory reading**

Write at the bottom of `src/entry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn entry_from_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        fs::write(&file_path, "hello").unwrap();

        let entry = Entry::from_path(&file_path).unwrap();
        assert_eq!(entry.name, "test.rs");
        assert_eq!(entry.kind, EntryKind::File);
        assert!(!entry.is_hidden);
    }

    #[test]
    fn entry_from_directory() {
        let tmp = TempDir::new().unwrap();
        let dir_path = tmp.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let entry = Entry::from_path(&dir_path).unwrap();
        assert_eq!(entry.name, "subdir");
        assert_eq!(entry.kind, EntryKind::Directory);
    }

    #[test]
    fn entry_hidden_detection() {
        let tmp = TempDir::new().unwrap();
        let hidden = tmp.path().join(".hidden");
        fs::write(&hidden, "").unwrap();

        let entry = Entry::from_path(&hidden).unwrap();
        assert!(entry.is_hidden);
    }

    #[test]
    fn read_entries_filters_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();

        let entries = read_entries(tmp.path(), false, None);
        assert_eq!(entries.len(), 2); // visible.txt + subdir
        assert!(entries.iter().all(|e| !e.is_hidden));
    }

    #[test]
    fn read_entries_shows_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();

        let entries = read_entries(tmp.path(), true, None);
        assert_eq!(entries.len(), 2); // visible.txt + .hidden
    }

    #[test]
    fn read_entries_sorts_dirs_first() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("zebra.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("alpha")).unwrap();
        fs::write(tmp.path().join("beta.txt"), "").unwrap();

        let entries = read_entries(tmp.path(), false, None);
        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[0].kind, EntryKind::Directory);
    }

    #[test]
    fn read_entries_with_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("keep.rs"), "").unwrap();
        fs::write(tmp.path().join("skip.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();

        let filter = |path: &Path| -> bool {
            path.is_dir()
                || path.extension().map(|e| e == "rs").unwrap_or(false)
        };
        let entries = read_entries(tmp.path(), false, Some(&filter));
        assert_eq!(entries.len(), 2); // keep.rs + subdir
    }

    #[test]
    fn read_entries_symlink_detection() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("target.txt");
        fs::write(&target, "").unwrap();

        let link = tmp.path().join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();

        #[cfg(unix)]
        {
            let entry = Entry::from_path(&link).unwrap();
            assert_eq!(entry.kind, EntryKind::Symlink);
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib entry::tests`
Expected: compilation error — `Entry`, `EntryKind`, `read_entries` not defined.

- [ ] **Step 4: Implement Entry type and read_entries**

Write `src/entry.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
    pub is_hidden: bool,
}

impl Entry {
    pub fn from_path(path: &Path) -> Option<Entry> {
        let name = path.file_name()?.to_string_lossy().to_string();
        let is_hidden = name.starts_with('.');

        let metadata = fs::symlink_metadata(path).ok()?;
        let kind = if metadata.is_symlink() {
            EntryKind::Symlink
        } else if metadata.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        };

        Some(Entry {
            name,
            path: path.to_path_buf(),
            kind,
            is_hidden,
        })
    }
}

pub fn read_entries(
    dir: &Path,
    show_hidden: bool,
    filter: Option<&dyn Fn(&Path) -> bool>,
) -> Vec<Entry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut entries: Vec<Entry> = read_dir
        .filter_map(|e| e.ok())
        .filter_map(|e| Entry::from_path(&e.path()))
        .filter(|e| show_hidden || !e.is_hidden)
        .filter(|e| {
            filter
                .map(|f| e.kind == EntryKind::Directory || f(&e.path))
                .unwrap_or(true)
        })
        .collect();

    entries.sort_by(|a, b| {
        let dir_ord = matches!(b.kind, EntryKind::Directory)
            .cmp(&matches!(a.kind, EntryKind::Directory));
        dir_ord.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries
}
```

- [ ] **Step 5: Update lib.rs with module declaration**

Replace `src/lib.rs` with:

```rust
mod entry;

pub use entry::{Entry, EntryKind};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib entry::tests`
Expected: all 7 tests pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/entry.rs src/lib.rs
git commit -m "feat: add Entry type with directory reading, filtering, and symlink detection"
```

---

### Task 2: Theme

**Files:**
- Create: `src/theme.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write test for theme defaults**

Write at the bottom of `src/theme.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_has_distinct_styles() {
        let theme = FilePickerTheme::default();
        // Cursor and selected should have visible backgrounds
        assert_ne!(theme.cursor, Style::default());
        assert_ne!(theme.selected, Style::default());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib theme::tests`
Expected: compilation error — `FilePickerTheme` not defined.

- [ ] **Step 3: Implement FilePickerTheme**

Write `src/theme.rs`:

```rust
use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct FilePickerTheme {
    pub normal: Style,
    pub cursor: Style,
    pub selected: Style,
    pub directory: Style,
    pub symlink: Style,
    pub path_bar: Style,
    pub status_bar: Style,
    pub search_input: Style,
    pub error: Style,
}

impl Default for FilePickerTheme {
    fn default() -> Self {
        Self {
            normal: Style::default(),
            cursor: Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
            selected: Style::default().fg(Color::Green),
            directory: Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
            symlink: Style::default().fg(Color::Cyan),
            path_bar: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            status_bar: Style::default().fg(Color::DarkGray),
            search_input: Style::default().fg(Color::Yellow),
            error: Style::default().fg(Color::Red),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_has_distinct_styles() {
        let theme = FilePickerTheme::default();
        assert_ne!(theme.cursor, Style::default());
        assert_ne!(theme.selected, Style::default());
    }
}
```

- [ ] **Step 4: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
mod theme;

pub use theme::FilePickerTheme;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib theme::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/theme.rs src/lib.rs
git commit -m "feat: add FilePickerTheme with ratatui Style customization"
```

---

### Task 3: Fuzzy Search

**Files:**
- Create: `src/search.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write tests for fuzzy matching**

Write at the bottom of `src/search.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_scores_highest() {
        let score = fuzzy_score("lib.rs", "lib.rs");
        assert!(score.is_some());
        let partial = fuzzy_score("lib.rs", "lib");
        assert!(score.unwrap() > partial.unwrap());
    }

    #[test]
    fn substring_match() {
        assert!(fuzzy_score("my_module.rs", "module").is_some());
    }

    #[test]
    fn no_match_returns_none() {
        assert!(fuzzy_score("lib.rs", "xyz").is_none());
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("Cargo.toml", "cargo").is_some());
    }

    #[test]
    fn subsequence_match() {
        // "lr" matches "lib.rs" (l...r)
        assert!(fuzzy_score("lib.rs", "lr").is_some());
    }

    #[test]
    fn filter_entries_by_query() {
        let names = vec!["lib.rs", "main.rs", "Cargo.toml", "README.md"];
        let matches = filter_by_query(&names, "rs");
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&0)); // lib.rs
        assert!(matches.contains(&1)); // main.rs
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib search::tests`
Expected: compilation error.

- [ ] **Step 3: Implement fuzzy search**

Write `src/search.rs`:

```rust
/// Returns a score for how well `name` matches `query`.
/// Higher is better. None means no match.
pub fn fuzzy_score(name: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();

    // Exact match
    if name_lower == query_lower {
        return Some(1000);
    }

    // Prefix match
    if name_lower.starts_with(&query_lower) {
        return Some(500 + query.len() as i32);
    }

    // Substring match
    if name_lower.contains(&query_lower) {
        return Some(200 + query.len() as i32);
    }

    // Subsequence match
    let mut query_chars = query_lower.chars().peekable();
    let mut score = 0i32;

    for ch in name_lower.chars() {
        if let Some(&qch) = query_chars.peek() {
            if ch == qch {
                score += 10;
                query_chars.next();
            }
        }
    }

    if query_chars.peek().is_none() {
        Some(score)
    } else {
        None
    }
}

/// Returns indices of entries whose names match the query, sorted by score (best first).
pub fn filter_by_query(names: &[&str], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, i32)> = names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| fuzzy_score(name, query).map(|s| (i, s)))
        .collect();

    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_scores_highest() {
        let score = fuzzy_score("lib.rs", "lib.rs");
        assert!(score.is_some());
        let partial = fuzzy_score("lib.rs", "lib");
        assert!(score.unwrap() > partial.unwrap());
    }

    #[test]
    fn substring_match() {
        assert!(fuzzy_score("my_module.rs", "module").is_some());
    }

    #[test]
    fn no_match_returns_none() {
        assert!(fuzzy_score("lib.rs", "xyz").is_none());
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("Cargo.toml", "cargo").is_some());
    }

    #[test]
    fn subsequence_match() {
        assert!(fuzzy_score("lib.rs", "lr").is_some());
    }

    #[test]
    fn filter_entries_by_query() {
        let names = vec!["lib.rs", "main.rs", "Cargo.toml", "README.md"];
        let matches = filter_by_query(&names, "rs");
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&0));
        assert!(matches.contains(&1));
    }
}
```

- [ ] **Step 4: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
mod search;
```

(search is internal — not pub-exported)

- [ ] **Step 5: Run tests**

Run: `cargo test --lib search::tests`
Expected: all 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/search.rs src/lib.rs
git commit -m "feat: add fuzzy search with substring and subsequence matching"
```

---

### Task 4: View State Types

**Files:**
- Create: `src/view/mod.rs`
- Create: `src/view/list.rs`
- Create: `src/view/tree.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write tests for view state types**

Write at the bottom of `src/view/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib view::tests`
Expected: compilation error.

- [ ] **Step 3: Implement view state types**

Create `src/view/mod.rs`:

```rust
pub mod list;
pub mod tree;

use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ListViewState {
    pub cursor: usize,
    pub scroll_offset: usize,
}

impl ListViewState {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll_offset: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeViewState {
    pub cursor: usize,
    pub scroll_offset: usize,
    pub expanded: HashSet<PathBuf>,
}

impl TreeViewState {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll_offset: 0,
            expanded: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ViewState {
    List(ListViewState),
    Tree(TreeViewState),
}

impl ViewState {
    pub fn toggle(self) -> Self {
        match self {
            ViewState::List(_) => ViewState::Tree(TreeViewState::new()),
            ViewState::Tree(_) => ViewState::List(ListViewState::new()),
        }
    }

    pub fn cursor(&self) -> usize {
        match self {
            ViewState::List(s) => s.cursor,
            ViewState::Tree(s) => s.cursor,
        }
    }

    pub fn cursor_mut(&mut self) -> &mut usize {
        match self {
            ViewState::List(s) => &mut s.cursor,
            ViewState::Tree(s) => &mut s.cursor,
        }
    }

    pub fn scroll_offset(&self) -> usize {
        match self {
            ViewState::List(s) => s.scroll_offset,
            ViewState::Tree(s) => s.scroll_offset,
        }
    }

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
```

Create `src/view/list.rs` (placeholder for rendering, implemented in Task 8):

```rust
// List view rendering — implemented in Task 8
```

Create `src/view/tree.rs` (placeholder for rendering, implemented in Task 9):

```rust
// Tree view rendering — implemented in Task 9
```

- [ ] **Step 4: Add module to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod view;

pub use view::ViewState;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib view::tests`
Expected: all 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/view/ src/lib.rs
git commit -m "feat: add ViewState enum with List and Tree variants"
```

---

### Task 5: State & Builder

**Files:**
- Create: `src/state.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write tests for state and builder**

Write at the bottom of `src/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn builder_defaults() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.common.current_dir, tmp.path());
        assert!(!state.common.entries.is_empty());
        assert!(matches!(state.view, ViewState::List(_)));
        assert_eq!(state.result(), PickerResult::Pending);
    }

    #[test]
    fn builder_with_tree_view() {
        let tmp = TempDir::new().unwrap();
        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .view(ViewMode::Tree)
            .build();

        assert!(matches!(state.view, ViewState::Tree(_)));
    }

    #[test]
    fn builder_with_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("keep.rs"), "").unwrap();
        fs::write(tmp.path().join("skip.txt"), "").unwrap();

        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .filter(|p: &Path| {
                p.extension().map(|e| e == "rs").unwrap_or(true)
            })
            .build();

        assert_eq!(state.common.entries.len(), 1);
        assert_eq!(state.common.entries[0].name, "keep.rs");
    }

    #[test]
    fn picker_mode_files_only() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("dir")).unwrap();

        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .mode(PickerMode::FilesOnly)
            .build();

        // Directories still shown for navigation, but mode restricts selection
        assert_eq!(state.common.mode, PickerMode::FilesOnly);
    }

    #[test]
    fn toggle_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.common.entries.len(), 1);

        state.toggle_hidden();
        assert_eq!(state.common.entries.len(), 2);

        state.toggle_hidden();
        assert_eq!(state.common.entries.len(), 1);
    }

    #[test]
    fn multi_select_toggle() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.toggle_select();
        assert_eq!(state.common.selected.len(), 1);

        state.toggle_select();
        assert_eq!(state.common.selected.len(), 0);
    }

    #[test]
    fn confirm_returns_cursor_when_no_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.confirm();
        match state.result() {
            PickerResult::Selected(paths) => {
                assert_eq!(paths.len(), 1);
                assert_eq!(paths[0].file_name().unwrap(), "file.txt");
            }
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn confirm_returns_selected_set() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        // Select first item
        state.toggle_select();
        // Move to second
        state.move_cursor_down();
        // Select second
        state.toggle_select();

        state.confirm();
        match state.result() {
            PickerResult::Selected(paths) => assert_eq!(paths.len(), 2),
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn cancel_returns_cancelled() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.cancel();
        assert_eq!(state.result(), PickerResult::Cancelled);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib state::tests`
Expected: compilation error.

- [ ] **Step 3: Implement state, builder, and core methods**

Write `src/state.rs`:

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::entry::{read_entries, Entry, EntryKind};
use crate::theme::FilePickerTheme;
use crate::view::{ListViewState, TreeViewState, ViewState};

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

pub struct CommonState {
    pub current_dir: PathBuf,
    pub entries: Vec<Entry>,
    pub filtered_indices: Option<Vec<usize>>,
    pub selected: HashSet<PathBuf>,
    pub show_hidden: bool,
    pub mode: PickerMode,
    pub input_mode: InputMode,
    pub search_query: String,
    pub visited_dirs: HashSet<PathBuf>,
    pub pending_key: Option<(char, Instant)>,
    pub error_message: Option<String>,
    pub result: PickerResult,
    pub filter: Option<Box<dyn Fn(&Path) -> bool>>,
    pub theme: FilePickerTheme,
}

pub struct FilePickerState {
    pub common: CommonState,
    pub view: ViewState,
}

impl FilePickerState {
    pub fn builder() -> FilePickerBuilder {
        FilePickerBuilder::default()
    }

    pub fn result(&self) -> PickerResult {
        self.common.result.clone()
    }

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

    pub fn refresh_entries(&mut self) {
        self.common.entries = read_entries(
            &self.common.current_dir,
            self.common.show_hidden,
            self.common.filter.as_deref(),
        );
        self.common.filtered_indices = None;
        self.clamp_cursor();
    }

    pub fn toggle_hidden(&mut self) {
        self.common.show_hidden = !self.common.show_hidden;
        self.refresh_entries();
    }

    pub fn toggle_select(&mut self) {
        if let Some(entry) = self.current_entry() {
            let path = entry.path.clone();
            let can_select = match self.common.mode {
                PickerMode::FilesOnly => entry.kind != EntryKind::Directory,
                PickerMode::DirsOnly => entry.kind == EntryKind::Directory,
                PickerMode::Both => true,
            };
            if can_select {
                if !self.common.selected.remove(&path) {
                    self.common.selected.insert(path);
                }
            }
        }
    }

    pub fn confirm(&mut self) {
        if !self.common.selected.is_empty() {
            let paths: Vec<PathBuf> = self.common.selected.iter().cloned().collect();
            self.common.result = PickerResult::Selected(paths);
        } else if let Some(entry) = self.current_entry() {
            if entry.kind == EntryKind::Directory {
                self.enter_directory();
                return;
            }
            self.common.result =
                PickerResult::Selected(vec![entry.path.clone()]);
        }
    }

    pub fn cancel(&mut self) {
        self.common.result = PickerResult::Cancelled;
    }

    pub fn enter_directory(&mut self) {
        if let Some(entry) = self.current_entry() {
            if entry.kind == EntryKind::Directory
                || entry.kind == EntryKind::Symlink
            {
                let target = if entry.kind == EntryKind::Symlink {
                    match std::fs::canonicalize(&entry.path) {
                        Ok(p) if p.is_dir() => p,
                        _ => return,
                    }
                } else {
                    entry.path.clone()
                };

                // Circular symlink detection
                let canonical = std::fs::canonicalize(&target).unwrap_or(target.clone());
                if self.common.visited_dirs.contains(&canonical) {
                    self.common.error_message =
                        Some("Circular symlink".to_string());
                    return;
                }

                self.common.visited_dirs.insert(canonical);
                self.common.current_dir = target;
                self.common.error_message = None;
                *self.view.cursor_mut() = 0;
                *self.view.scroll_offset_mut() = 0;
                self.refresh_entries();
            }
        }
    }

    pub fn go_parent(&mut self) {
        if let Some(parent) = self.common.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.common.current_dir = parent;
            self.common.error_message = None;
            *self.view.cursor_mut() = 0;
            *self.view.scroll_offset_mut() = 0;
            self.refresh_entries();
        }
    }

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

    pub fn toggle_view(&mut self) {
        self.view = self.view.clone().toggle();
        self.clamp_cursor();
    }

    fn clamp_cursor(&mut self) {
        let count = self.visible_count();
        if count == 0 {
            *self.view.cursor_mut() = 0;
        } else {
            let cursor = self.view.cursor_mut();
            if *cursor >= count {
                *cursor = count - 1;
            }
        }
    }

    pub fn go_home(&mut self) {
        if let Some(home) = dirs::home_dir() {
            self.common.current_dir = home;
            self.common.error_message = None;
            *self.view.cursor_mut() = 0;
            *self.view.scroll_offset_mut() = 0;
            self.refresh_entries();
        }
    }
}

pub struct FilePickerBuilder {
    start_dir: Option<PathBuf>,
    mode: PickerMode,
    view_mode: ViewMode,
    filter: Option<Box<dyn Fn(&Path) -> bool>>,
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

    pub fn view(mut self, view: ViewMode) -> Self {
        self.view_mode = view;
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

        let entries = read_entries(
            &start_dir,
            self.show_hidden,
            self.filter.as_deref(),
        );

        let view = match self.view_mode {
            ViewMode::List => ViewState::List(ListViewState::new()),
            ViewMode::Tree => ViewState::Tree(TreeViewState::new()),
        };

        FilePickerState {
            common: CommonState {
                current_dir: start_dir,
                entries,
                filtered_indices: None,
                selected: HashSet::new(),
                show_hidden: self.show_hidden,
                mode: self.mode,
                input_mode: InputMode::Normal,
                search_query: String::new(),
                visited_dirs: HashSet::new(),
                pending_key: None,
                error_message: None,
                result: PickerResult::Pending,
                filter: self.filter,
                theme: self.theme,
            },
            view,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn builder_defaults() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.common.current_dir, tmp.path());
        assert!(!state.common.entries.is_empty());
        assert!(matches!(state.view, ViewState::List(_)));
        assert_eq!(state.result(), PickerResult::Pending);
    }

    #[test]
    fn builder_with_tree_view() {
        let tmp = TempDir::new().unwrap();
        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .view(ViewMode::Tree)
            .build();

        assert!(matches!(state.view, ViewState::Tree(_)));
    }

    #[test]
    fn builder_with_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("keep.rs"), "").unwrap();
        fs::write(tmp.path().join("skip.txt"), "").unwrap();

        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .filter(|p: &Path| {
                p.extension().map(|e| e == "rs").unwrap_or(true)
            })
            .build();

        assert_eq!(state.common.entries.len(), 1);
        assert_eq!(state.common.entries[0].name, "keep.rs");
    }

    #[test]
    fn picker_mode_files_only() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("dir")).unwrap();

        let state = FilePickerState::builder()
            .start_dir(tmp.path())
            .mode(PickerMode::FilesOnly)
            .build();

        assert_eq!(state.common.mode, PickerMode::FilesOnly);
    }

    #[test]
    fn toggle_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.common.entries.len(), 1);

        state.toggle_hidden();
        assert_eq!(state.common.entries.len(), 2);

        state.toggle_hidden();
        assert_eq!(state.common.entries.len(), 1);
    }

    #[test]
    fn multi_select_toggle() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.toggle_select();
        assert_eq!(state.common.selected.len(), 1);

        state.toggle_select();
        assert_eq!(state.common.selected.len(), 0);
    }

    #[test]
    fn confirm_returns_cursor_when_no_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.confirm();
        match state.result() {
            PickerResult::Selected(paths) => {
                assert_eq!(paths.len(), 1);
                assert_eq!(paths[0].file_name().unwrap(), "file.txt");
            }
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn confirm_returns_selected_set() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.toggle_select();
        state.move_cursor_down();
        state.toggle_select();

        state.confirm();
        match state.result() {
            PickerResult::Selected(paths) => assert_eq!(paths.len(), 2),
            _ => panic!("expected Selected"),
        }
    }

    #[test]
    fn cancel_returns_cancelled() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.cancel();
        assert_eq!(state.result(), PickerResult::Cancelled);
    }
}
```

- [ ] **Step 4: Update lib.rs**

Replace `src/lib.rs` with:

```rust
mod entry;
mod search;
mod state;
pub mod view;
mod theme;

pub use entry::{Entry, EntryKind};
pub use state::{
    FilePickerBuilder, FilePickerState, InputMode, PickerMode, PickerResult,
    ViewMode,
};
pub use theme::FilePickerTheme;
pub use view::ViewState;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib state::tests`
Expected: all 8 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/state.rs src/lib.rs
git commit -m "feat: add FilePickerState with builder, navigation, and multi-select"
```

---

### Task 6: Event Handling

**Files:**
- Create: `src/event.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write tests for event handling**

Write at the bottom of `src/event.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::fs;
    use tempfile::TempDir;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn key_ctrl(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    fn key_char(c: char) -> Event {
        key(KeyCode::Char(c))
    }

    #[test]
    fn j_moves_down() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.view.cursor(), 0);
        handle_event(&mut state, key_char('j'));
        assert_eq!(state.view.cursor(), 1);
    }

    #[test]
    fn k_moves_up() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('j'));
        handle_event(&mut state, key_char('k'));
        assert_eq!(state.view.cursor(), 0);
    }

    #[test]
    fn arrow_keys_navigate() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key(KeyCode::Down));
        assert_eq!(state.view.cursor(), 1);
        handle_event(&mut state, key(KeyCode::Up));
        assert_eq!(state.view.cursor(), 0);
    }

    #[test]
    fn shift_g_moves_to_bottom() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        fs::write(tmp.path().join("c.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
        );
        assert_eq!(state.view.cursor(), 2);
    }

    #[test]
    fn space_toggles_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char(' '));
        assert_eq!(state.common.selected.len(), 1);
        handle_event(&mut state, key_char(' '));
        assert_eq!(state.common.selected.len(), 0);
    }

    #[test]
    fn dot_toggles_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.common.entries.len(), 1);
        handle_event(&mut state, key_char('.'));
        assert_eq!(state.common.entries.len(), 2);
    }

    #[test]
    fn esc_cancels() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key(KeyCode::Esc));
        assert_eq!(state.result(), PickerResult::Cancelled);
    }

    #[test]
    fn q_cancels() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('q'));
        assert_eq!(state.result(), PickerResult::Cancelled);
    }

    #[test]
    fn tab_toggles_view() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert!(matches!(state.view, ViewState::List(_)));
        handle_event(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.view, ViewState::Tree(_)));
        handle_event(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.view, ViewState::List(_)));
    }

    #[test]
    fn slash_enters_search_mode() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('/'));
        assert_eq!(state.common.input_mode, InputMode::Search);
    }

    #[test]
    fn search_mode_typing_and_esc_clears() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("alpha.rs"), "").unwrap();
        fs::write(tmp.path().join("beta.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('/'));
        handle_event(&mut state, key_char('a'));
        assert_eq!(state.common.search_query, "a");
        assert!(state.common.filtered_indices.is_some());

        handle_event(&mut state, key(KeyCode::Esc));
        assert_eq!(state.common.input_mode, InputMode::Normal);
        assert!(state.common.search_query.is_empty());
        assert!(state.common.filtered_indices.is_none());
    }

    #[test]
    fn search_mode_enter_keeps_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("alpha.rs"), "").unwrap();
        fs::write(tmp.path().join("beta.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('/'));
        handle_event(&mut state, key_char('a'));
        handle_event(&mut state, key(KeyCode::Enter));
        assert_eq!(state.common.input_mode, InputMode::Normal);
        assert!(state.common.filtered_indices.is_some());
    }

    #[test]
    fn gg_sequence_moves_to_top() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        fs::write(tmp.path().join("c.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        // Move to bottom first
        handle_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
        );
        assert_eq!(state.view.cursor(), 2);

        // gg to top
        handle_event(&mut state, key_char('g'));
        handle_event(&mut state, key_char('g'));
        assert_eq!(state.view.cursor(), 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib event::tests`
Expected: compilation error.

- [ ] **Step 3: Implement event handling**

Write `src/event.rs`:

```rust
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};

use crate::search::fuzzy_score;
use crate::state::{FilePickerState, InputMode, PickerResult, ViewState};

const GG_TIMEOUT: Duration = Duration::from_millis(500);
const DEFAULT_PAGE_HEIGHT: usize = 20;

pub fn handle_event(state: &mut FilePickerState, event: Event) {
    // Don't process events if already resolved
    if state.result() != PickerResult::Pending {
        return;
    }

    match event {
        Event::Key(key) => handle_key(state, key),
        Event::Mouse(mouse) => handle_mouse(state, mouse),
        _ => {}
    }
}

fn handle_key(state: &mut FilePickerState, key: KeyEvent) {
    match state.common.input_mode {
        InputMode::Normal => handle_normal_key(state, key),
        InputMode::Search => handle_search_key(state, key),
    }
}

fn handle_normal_key(state: &mut FilePickerState, key: KeyEvent) {
    // Check pending gg sequence
    if let Some((pending_char, timestamp)) = state.common.pending_key.take() {
        if timestamp.elapsed() < GG_TIMEOUT {
            if pending_char == 'g'
                && key.code == KeyCode::Char('g')
                && key.modifiers == KeyModifiers::NONE
            {
                state.move_to_top();
                return;
            }
        }
        // Timeout or non-matching key — fall through to normal handling
    }

    match key.code {
        // Navigation
        KeyCode::Char('j') | KeyCode::Down => state.move_cursor_down(),
        KeyCode::Char('k') | KeyCode::Up => state.move_cursor_up(),
        KeyCode::Char('l') | KeyCode::Right => state.enter_directory(),
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
            state.go_parent()
        }
        KeyCode::Char('G') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            state.move_to_bottom()
        }
        KeyCode::Char('g') => {
            state.common.pending_key = Some(('g', Instant::now()));
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_half_page_down(DEFAULT_PAGE_HEIGHT)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_half_page_up(DEFAULT_PAGE_HEIGHT)
        }

        // Actions
        KeyCode::Char(' ') => state.toggle_select(),
        KeyCode::Enter => state.confirm(),
        KeyCode::Esc | KeyCode::Char('q') => state.cancel(),
        KeyCode::Tab => state.toggle_view(),
        KeyCode::Char('.') => state.toggle_hidden(),
        KeyCode::Char('/') => {
            state.common.input_mode = InputMode::Search;
            state.common.search_query.clear();
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.common.input_mode = InputMode::Search;
            state.common.search_query.clear();
        }
        KeyCode::Char('~') => state.go_home(),

        _ => {}
    }
}

fn handle_search_key(state: &mut FilePickerState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            state.common.input_mode = InputMode::Normal;
            state.common.search_query.clear();
            state.common.filtered_indices = None;
            state.clamp_cursor_pub();
        }
        KeyCode::Enter => {
            state.common.input_mode = InputMode::Normal;
            // Keep filter active
        }
        KeyCode::Backspace => {
            state.common.search_query.pop();
            update_search_filter(state);
        }
        KeyCode::Char('j') | KeyCode::Down => state.move_cursor_down(),
        KeyCode::Char('k') | KeyCode::Up => state.move_cursor_up(),
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            state.common.search_query.push(c);
            update_search_filter(state);
        }
        _ => {}
    }
}

fn update_search_filter(state: &mut FilePickerState) {
    if state.common.search_query.is_empty() {
        state.common.filtered_indices = None;
    } else {
        let mut scored: Vec<(usize, i32)> = state
            .common
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                fuzzy_score(&entry.name, &state.common.search_query)
                    .map(|s| (i, s))
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        state.common.filtered_indices =
            Some(scored.into_iter().map(|(i, _)| i).collect());
    }
    *state.view.cursor_mut() = 0;
    *state.view.scroll_offset_mut() = 0;
}

fn handle_mouse(state: &mut FilePickerState, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            let clicked_row = mouse.row as usize;
            // Offset by 2 for path bar + border
            if clicked_row >= 2 {
                let entry_idx = (clicked_row - 2) + state.view.scroll_offset();
                if entry_idx < state.visible_count() {
                    *state.view.cursor_mut() = entry_idx;
                }
            }
        }
        MouseEventKind::ScrollDown => state.move_cursor_down(),
        MouseEventKind::ScrollUp => state.move_cursor_up(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::fs;
    use tempfile::TempDir;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn key_ctrl(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    fn key_char(c: char) -> Event {
        key(KeyCode::Char(c))
    }

    #[test]
    fn j_moves_down() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.view.cursor(), 0);
        handle_event(&mut state, key_char('j'));
        assert_eq!(state.view.cursor(), 1);
    }

    #[test]
    fn k_moves_up() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('j'));
        handle_event(&mut state, key_char('k'));
        assert_eq!(state.view.cursor(), 0);
    }

    #[test]
    fn arrow_keys_navigate() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key(KeyCode::Down));
        assert_eq!(state.view.cursor(), 1);
        handle_event(&mut state, key(KeyCode::Up));
        assert_eq!(state.view.cursor(), 0);
    }

    #[test]
    fn shift_g_moves_to_bottom() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        fs::write(tmp.path().join("c.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
        );
        assert_eq!(state.view.cursor(), 2);
    }

    #[test]
    fn space_toggles_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char(' '));
        assert_eq!(state.common.selected.len(), 1);
        handle_event(&mut state, key_char(' '));
        assert_eq!(state.common.selected.len(), 0);
    }

    #[test]
    fn dot_toggles_hidden() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("visible.txt"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert_eq!(state.common.entries.len(), 1);
        handle_event(&mut state, key_char('.'));
        assert_eq!(state.common.entries.len(), 2);
    }

    #[test]
    fn esc_cancels() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key(KeyCode::Esc));
        assert_eq!(state.result(), PickerResult::Cancelled);
    }

    #[test]
    fn q_cancels() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('q'));
        assert_eq!(state.result(), PickerResult::Cancelled);
    }

    #[test]
    fn tab_toggles_view() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        assert!(matches!(state.view, ViewState::List(_)));
        handle_event(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.view, ViewState::Tree(_)));
        handle_event(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.view, ViewState::List(_)));
    }

    #[test]
    fn slash_enters_search_mode() {
        let tmp = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('/'));
        assert_eq!(state.common.input_mode, InputMode::Search);
    }

    #[test]
    fn search_mode_typing_and_esc_clears() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("alpha.rs"), "").unwrap();
        fs::write(tmp.path().join("beta.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('/'));
        handle_event(&mut state, key_char('a'));
        assert_eq!(state.common.search_query, "a");
        assert!(state.common.filtered_indices.is_some());

        handle_event(&mut state, key(KeyCode::Esc));
        assert_eq!(state.common.input_mode, InputMode::Normal);
        assert!(state.common.search_query.is_empty());
        assert!(state.common.filtered_indices.is_none());
    }

    #[test]
    fn search_mode_enter_keeps_filter() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("alpha.rs"), "").unwrap();
        fs::write(tmp.path().join("beta.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(&mut state, key_char('/'));
        handle_event(&mut state, key_char('a'));
        handle_event(&mut state, key(KeyCode::Enter));
        assert_eq!(state.common.input_mode, InputMode::Normal);
        assert!(state.common.filtered_indices.is_some());
    }

    #[test]
    fn gg_sequence_moves_to_top() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        fs::write(tmp.path().join("c.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        handle_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT)),
        );
        assert_eq!(state.view.cursor(), 2);

        handle_event(&mut state, key_char('g'));
        handle_event(&mut state, key_char('g'));
        assert_eq!(state.view.cursor(), 0);
    }
}
```

Note: The event handler references `state.clamp_cursor_pub()` — add this public wrapper to `src/state.rs`:

```rust
// Add to FilePickerState impl block
pub fn clamp_cursor_pub(&mut self) {
    self.clamp_cursor();
}
```

- [ ] **Step 4: Add module to lib.rs and re-export handle_event on FilePickerState**

Add to `src/lib.rs`:

```rust
mod event;
```

Add a convenience method to `FilePickerState` in `src/state.rs`:

```rust
// Add to FilePickerState impl block
pub fn handle_event(&mut self, event: crossterm::event::Event) {
    crate::event::handle_event(self, event);
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib event::tests`
Expected: all 13 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/event.rs src/state.rs src/lib.rs
git commit -m "feat: add event handling with vim+arrow keys, search mode, gg sequence, and mouse support"
```

---

### Task 7: Widget Rendering — Path Bar & Status Bar

**Files:**
- Create: `src/widget.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write test for widget rendering**

Write at the bottom of `src/widget.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn renders_without_panic() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                f.render_stateful_widget(
                    FilePicker::default(),
                    f.area(),
                    &mut state,
                );
            })
            .unwrap();
    }

    #[test]
    fn renders_empty_directory() {
        let tmp = TempDir::new().unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                f.render_stateful_widget(
                    FilePicker::default(),
                    f.area(),
                    &mut state,
                );
            })
            .unwrap();
    }

    #[test]
    fn renders_with_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.toggle_select();

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                f.render_stateful_widget(
                    FilePicker::default(),
                    f.area(),
                    &mut state,
                );
            })
            .unwrap();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib widget::tests`
Expected: compilation error.

- [ ] **Step 3: Implement FilePicker widget**

Write `src/widget.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, StatefulWidget, Widget};

use crate::entry::EntryKind;
use crate::state::{FilePickerState, InputMode};
use crate::view::ViewState;

pub struct FilePicker {
    block: Option<Block<'static>>,
}

impl Default for FilePicker {
    fn default() -> Self {
        Self { block: None }
    }
}

impl FilePicker {
    pub fn block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
    }
}

impl StatefulWidget for FilePicker {
    type State = FilePickerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let block = self
            .block
            .unwrap_or_else(|| Block::default().borders(Borders::ALL));
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // path bar
                Constraint::Min(1),   // file list
                Constraint::Length(1), // status bar
            ])
            .split(inner);

        render_path_bar(state, chunks[0], buf);
        render_file_list(state, chunks[1], buf);
        render_status_bar(state, chunks[2], buf);
    }
}

fn render_path_bar(state: &FilePickerState, area: Rect, buf: &mut Buffer) {
    let path_str = state.common.current_dir.display().to_string();
    let style = state.common.theme.path_bar;
    let line = Line::from(vec![Span::styled(format!(" {}", path_str), style)]);
    Paragraph::new(line).render(area, buf);
}

fn render_file_list(state: &mut FilePickerState, area: Rect, buf: &mut Buffer) {
    let visible = state.visible_entries();
    let height = area.height as usize;

    if visible.is_empty() {
        let empty = Line::from(Span::styled(
            "  (empty)",
            state.common.theme.status_bar,
        ));
        Paragraph::new(empty).render(area, buf);
        return;
    }

    // Update scroll offset
    let cursor = state.view.cursor();
    let scroll = state.view.scroll_offset();
    let new_scroll = if cursor < scroll {
        cursor
    } else if cursor >= scroll + height {
        cursor - height + 1
    } else {
        scroll
    };
    *state.view.scroll_offset_mut() = new_scroll;

    let entries = state.visible_entries();
    let theme = &state.common.theme;

    for (i, entry) in entries
        .iter()
        .skip(new_scroll)
        .take(height)
        .enumerate()
    {
        let visible_idx = new_scroll + i;
        let is_cursor = visible_idx == cursor;
        let is_selected = state.common.selected.contains(&entry.path);

        let prefix = if is_selected { " * " } else { "   " };

        let name_style = match entry.kind {
            EntryKind::Directory => theme.directory,
            EntryKind::Symlink => theme.symlink,
            EntryKind::File => theme.normal,
        };

        let suffix = match entry.kind {
            EntryKind::Directory => "/",
            EntryKind::Symlink => " ->",
            EntryKind::File => "",
        };

        let mut line_style = name_style;
        if is_cursor {
            line_style = line_style.patch(theme.cursor);
        }
        if is_selected {
            line_style = line_style.patch(theme.selected);
        }

        let text = format!("{}{}{}", prefix, entry.name, suffix);
        let y = area.y + i as u16;
        if y < area.y + area.height {
            buf.set_string(area.x, y, &text, line_style);
            // Fill remaining width
            let remaining = area.width.saturating_sub(text.len() as u16);
            if remaining > 0 && is_cursor {
                buf.set_string(
                    area.x + text.len() as u16,
                    y,
                    " ".repeat(remaining as usize),
                    theme.cursor,
                );
            }
        }
    }
}

fn render_status_bar(state: &FilePickerState, area: Rect, buf: &mut Buffer) {
    let theme = &state.common.theme;

    let status = match state.common.input_mode {
        InputMode::Search => {
            let query = &state.common.search_query;
            let count = state.visible_count();
            format!(" / {}  ({} matches)", query, count)
        }
        InputMode::Normal => {
            let mut parts = Vec::new();

            let selected_count = state.common.selected.len();
            if selected_count > 0 {
                parts.push(format!("{} selected", selected_count));
            }

            parts.push(if state.common.show_hidden {
                "hidden: on".to_string()
            } else {
                "hidden: off".to_string()
            });

            let view_name = match state.view {
                ViewState::List(_) => "list",
                ViewState::Tree(_) => "tree",
            };
            parts.push(format!("view: {}", view_name));

            if let Some(err) = &state.common.error_message {
                parts.push(err.clone());
            }

            format!(" {}", parts.join(" | "))
        }
    };

    let style = if state.common.error_message.is_some() {
        theme.error
    } else if state.common.input_mode == InputMode::Search {
        theme.search_input
    } else {
        theme.status_bar
    };

    let line = Line::from(Span::styled(status, style));
    Paragraph::new(line).render(area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn renders_without_panic() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("subdir")).unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                f.render_stateful_widget(
                    FilePicker::default(),
                    f.area(),
                    &mut state,
                );
            })
            .unwrap();
    }

    #[test]
    fn renders_empty_directory() {
        let tmp = TempDir::new().unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                f.render_stateful_widget(
                    FilePicker::default(),
                    f.area(),
                    &mut state,
                );
            })
            .unwrap();
    }

    #[test]
    fn renders_with_selection() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();

        let mut state = FilePickerState::builder()
            .start_dir(tmp.path())
            .build();

        state.toggle_select();

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                f.render_stateful_widget(
                    FilePicker::default(),
                    f.area(),
                    &mut state,
                );
            })
            .unwrap();
    }
}
```

- [ ] **Step 4: Add module and re-export**

Add to `src/lib.rs`:

```rust
mod widget;

pub use widget::FilePicker;
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib widget::tests`
Expected: all 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/widget.rs src/lib.rs
git commit -m "feat: add FilePicker StatefulWidget with path bar, file list, and status bar rendering"
```

---

### Task 8: List View Rendering

**Files:**
- Modify: `src/view/list.rs`

- [ ] **Step 1: Implement list view rendering**

The core list rendering is already in `widget.rs` `render_file_list`. The `list.rs` module provides list-specific layout helpers if needed. For now, the list view uses the default rendering from `widget.rs` directly.

Write `src/view/list.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::ListViewState;

impl ListViewState {
    /// Returns the visible range of entries for the current scroll state.
    pub fn visible_range(&self, total: usize, height: usize) -> std::ops::Range<usize> {
        let start = self.scroll_offset;
        let end = (start + height).min(total);
        start..end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_basic() {
        let state = ListViewState {
            cursor: 0,
            scroll_offset: 0,
        };
        let range = state.visible_range(20, 10);
        assert_eq!(range, 0..10);
    }

    #[test]
    fn visible_range_scrolled() {
        let state = ListViewState {
            cursor: 15,
            scroll_offset: 10,
        };
        let range = state.visible_range(20, 10);
        assert_eq!(range, 10..20);
    }

    #[test]
    fn visible_range_at_end() {
        let state = ListViewState {
            cursor: 18,
            scroll_offset: 15,
        };
        let range = state.visible_range(20, 10);
        assert_eq!(range, 15..20);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib view::list::tests`
Expected: all 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/view/list.rs
git commit -m "feat: add list view visible range calculation"
```

---

### Task 9: Tree View Rendering

**Files:**
- Modify: `src/view/tree.rs`

- [ ] **Step 1: Write tests for tree view**

Write `src/view/tree.rs`:

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::entry::{read_entries, Entry, EntryKind};

use super::TreeViewState;

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub entry: Entry,
    pub depth: usize,
}

impl TreeViewState {
    pub fn toggle_expand(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
        }
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    /// Builds a flat list of tree entries by walking expanded directories.
    pub fn build_tree_entries(
        &self,
        root: &Path,
        show_hidden: bool,
        filter: Option<&dyn Fn(&Path) -> bool>,
    ) -> Vec<TreeEntry> {
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
        result: &mut Vec<TreeEntry>,
    ) {
        let entries = read_entries(dir, show_hidden, filter);
        for entry in entries {
            let is_dir = entry.kind == EntryKind::Directory;
            let path = entry.path.clone();
            result.push(TreeEntry {
                entry,
                depth,
            });
            if is_dir && self.is_expanded(&path) {
                self.collect_entries(&path, depth + 1, show_hidden, filter, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

        // Only root level entries — subdir not expanded
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
        let sub_entry = tree.iter().find(|e| e.entry.name == "b.txt").unwrap();
        assert_eq!(sub_entry.depth, 1);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib view::tree::tests`
Expected: all 3 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/view/tree.rs
git commit -m "feat: add tree view with expand/collapse and recursive entry building"
```

---

### Task 10: Basic Example

**Files:**
- Create: `examples/basic.rs`

- [ ] **Step 1: Write the basic example**

Write `examples/basic.rs`:

```rust
use std::io;

use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use ratatree::{FilePicker, FilePickerState, PickerResult};

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create picker state
    let mut state = FilePickerState::builder()
        .start_dir(".")
        .build();

    // Event loop
    loop {
        terminal.draw(|f| {
            f.render_stateful_widget(
                FilePicker::default(),
                f.area(),
                &mut state,
            );
        })?;

        if let Event::Key(key) = event::read()? {
            state.handle_event(Event::Key(key));
        }

        match state.result() {
            PickerResult::Selected(paths) => {
                // Restore terminal
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                terminal.show_cursor()?;

                println!("Selected:");
                for path in paths {
                    println!("  {}", path.display());
                }
                return Ok(());
            }
            PickerResult::Cancelled => {
                disable_raw_mode()?;
                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                terminal.show_cursor()?;

                println!("Cancelled");
                return Ok(());
            }
            PickerResult::Pending => {}
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --example basic`
Expected: compiles successfully.

- [ ] **Step 3: Run it manually to sanity check**

Run: `cargo run --example basic`
Expected: interactive file picker appears. Navigate with j/k, Enter to select, q to quit.

- [ ] **Step 4: Commit**

```bash
git add examples/basic.rs
git commit -m "feat: add basic example with full event loop"
```

---

### Task 11: Integration Tests

**Files:**
- Create: `tests/integration.rs`

- [ ] **Step 1: Write integration tests**

Write `tests/integration.rs`:

```rust
use std::fs;
use std::path::PathBuf;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

use ratatree::{FilePickerState, PickerMode, PickerResult, ViewMode};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn key_char(c: char) -> Event {
    key(KeyCode::Char(c))
}

fn setup_test_dir() -> TempDir {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src").join("main.rs"), "fn main() {}").unwrap();
    fs::write(tmp.path().join("src").join("lib.rs"), "").unwrap();
    fs::create_dir(tmp.path().join("tests")).unwrap();
    fs::write(tmp.path().join("tests").join("test.rs"), "").unwrap();
    fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
    fs::write(tmp.path().join("README.md"), "# Hello").unwrap();
    fs::write(tmp.path().join(".gitignore"), "/target").unwrap();
    tmp
}

#[test]
fn navigate_and_select_single_file() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    // Navigate to a file and confirm
    // Entries are sorted: dirs first (src, tests), then files (Cargo.toml, README.md)
    state.handle_event(key(KeyCode::Down)); // tests/
    state.handle_event(key(KeyCode::Down)); // Cargo.toml
    state.handle_event(key(KeyCode::Enter)); // confirm

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].file_name().unwrap(), "Cargo.toml");
        }
        _ => panic!("expected Selected"),
    }
}

#[test]
fn multi_select_across_navigation() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    // Select first two items
    state.handle_event(key(KeyCode::Down)); // tests/
    state.handle_event(key(KeyCode::Down)); // Cargo.toml
    state.handle_event(key_char(' '));      // toggle select
    state.handle_event(key(KeyCode::Down)); // README.md
    state.handle_event(key_char(' '));      // toggle select
    state.handle_event(key(KeyCode::Enter)); // confirm

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 2);
        }
        _ => panic!("expected Selected"),
    }
}

#[test]
fn directory_navigation() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    // Enter src/ directory (first entry)
    state.handle_event(key(KeyCode::Enter)); // enter src/

    assert!(state
        .common
        .current_dir
        .ends_with("src"));

    // Go back to parent
    state.handle_event(key_char('h'));
    assert_eq!(state.common.current_dir, tmp.path());
}

#[test]
fn view_toggle_preserves_directory() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    let original_dir = state.common.current_dir.clone();
    let original_entries_count = state.common.entries.len();

    // Toggle to tree view
    state.handle_event(key(KeyCode::Tab));

    assert_eq!(state.common.current_dir, original_dir);
    assert_eq!(state.common.entries.len(), original_entries_count);
}

#[test]
fn hidden_files_toggle() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    let count_without_hidden = state.common.entries.len();

    state.handle_event(key_char('.'));
    let count_with_hidden = state.common.entries.len();

    assert!(count_with_hidden > count_without_hidden);
}

#[test]
fn search_and_confirm() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    // Enter search mode and type "Cargo"
    state.handle_event(key_char('/'));
    state.handle_event(key_char('C'));
    state.handle_event(key_char('a'));
    state.handle_event(key_char('r'));

    assert!(state.common.filtered_indices.is_some());
    let visible = state.visible_count();
    assert_eq!(visible, 1); // Only Cargo.toml matches

    // Confirm from search
    state.handle_event(key(KeyCode::Enter)); // exits search, keeps filter
    state.handle_event(key(KeyCode::Enter)); // confirms selection

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].file_name().unwrap(), "Cargo.toml");
        }
        _ => panic!("expected Selected"),
    }
}

#[test]
fn files_only_mode_blocks_dir_selection() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .mode(PickerMode::FilesOnly)
        .build();

    // First entry is src/ (directory) — try to space-select it
    state.handle_event(key_char(' '));
    assert_eq!(state.common.selected.len(), 0); // Should not select directory
}

#[test]
fn cancel_returns_cancelled() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .build();

    state.handle_event(key(KeyCode::Esc));
    assert_eq!(state.result(), PickerResult::Cancelled);
}

#[cfg(unix)]
#[test]
fn symlink_cycle_detection() {
    let tmp = setup_test_dir();
    let link_path = tmp.path().join("self_link");
    std::os::unix::fs::symlink(tmp.path(), &link_path).unwrap();

    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .show_hidden(true)
        .build();

    // Find and enter the symlink
    let link_idx = state
        .common
        .entries
        .iter()
        .position(|e| e.name == "self_link")
        .unwrap();

    for _ in 0..link_idx {
        state.handle_event(key(KeyCode::Down));
    }

    // Enter the symlink — first time should work (it points to parent)
    state.handle_event(key(KeyCode::Enter));

    // Find and try to enter it again — should be blocked
    let link_idx2 = state
        .common
        .entries
        .iter()
        .position(|e| e.name == "self_link");

    if let Some(idx) = link_idx2 {
        *state.view.cursor_mut() = idx;
        state.handle_event(key(KeyCode::Enter));
        assert!(state.common.error_message.is_some());
    }
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test integration`
Expected: all tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/integration.rs
git commit -m "feat: add integration tests for navigation, selection, search, and symlink detection"
```

---

### Task 12: Final Cleanup & All Tests Green

**Files:**
- Verify all existing files

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests pass (unit + integration).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 3: Fix any clippy warnings if present**

Address each warning individually.

- [ ] **Step 4: Verify example compiles**

Run: `cargo build --example basic`
Expected: compiles successfully.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "chore: clippy fixes and final cleanup"
```
