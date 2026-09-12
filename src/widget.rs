//! Drawing the picker.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget};

use crate::entry::EntryKind;
use crate::state::{FilePickerState, InputMode};
use crate::view::ViewState;

/// Tree view markers shown in front of directory names.
const EXPANDED_MARKER: &str = "\u{25be} "; // ▾
const COLLAPSED_MARKER: &str = "\u{25b8} "; // ▸

/// Draws a [`FilePickerState`].
///
/// The widget holds no state of its own beyond an optional [`Block`], so construct one per frame with [`FilePicker::default`].
///
/// It splits its area into a one-row path bar, the entry list, and a one-row status bar, and needs at least three rows or it draws nothing. Each entry row is a three-column prefix (`*` when multi-selected), the name, and a suffix (`/` for directories, `->` for symlinks); tree view adds two spaces of indent per level and a `▾` or `▸` marker on directories. The cursor is a style applied across the full row rather than a marker character.
///
/// Rendering records the list area on the state, which is what makes mouse clicks land on the right entry, so clicks arriving before the first render are ignored.
#[derive(Default)]
pub struct FilePicker {
    block: Option<Block<'static>>,
}

impl FilePicker {
    /// Draws the picker inside this block and uses its inner area, letting the caller supply borders, a title or padding.
    pub fn block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
    }
}

impl StatefulWidget for FilePicker {
    type State = FilePickerState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        // Apply outer block if provided, get inner area
        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        // Need at least 3 rows: path bar (1) + list (>=1) + status bar (1)
        if inner.height < 3 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

        render_path_bar(chunks[0], buf, state);
        render_file_list(chunks[1], buf, state);
        render_status_bar(chunks[2], buf, state);
    }
}

/// Shortens a path for a bar `width` columns wide by dropping leading components, so the directory the user is actually in stays visible. A lone last component that is still too wide is cut from the front.
fn truncate_path_left(path: &str, width: usize) -> String {
    let display_width = |s: &str| Span::raw(s).width();
    if display_width(path) <= width {
        return path.to_string();
    }
    const ELLIPSIS: &str = "…";
    if width <= 1 {
        return ELLIPSIS.to_string();
    }
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let parts: Vec<&str> = path.split(std::path::MAIN_SEPARATOR).collect();
    for start in 1..parts.len() {
        let candidate = format!("{ELLIPSIS}{sep}{}", parts[start..].join(&sep));
        if display_width(&candidate) <= width {
            return candidate;
        }
    }
    let last = parts.last().copied().unwrap_or("");
    let mut chars: Vec<char> = last.chars().collect();
    loop {
        let candidate = format!("{ELLIPSIS}{}", chars.iter().collect::<String>());
        if display_width(&candidate) <= width || chars.is_empty() {
            return candidate;
        }
        chars.remove(0);
    }
}

fn render_path_bar(area: Rect, buf: &mut Buffer, state: &FilePickerState) {
    let path_str = state.common.current_dir.to_string_lossy();
    let shown = truncate_path_left(&path_str, area.width as usize);
    let style = state.common.theme.path_bar;
    let para = Paragraph::new(Line::from(Span::styled(shown, style)));
    para.render(area, buf);
}

