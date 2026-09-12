# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - unreleased

The three dialogs an application usually needs, open a file, choose a folder and pick several files, now work without help in a narrow panel, a long path, a large directory and an empty folder.

### Migration

- In `PickerMode::DirsOnly` the first entry of `CommonState::entries` is now `.`, whose `path` is `current_dir`. Code that iterates `entries` or indexes `entries[0]` in that mode sees it. `FilesOnly` and `Both` are unchanged.
- `PickerResult::Selected` can now return `current_dir` itself in `DirsOnly` mode.

### Added

- `.` entry in `DirsOnly` mode. A choose-folder dialog had no way to pick the folder the user had navigated into, and an empty folder could not be picked at all. `Enter` or `Space` on `.` picks the current directory; it cannot be entered or expanded.
- Cursor position in the status bar: `3/340 | 0 selected | ...`. A home directory with a few hundred entries gave no sense of where the cursor was.
- `examples/dialogs.rs`: the three dialogs as 50x18 modals over a host screen.

### Changed

- The path bar truncates from the left. A 50-column panel showing a home directory path used to lose the one component that matters, the directory the user is in. It now reads `…/Application Support/app`.

## [0.3.0] - 2026-09-12

Reusable state, a read failure that stays on screen, clean paths on Windows, and two renames that make the next field addition a non-breaking change.

### Migration

- Replace `clamp_cursor_pub()` with `clamp_cursor()`.
- `CommonState` and `FilePickerState` are now `#[non_exhaustive]`. Code that built either with a struct literal must go through `FilePickerState::builder()` instead. Reading and writing the public fields is unchanged.
- `TreeViewState::build_tree_entries` returns `io::Result<Vec<Entry>>`. Only callers of that method directly are affected; `refresh_entries` handles it.
- A directory read failure no longer appears in `CommonState::error_message`. Read it from `CommonState::read_error`.

### Added

- `FilePickerState::reset()` - puts a finished picker back to `PickerResult::Pending` and clears the selection, search, pending key prefix and error message. The current directory, cursor, scroll and tree expansion are kept, so the user resumes where they left off. Previously a state whose result was `Selected` or `Cancelled` ignored every event for good.
- `CommonState::read_error` - why the current directory could not be listed, or `None` after a successful read. Unlike `error_message` it is not cleared by the next keypress; it goes away when a directory read succeeds. The status bar shows it whenever there is no one-off `error_message`, so an empty listing keeps explaining itself. This is the 0.2.1 "Known limitation".
- `FilePickerState` implements `Debug`.

### Changed

- `clamp_cursor_pub` is now `clamp_cursor`. The private method took the public name; there was never a reason for two.
- `CommonState` and `FilePickerState` are `#[non_exhaustive]`.
- `TreeViewState::build_tree_entries` returns `io::Result<Vec<Entry>>`. A failure to read the root is now an error; an unreadable expanded subdirectory is still skipped, as in 0.2.1.

### Fixed

