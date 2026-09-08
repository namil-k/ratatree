# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - unreleased

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

[0.2.0]: https://github.com/namil-k/ratatree/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/namil-k/ratatree/releases/tag/v0.1.0