fn render_file_list(area: Rect, buf: &mut Buffer, state: &mut FilePickerState) {
    state.common.list_area = area;
    let entries = state.visible_entries();

    if entries.is_empty() {
        let style = state.common.theme.status_bar;
        let para = Paragraph::new(Line::from(Span::styled("(empty)", style)));
        para.render(area, buf);
        return;
    }

    let visible_height = area.height as usize;
    let cursor = state.view.cursor();

    // Update scroll offset so cursor stays visible
    {
        let scroll = state.view.scroll_offset_mut();
        if cursor < *scroll {
            *scroll = cursor;
        } else if cursor >= *scroll + visible_height {
            *scroll = cursor + 1 - visible_height;
        }
    }

    let scroll_offset = state.view.scroll_offset();

    let entries = state.visible_entries(); // re-borrow after mut borrow ends

    let theme = &state.common.theme;
    let selected_paths = &state.common.selected;
    let tree = match &state.view {
        ViewState::Tree(tree) => Some(tree),
        ViewState::List(_) => None,
    };

    for (row, entry) in entries
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
    {
        let y = area.y + (row - scroll_offset) as u16;
        let is_cursor = row == cursor;
        let is_selected = selected_paths.contains(&entry.path);

        let kind_style = match entry.kind {
            EntryKind::Directory => theme.directory,
            EntryKind::Symlink => theme.symlink,
            EntryKind::File => theme.normal,
        };
        let (prefix, prefix_style, name_style) = if is_selected {
            (" * ", theme.selected, kind_style.patch(theme.selected))
        } else {
            ("   ", Style::default(), kind_style)
        };
        let suffix = match entry.kind {
            EntryKind::Directory => "/",
            EntryKind::Symlink => " ->",
            EntryKind::File => "",
        };

        // Tree view: indent by depth and mark directories as expanded or collapsed. Files get a blank marker so names line up per level.
        let indent = "  ".repeat(entry.depth);
        let marker = match tree {
            Some(tree) if entry.kind == EntryKind::Directory => {
                if tree.is_expanded(&entry.path) {
                    EXPANDED_MARKER
                } else {
                    COLLAPSED_MARKER
                }
            }
            Some(_) => "  ",
            None => "",
        };

        // Line handles grapheme widths, so wide characters (CJK, emoji) take the cells they need instead of overlapping the next glyph.
        let line = Line::from(vec![
            Span::styled(prefix, prefix_style),
            Span::styled(format!("{indent}{marker}"), name_style),
            Span::styled(entry.name.as_str(), name_style),
            Span::styled(suffix, name_style),
        ]);
        buf.set_line(area.x, y, &line, area.width);

        if is_cursor {
            buf.set_style(Rect::new(area.x, y, area.width, 1), theme.cursor);
        }
    }
}

