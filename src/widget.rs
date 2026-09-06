use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, StatefulWidget, Widget};

use crate::entry::EntryKind;
use crate::state::{FilePickerState, InputMode};
use crate::view::ViewState;

#[derive(Default)]
pub struct FilePicker {
    block: Option<Block<'static>>,
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

fn render_path_bar(area: Rect, buf: &mut Buffer, state: &FilePickerState) {
    let path_str = state.common.current_dir.to_string_lossy().to_string();
    let style = state.common.theme.path_bar;
    let para = Paragraph::new(Line::from(Span::styled(path_str, style)));
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

    for (row, entry) in entries.iter().enumerate().skip(scroll_offset).take(visible_height) {
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

        // Line handles grapheme widths, so wide characters (CJK, emoji)
        // take the cells they need instead of overlapping the next glyph.
        let line = Line::from(vec![
            Span::styled(prefix, prefix_style),
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

    // Error message takes priority
    if let Some(err) = &state.common.error_message {
        let para = Paragraph::new(Line::from(Span::styled(err.clone(), theme.error)));
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
            let hidden_str = if state.common.show_hidden { "on" } else { "off" };
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
    fn renders_without_panic() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| {
            let widget = FilePicker::default().block(Block::default().borders(Borders::ALL));
            frame.render_stateful_widget(widget, frame.area(), &mut state);
        }).unwrap();
    }

    #[test]
    fn renders_wide_characters_without_overlap() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("한글.txt"), b"").unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        let backend = TestBackend::new(30, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
        }).unwrap();

        // Row 1 is the first list row. After the 3-column prefix each Hangul
        // syllable occupies two cells: the glyph, then a blank continuation.
        let buf = terminal.backend().buffer();
        let symbols: Vec<&str> = (3..11).map(|x| buf[(x, 1)].symbol()).collect();
        assert_eq!(symbols, ["한", " ", "글", " ", ".", "t", "x", "t"]);
    }

    fn click(column: u16, row: u16) -> crossterm::event::Event {
        use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        // Bordered widget at an offset: border on row 5, path bar on row 6,
        // list rows start at row 7, columns 11..=38.
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            let widget = FilePicker::default().block(Block::default().borders(Borders::ALL));
            frame.render_stateful_widget(widget, Rect::new(10, 5, 30, 8), &mut state);
        }).unwrap();

        state.handle_event(click(15, 8));
        assert_eq!(state.view.cursor(), 1, "second list row");
        state.handle_event(click(15, 7));
        assert_eq!(state.view.cursor(), 0, "first list row");
        state.handle_event(click(15, 6));
        assert_eq!(state.view.cursor(), 0, "path bar click is ignored");
        state.handle_event(click(15, 8));
        state.handle_event(click(5, 8));
        assert_eq!(state.view.cursor(), 1, "click left of the widget is ignored");
        state.handle_event(click(15, 11));
        assert_eq!(state.view.cursor(), 1, "click below the last entry is ignored");
    }

    #[test]
    fn mouse_click_accounts_for_scroll_offset() {
        let dir = make_dir_with_n_files(10);
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();
        *state.view.cursor_mut() = 5;

        // 5 rows: path bar, 3 list rows, status bar. Cursor 5 scrolls to entries 3..=5.
        let backend = TestBackend::new(30, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
        }).unwrap();
        assert_eq!(state.view.scroll_offset(), 3);

        state.handle_event(click(0, 1));
        assert_eq!(state.view.cursor(), 3, "first visible row is entry 3");
    }

    #[test]
    fn mouse_click_before_first_render_is_ignored() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();
        *state.view.cursor_mut() = 1;

        state.handle_event(click(0, 2));

        assert_eq!(state.view.cursor(), 1);
    }

    #[test]
    fn half_page_uses_rendered_height() {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        let dir = make_dir_with_n_files(30);
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        // 14 rows: path bar + 12 list rows + status bar, so half a page is 6.
        let backend = TestBackend::new(30, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| {
            frame.render_stateful_widget(FilePicker::default(), frame.area(), &mut state);
        }).unwrap();

        state.handle_event(Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)));
        assert_eq!(state.view.cursor(), 6);
        state.handle_event(Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)));
        assert_eq!(state.view.cursor(), 0);
    }

    #[test]
    fn renders_empty_directory() {
        let dir = TempDir::new().unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| {
            let widget = FilePicker::default();
            frame.render_stateful_widget(widget, frame.area(), &mut state);
        }).unwrap();
    }

    #[test]
    fn renders_with_selection() {
        let dir = make_dir_with_files();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();

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

        terminal.draw(|frame| {
            let widget = FilePicker::default();
            frame.render_stateful_widget(widget, frame.area(), &mut state);
        }).unwrap();
    }
}
