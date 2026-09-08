#![warn(missing_docs)]

//! A file and directory picker widget for [ratatui](https://docs.rs/ratatui).
//!
//! The picker is split the way ratatui's stateful widgets usually are. [`FilePickerState`] owns everything that persists between frames (the current directory, the cursor, the selection, the search query), [`FilePicker`] is a zero-sized [`StatefulWidget`](ratatui::widgets::StatefulWidget) that draws that state, and [`PickerResult`] reports what the user did.
//!
//! # Example
//!
//! ```
//! use ratatree::{FilePickerState, PickerMode, PickerResult};
//!
//! let mut state = FilePickerState::builder()
//!     .start_dir(".")
//!     .mode(PickerMode::FilesOnly)
//!     .build();
//!
//! assert_eq!(state.result(), PickerResult::Pending);
//! ```
//!
//! Wiring it into an event loop takes three calls per frame: render the widget, feed it the terminal event, then check the result.
//!
//! ```no_run
//! # use ratatree::crossterm::event::{self, Event};
//! # use ratatree::{FilePicker, FilePickerState, PickerResult};
//! # fn run(terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
//! # let mut state = FilePickerState::builder().build();
//! loop {
//!     terminal.draw(|frame| {
//!         frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
//!     })?;
//!
//!     state.handle_event(event::read()?);
//!
//!     match state.result() {
//!         PickerResult::Selected(paths) => {
//!             // The user picked these paths.
//!             break;
//!         }
//!         PickerResult::Cancelled => break,
//!         PickerResult::Pending => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Views
//!
//! The picker has two views, toggled with `Tab`. The list view shows one directory at a time and `l`/`h` change which directory that is. The tree view expands directories in place and `l`/`h` expand and collapse them, following the convention used by VS Code's explorer and nvim-tree.
//!
//! Both views share one flattened entry list, so the cursor, fuzzy search, multi-select and mouse handling behave identically in each. [`Entry::depth`] is what distinguishes them: it is always `0` in list view, and in tree view it is the nesting level used for indentation and for parent/child movement.
//!
//! # Events
//!
//! [`FilePickerState::handle_event`] takes a [`crossterm::event::Event`]. Because that type comes from a dependency, an application whose `crossterm` version differs from the one ratatui was built with would hand over a same-named but incompatible type. To rule that out, this crate does not depend on `crossterm` directly and re-exports the one it was built against as [`crossterm`]. Import the event types from there and the versions cannot drift.
//!
//! Applications that want their own key map can skip `handle_event` entirely and drive the state methods ([`move_cursor_down`](FilePickerState::move_cursor_down), [`descend`](FilePickerState::descend), [`toggle_select`](FilePickerState::toggle_select) and friends) directly.

mod entry;
mod event;
mod search;
mod state;
mod theme;
pub mod view;
mod widget;

/// The `crossterm` version this crate is built against, re-exported so callers pass `handle_event` an `Event` of exactly the type it expects.
pub use ratatui::crossterm;

pub use entry::{Entry, EntryKind};
pub use state::{
    CommonState, FilePickerBuilder, FilePickerState, FilterFn, InputMode, PickerMode, PickerResult,
    ViewMode,
};
pub use theme::FilePickerTheme;
pub use view::ViewState;
pub use widget::FilePicker;