fn render_status_bar(area: Rect, buf: &mut Buffer, state: &FilePickerState) {
    let theme = &state.common.theme;

    // A one-off message wins over the standing read error, which in turn wins over the ordinary status line.
    let error = state
        .common
        .error_message
        .as_deref()
        .or(state.common.read_error.as_deref());
    if let Some(err) = error {
        let para = Paragraph::new(Line::from(Span::styled(err.to_string(), theme.error)));
        para.render(area, buf);
        return;
    }

    match state.common.input_mode {
        InputMode::Search => {
            let query = &state.common.search_query;
            let count = state.visible_count();
            let text = format!("/ {}  ({} matches)", query, count);
            let para = Paragraph::new(Line::from(Span::styled(text, theme.search_input)));
            para.render(area, buf);
        }
        InputMode::Normal => {
            let selected_count = state.common.selected.len();
            let hidden_str = if state.common.show_hidden {
                "on"
            } else {
                "off"
            };
            let view_str = match &state.view {
                ViewState::List(_) => "list",
                ViewState::Tree(_) => "tree",
            };
            let text = format!(
                "{} selected | hidden: {} | view: {}",
                selected_count, hidden_str, view_str
            );
            let para = Paragraph::new(Line::from(Span::styled(text, theme.status_bar)));
            para.render(area, buf);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Borders;
    use ratatui::Terminal;
    use std::fs;
    use tempfile::TempDir;

    fn make_dir_with_files() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("alpha.txt"), b"").unwrap();
        fs::write(dir.path().join("beta.rs"), b"").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        dir
    }

    #[test]
    fn short_path_is_left_alone() {
        assert_eq!(truncate_path_left("/a/b", 10), "/a/b");
        assert_eq!(truncate_path_left("/a/b", 4), "/a/b");
    }

    #[test]
    fn long_path_keeps_its_tail_and_cuts_at_a_component() {
        let sep = std::path::MAIN_SEPARATOR;
        let path = [
            "",
            "Users",
            "namilkim",
            "Library",
            "Application Support",
            "app",
        ]
        .join(&sep.to_string());
        let got = truncate_path_left(&path, 30);
        assert_eq!(got, format!("…{sep}Application Support{sep}app"));
        assert!(Span::raw(&got).width() <= 30);
    }

    #[test]
    fn last_component_wider_than_the_bar_is_cut_from_the_front() {
        let sep = std::path::MAIN_SEPARATOR;
        let path = ["", "x", "abcdefghijklmnop"].join(&sep.to_string());
        assert_eq!(truncate_path_left(&path, 8), "…jklmnop");
    }

    #[test]
    fn wide_characters_are_measured_by_columns() {
        let sep = std::path::MAIN_SEPARATOR;
        let path = ["", "홈", "문서", "프로젝트"].join(&sep.to_string());
        // "프로젝트" is 8 columns; with the separator and the ellipsis that is 10.
        let got = truncate_path_left(&path, 10);
        assert_eq!(got, format!("…{sep}프로젝트"));
        assert_eq!(Span::raw(&got).width(), 10);
    }

    #[test]
    fn bar_of_one_column_shows_only_the_ellipsis() {
        assert_eq!(truncate_path_left("/a/b", 1), "…");
    }

    #[test]
    fn path_bar_keeps_the_directory_name_when_the_panel_is_narrow() {
        let dir = TempDir::new().unwrap();
        let deep = dir
            .path()
            .join("a-rather-long-directory-name")
            .join("target");
        fs::create_dir_all(&deep).unwrap();
        let mut state = FilePickerState::builder().start_dir(&deep).build();

        let backend = TestBackend::new(20, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();

        let bar = row_text(&terminal, 0);
        assert!(bar.starts_with('…'), "got {bar:?}");
        assert!(bar.ends_with("target"), "got {bar:?}");
    }

    #[test]
    fn renders_without_panic() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let widget = FilePicker::default().block(Block::default().borders(Borders::ALL));
                frame.render_stateful_widget(widget, frame.area(), &mut state);
            })
            .unwrap();
    }

    #[test]
    fn renders_wide_characters_without_overlap() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("한글.txt"), b"").unwrap();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        let backend = TestBackend::new(30, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();

        // Row 1 is the first list row. After the 3-column prefix each Hangul syllable occupies two cells: the glyph, then a blank continuation.
        let buf = terminal.backend().buffer();
        let symbols: Vec<&str> = (3..11).map(|x| buf[(x, 1)].symbol()).collect();
        assert_eq!(symbols, ["한", " ", "글", " ", ".", "t", "x", "t"]);
    }

    fn click(column: u16, row: u16) -> ratatui::crossterm::event::Event {
        use ratatui::crossterm::event::{
            Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn make_dir_with_n_files(n: usize) -> TempDir {
        let dir = TempDir::new().unwrap();
        for i in 0..n {
            fs::write(dir.path().join(format!("f{i:02}.txt")), b"").unwrap();
        }
        dir
    }

    #[test]
    fn mouse_click_maps_to_rendered_row() {
        let dir = make_dir_with_files(); // subdir, alpha.txt, beta.rs
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        // Bordered widget at an offset: border on row 5, path bar on row 6, list rows start at row 7, columns 11..=38.
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let widget = FilePicker::default().block(Block::default().borders(Borders::ALL));
                frame.render_stateful_widget(widget, Rect::new(10, 5, 30, 8), &mut state);
            })
            .unwrap();

        state.handle_event(click(15, 8));
        assert_eq!(state.view.cursor(), 1, "second list row");
        state.handle_event(click(15, 7));
        assert_eq!(state.view.cursor(), 0, "first list row");
        state.handle_event(click(15, 6));
        assert_eq!(state.view.cursor(), 0, "path bar click is ignored");
        state.handle_event(click(15, 8));
        state.handle_event(click(5, 8));
        assert_eq!(
            state.view.cursor(),
            1,
            "click left of the widget is ignored"
        );
        state.handle_event(click(15, 11));
        assert_eq!(
            state.view.cursor(),
            1,
            "click below the last entry is ignored"
        );
    }

    #[test]
    fn mouse_click_accounts_for_scroll_offset() {
        let dir = make_dir_with_n_files(10);
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();
        *state.view.cursor_mut() = 5;

        // 5 rows: path bar, 3 list rows, status bar. Cursor 5 scrolls to entries 3..=5.
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();
        assert_eq!(state.view.scroll_offset(), 3);

        state.handle_event(click(0, 1));
        assert_eq!(state.view.cursor(), 3, "first visible row is entry 3");
    }

    #[test]
    fn mouse_click_before_first_render_is_ignored() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();
        *state.view.cursor_mut() = 1;

        state.handle_event(click(0, 2));

        assert_eq!(state.view.cursor(), 1);
    }

    #[test]
    fn half_page_uses_rendered_height() {
        use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let dir = make_dir_with_n_files(30);
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        // 14 rows: path bar + 12 list rows + status bar, so half a page is 6.
        let backend = TestBackend::new(30, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();

        state.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(state.view.cursor(), 6);
        state.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
        )));
        assert_eq!(state.view.cursor(), 0);
    }

    fn row_text(terminal: &Terminal<TestBackend>, y: u16) -> String {
        let buf = terminal.backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[test]
    fn tree_view_renders_indent_and_markers() {
        use crate::state::ViewMode;
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("a_dir").join("nested")).unwrap();
        fs::write(dir.path().join("a_dir").join("inner.txt"), b"").unwrap();
        fs::create_dir(dir.path().join("b_dir")).unwrap();
        fs::write(dir.path().join("top.txt"), b"").unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .view(ViewMode::Tree)
            .build();
        state.expand_current(); // a_dir

        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();

        let rows: Vec<String> = (1..6).map(|y| row_text(&terminal, y)).collect();
        assert_eq!(
            rows,
            [
                "   ▾ a_dir/",
                "     ▸ nested/",
                "       inner.txt",
                "   ▸ b_dir/",
                "     top.txt",
            ]
        );
    }

    #[test]
    fn status_bar_shows_read_error_while_directory_is_unreadable() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("no-such-directory");
        let mut state = FilePickerState::builder().start_dir(&missing).build();
        assert!(state.common.read_error.is_some());

        let backend = TestBackend::new(60, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();

        let status = row_text(&terminal, 3);
        assert!(
            status.starts_with("Cannot read directory:"),
            "status bar should explain the empty listing, got {status:?}"
        );
    }

    #[test]
    fn transient_error_message_takes_priority_over_read_error() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("no-such-directory");
        let mut state = FilePickerState::builder().start_dir(&missing).build();
        state.common.error_message = Some("Circular symlink".to_string());

        let backend = TestBackend::new(60, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
            })
            .unwrap();

        assert_eq!(row_text(&terminal, 3), "Circular symlink");
    }

    #[test]
    fn renders_empty_directory() {
        let dir = TempDir::new().unwrap();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let widget = FilePicker::default();
                frame.render_stateful_widget(widget, frame.area(), &mut state);
            })
            .unwrap();
    }

    #[test]
    fn renders_with_selection() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        // Find a file entry and select it
        let file_idx = state
            .common
            .entries
            .iter()
            .position(|e| e.kind == EntryKind::File)
            .expect("should have a file entry");
        *state.view.cursor_mut() = file_idx;
        state.toggle_select();

        assert_eq!(state.common.selected.len(), 1);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let widget = FilePicker::default();
                frame.render_stateful_widget(widget, frame.area(), &mut state);
            })
            .unwrap();
    }
}