- On Windows, `current_dir`, every entry path and every path returned through `PickerResult::Selected` carried the `\\?\` verbatim prefix that `std::fs::canonicalize` adds. Paths are now canonicalized with `dunce`, which drops the prefix whenever the path is short enough to work without it.
- In tree view, a root directory that could not be read rendered as an empty pane with no message. The 0.2.1 fix only covered list view.

## [0.2.1] - 2026-09-08

### Fixed

- Fuzzy search ranked almost nothing. Every subsequence match scored `10 x query length`, so they all tied, and the `+ query length` added to the prefix and substring tiers was the same for every candidate. Names therefore came back in directory order rather than by how well they matched. Scores are now a tier plus a bonus that rewards a shorter name, an earlier match, a match starting at a word boundary, and matched letters sitting close together. The bonus is capped below the gap between tiers, so a name that literally contains the query always outranks one that merely has the letters in order.
- A directory that could not be listed rendered as an empty pane with no explanation. `read_dir` failures now reach the status bar as `Cannot read directory: <reason>`. In tree view an unreadable subdirectory is skipped instead, leaving the rest of the tree intact, because a large tree often contains several and erroring on each would bury the listing.

### Known limitations

- The read failure message clears on the next keypress, like every other status message. Showing it for as long as the directory stays unreadable needs a new field on `CommonState`, whose fields are all public, so that waits for 0.3.0.

## [0.2.0] - 2026-09-08

Tree view is now actually wired into the widget, nine navigation and rendering bugs are fixed, and the crate builds against ratatui 0.30.

### Added

- `Entry.depth: usize` - nesting depth in tree view. Always `0` in list view.
- `FilePickerState::descend()` / `ascend()` - view-aware enter and leave. In list view these behave exactly like `enter_directory` and `go_parent`.
- `FilePickerState::expand_current()` / `collapse_current()` / `toggle_expand_current()` - expansion control for tree view.
- `FilePickerState::page_height()` - height of the last render, used by `Ctrl+D` / `Ctrl+U`. Returns `20` before the first render.
- `CommonState.list_area: Rect` - the list area recorded at render time, used to map mouse coordinates.
- `ratatree::crossterm` - re-export of the `crossterm` this crate was built against, so `handle_event` always receives a matching `Event` type.
- `ratatree::CommonState` and `ratatree::FilterFn` re-exports. Both were already reachable through `FilePickerState::common` and `FilePickerBuilder::filter`, but could not be named from outside the crate.
- Documentation for every public item, with `#![warn(missing_docs)]` enabled to keep it that way. Previously docs.rs listed names with almost no prose.

### Removed

- `CommonState.visited_dirs` - no longer needed after the circular symlink check changed.
- `view::tree::TreeEntry` - merged into `Entry` now that `Entry` carries `depth`.

### Changed

- `TreeViewState::build_tree_entries()` returns `Vec<Entry>` instead of `Vec<TreeEntry>`.
- In search mode, `j` and `k` type into the query instead of moving the cursor. Navigate results with `Up`/`Down`, `Ctrl+N`/`Ctrl+P`, or `Ctrl+J`/`Ctrl+K`.
- In tree view, `l` / `h` / `Enter` expand and collapse in place instead of changing the root directory. List view behaviour is unchanged.
- `PickerMode::DirsOnly` no longer returns a file when `Enter` is pressed on one.
- `start_dir` expands a leading `~` and canonicalizes the path. A path that does not exist is expanded but otherwise left alone.
- A symlink pointing at the current directory or one of its ancestors is now blocked on the first attempt to enter it. Previously it was only caught on the second attempt.

### Fixed

- Re-entering a directory after going up reported a false "Circular symlink" error and refused to enter. The visited-directory set was pushed to but never popped.
- `start_dir(".")` left `current_dir` empty after pressing `h`, emptying the listing. The shipped example used exactly this combination.
- Search mode could not accept `j` or `k` in the query, so typing "json" produced "son".
- `error_message` was never cleared once set. It is now cleared on each handled key or mouse event.
- Wide characters (CJK, emoji) in file names overlapped the following glyph. Rows are now built as a `Line` and drawn with `buf.set_line`, which accounts for grapheme width.
- `KeyEventKind::Release` events were handled as presses, so every key acted twice on Windows and on terminals using the kitty keyboard protocol.
- Mouse clicks assumed a hardcoded `row - 2` offset, which was off by one without a border and ignored both the scroll offset and the widget's position. Clicks are now mapped through the recorded `list_area`.
- `Ctrl+D` / `Ctrl+U` scrolled a hardcoded 20 rows instead of the actual rendered height.

### Dependencies

- ratatui 0.29 to 0.30, dirs 5 to 7.
- Dropped the direct `crossterm` dependency in favour of ratatui's re-export. crossterm still resolves to 0.29, now transitively.
- Added `rust-version = "1.88"`, required by ratatui 0.30.

## [0.1.0] - 2026-04-10

Initial release.

- `FilePicker` stateful widget with `FilePickerState` and a builder.
- List and tree view modes, vim-style keybindings, fuzzy search, multi-select, hidden file toggle, symlink handling, a filter callback, and a fully customizable `FilePickerTheme`.

[0.2.1]: https://github.com/namil-k/ratatree/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/namil-k/ratatree/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/namil-k/ratatree/releases/tag/v0.1.0
