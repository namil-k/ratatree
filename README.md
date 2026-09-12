# ratatree

[![CI](https://github.com/namil-k/ratatree/actions/workflows/ci.yml/badge.svg)](https://github.com/namil-k/ratatree/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ratatree.svg)](https://crates.io/crates/ratatree)
[![docs.rs](https://img.shields.io/docsrs/ratatree)](https://docs.rs/ratatree)

A file and directory picker widget for [ratatui](https://github.com/ratatui/ratatui).

Drop it into any ratatui app. Your users get a full-featured file browser with keyboard navigation, fuzzy search, multi-select, and two view modes - all from a single widget.

```
┌─────────────────────────────────────────────┐
│/home/you/projects/ratatree                  │
│   src/                                      │
│   tests/                                    │
│ * Cargo.toml                                │
│   config ->                                 │
│                                             │
│3/4 | 1 selected | hidden: off | view: list  │
└─────────────────────────────────────────────┘
```

The cursor row is drawn with the cursor style (a background highlight), not a marker character. Multi-selected rows are prefixed with `*`, directories get a trailing `/`, and symlinks a trailing `->`. The example above is drawn inside a `Block` you supply with `FilePicker::default().block(...)`; without one the widget uses the whole area and draws no border.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
ratatree = "0.4"
ratatui = "0.30"
```

You do not need a `crossterm` entry. ratatree re-exports the version it was built against as `ratatree::crossterm`, so the events you hand to `handle_event` always match the type it expects.

Three things to know:

1. **`FilePickerState`** holds all the state (current directory, cursor, selections)
2. **`FilePicker`** renders it (implements ratatui's `StatefulWidget`)
3. **`PickerResult`** tells you what the user did (still browsing, picked files, or cancelled)

```rust
use ratatree::crossterm::event::{self, Event};
use ratatree::{FilePicker, FilePickerState, PickerMode, PickerResult};

// Create the picker
let mut state = FilePickerState::builder()
    .start_dir("~/projects")
    .mode(PickerMode::Both)       // files and directories
    .build();

// In your event loop:
loop {
    terminal.draw(|f| {
        f.render_stateful_widget(FilePicker::default(), f.area(), &mut state);
    })?;

    if let Event::Key(key) = event::read()? {
        state.handle_event(Event::Key(key));
    }

    match state.result() {
        PickerResult::Selected(paths) => {
            // User picked these files/directories
            break;
        }
        PickerResult::Cancelled => break,
        PickerResult::Pending => {}
    }
}
```

## Features

- **List and Tree views** - toggle with `Tab`
- **Vim keybindings** - `hjkl`, `gg`, `G`, `Ctrl+D/U` (arrow keys too)
- **Fuzzy search** - press `/` and start typing; results rank by how well they match, not by directory order
- **Multi-select** - `Space` to toggle, `Enter` to confirm
- **Choose-folder dialogs** - in `DirsOnly` mode the listing starts with `.`, so the folder being browsed can be picked itself, even when it is empty
- **Hidden files** - toggle with `.`
- **Symlink support** - follows symlinks with circular reference detection
- **Filter callback** - show only the files you want
- **Themeable** - customize every color via `FilePickerTheme`

## Builder Options

```rust
let mut state = FilePickerState::builder()
    .start_dir("~/Documents")           // starting directory (default: ".")
    .mode(PickerMode::FilesOnly)        // FilesOnly | DirsOnly | Both
    .view(ViewMode::Tree)               // List | Tree (default: List)
    .show_hidden(true)                  // show dotfiles (default: false)
    .filter(|path| {                    // custom filter
        path.extension()
            .map(|e| e == "rs" || e == "toml")
            .unwrap_or(true)            // always show directories
    })
    .theme(my_theme)                    // custom FilePickerTheme
    .build();
```

## Key Bindings

### Navigation

| Key | Action |
|---|---|
| `j` / `Down` | Move cursor down |
| `k` / `Up` | Move cursor up |
| `l` / `Right` | Enter directory (tree view: expand it) |
| `h` / `Left` / `Backspace` | Go to parent directory (tree view: collapse it) |
| `gg` | Jump to top |
| `G` | Jump to bottom |
| `Ctrl+D` | Half page down |
| `Ctrl+U` | Half page up |
| `~` | Go to home directory |

### Actions

| Key | Action |
|---|---|
| `Enter` | Confirm selection (or enter directory; tree view: expand/collapse it). On the `.` entry in `DirsOnly` mode, confirm the current directory |
| `Space` | Toggle multi-select on current item |
| `Esc` / `q` | Cancel |
| `Tab` | Switch between List and Tree view |
| `.` | Toggle hidden files |
| `/` | Start fuzzy search |

### Search Mode

| Key | Action |
|---|---|
| Type | Filter entries in real time |
| `Enter` | Accept filter, return to normal mode |
| `Esc` | Clear filter, return to normal mode |
| `Up/Down` / `Ctrl+N/P` / `Ctrl+J/K` | Navigate within results |

### Tree View

```
┌─────────────────────────────────────────────┐
│/home/you/projects/ratatree                  │
│   ▾ src/                                    │
│     ▾ view/                                 │
│         list.rs                             │
│         mod.rs                              │
│       entry.rs                              │
│       state.rs                              │
│   ▸ tests/                                  │
│     Cargo.toml                              │
│     config ->                               │
│1/9 | 0 selected | hidden: off | view: tree  │
└─────────────────────────────────────────────┘
```

Press `Tab` to switch to the tree view. Directories expand in place instead of replacing the listing, and the cursor keeps working on the flattened tree, so search, multi-select and mouse clicks behave the same as in list view.

| Key | Action |
|---|---|
| `l` / `Right` | Expand the directory. If it is already expanded, move to its first child. Symlinks are entered instead |
| `h` / `Left` / `Backspace` | Collapse the directory. Otherwise move to the parent node, or to the parent directory from the top level |
| `Enter` | Expand or collapse a directory, or confirm a file |

## Theming

Every visual element is customizable:

```rust
use ratatree::FilePickerTheme;
use ratatui::style::{Color, Modifier, Style};

let theme = FilePickerTheme {
    normal: Style::default(),
    cursor: Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD),
    selected: Style::default().fg(Color::Green),
    directory: Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
    symlink: Style::default().fg(Color::Cyan),
    path_bar: Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    status_bar: Style::default().fg(Color::DarkGray),
    search_input: Style::default().fg(Color::Yellow),
    error: Style::default().fg(Color::Red),
};

let mut state = FilePickerState::builder()
    .theme(theme)
    .build();
```

## Integration with Your App

A common pattern is to show the picker as a modal overlay:

```rust
struct App {
    picker_state: Option<FilePickerState>,
    // ... your app state
}

impl App {
    fn open_picker(&mut self) {
        self.picker_state = Some(
            FilePickerState::builder()
                .start_dir(".")
                .mode(PickerMode::FilesOnly)
                .build()
        );
    }

    fn handle_event(&mut self, event: Event) {
        if let Some(picker) = &mut self.picker_state {
            picker.handle_event(event);
            match picker.result() {
                PickerResult::Selected(paths) => {
                    self.on_files_selected(paths);
                    self.picker_state = None;
                }
                PickerResult::Cancelled => {
                    self.picker_state = None;
                }
                PickerResult::Pending => {}
            }
        } else {
            // your normal event handling
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        if let Some(picker) = &mut self.picker_state {
            frame.render_stateful_widget(FilePicker::default(), frame.area(), picker);
        } else {
            // your normal rendering
        }
    }
}
```

Dropping the state is the simplest option. To reopen the picker where the user left it, keep the state and call `reset()` instead: it puts the result back to `Pending` and clears the selection and search while keeping the current directory and cursor.

## Running the Examples

```bash
cargo run --example basic     # the picker filling the whole terminal
cargo run --example dialogs   # open-file, choose-folder and pick-files modals over a host screen
```

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## License

MIT
