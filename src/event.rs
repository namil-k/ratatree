use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Position;
use std::time::{Duration, Instant};

use crate::search::filter_by_query;
use crate::state::{FilePickerState, InputMode, PickerResult};

/// Main event dispatcher. Does nothing if the picker has already finished.
pub fn handle_event(state: &mut FilePickerState, event: Event) {
    if state.common.result != PickerResult::Pending {
        return;
    }

    match event {
        Event::Key(key) => handle_key(state, key),
        Event::Mouse(mouse) => handle_mouse(state, mouse),
        _ => {}
    }
}

fn handle_key(state: &mut FilePickerState, key: KeyEvent) {
    // Terminals that report key releases (Windows, kitty protocol) would otherwise trigger every binding twice.
    if key.kind == KeyEventKind::Release {
        return;
    }
    // An error stays visible until the user does something else.
    state.common.error_message = None;
    match state.common.input_mode {
        InputMode::Normal => handle_normal_key(state, key),
        InputMode::Search => handle_search_key(state, key),
    }
}

fn handle_normal_key(state: &mut FilePickerState, key: KeyEvent) {
    // gg sequence: check if pending_key is 'g' and not expired (500ms)
    if let Some((pending_char, pending_time)) = state.common.pending_key.take() {
        if pending_char == 'g'
            && pending_time.elapsed() < Duration::from_millis(500)
            && key.code == KeyCode::Char('g')
        {
            state.move_to_top();
            return;
        }
        // Pending key expired or different key - fall through with pending_key cleared (already taken)
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.move_cursor_down();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.move_cursor_up();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            state.descend();
        }
        KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
            state.ascend();
        }
        KeyCode::Char('G') => {
            state.move_to_bottom();
        }
        KeyCode::Char('g') => {
            state.common.pending_key = Some(('g', Instant::now()));
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_half_page_down(state.page_height());
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.move_half_page_up(state.page_height());
        }
        KeyCode::Char(' ') => {
            state.toggle_select();
        }
        KeyCode::Enter => {
            state.confirm();
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            state.cancel();
        }
        KeyCode::Tab => {
            state.toggle_view();
        }
        KeyCode::Char('.') => {
            state.toggle_hidden();
        }
        KeyCode::Char('/') => {
            state.common.input_mode = InputMode::Search;
            state.common.search_query.clear();
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.common.input_mode = InputMode::Search;
            state.common.search_query.clear();
        }
        KeyCode::Char('~') => {
            state.go_home();
        }
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
            // Keep filter active, just return to Normal mode
            state.common.input_mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            state.common.search_query.pop();
            update_search_filter(state);
        }
        KeyCode::Down => {
            state.move_cursor_down();
        }
        KeyCode::Up => {
            state.move_cursor_up();
        }
        // Plain j/k are query text here, so navigation uses Ctrl variants.
        KeyCode::Char('n') | KeyCode::Char('j')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            state.move_cursor_down();
        }
        KeyCode::Char('p') | KeyCode::Char('k')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            state.move_cursor_up();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.common.search_query.push(c);
            update_search_filter(state);
        }
        _ => {}
    }
}

fn update_search_filter(state: &mut FilePickerState) {
    let query = state.common.search_query.clone();
    if query.is_empty() {
        state.common.filtered_indices = None;
    } else {
        let names: Vec<&str> = state.common.entries.iter().map(|e| e.name.as_str()).collect();
        let indices = filter_by_query(&names, &query);
        state.common.filtered_indices = Some(indices);
    }
    *state.view.cursor_mut() = 0;
    *state.view.scroll_offset_mut() = 0;
}

fn handle_mouse(state: &mut FilePickerState, mouse: MouseEvent) {
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        return;
    }
    state.common.error_message = None;
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Map the click through the list area recorded by the last render, so borders, offsets and scrolling are all accounted for.
            let area = state.common.list_area;
            if !area.contains(Position::new(mouse.column, mouse.row)) {
                return;
            }
            let entry_idx = state.view.scroll_offset() + (mouse.row - area.y) as usize;
            if entry_idx < state.visible_count() {
                *state.view.cursor_mut() = entry_idx;
            }
        }
        MouseEventKind::ScrollDown => {
            state.move_cursor_down();
        }
        MouseEventKind::ScrollUp => {
            state.move_cursor_up();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::fs;
    use tempfile::TempDir;

    fn make_state() -> (TempDir, FilePickerState) {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("alpha.txt"), b"").unwrap();
        fs::write(dir.path().join("beta.rs"), b"").unwrap();
        fs::create_dir(dir.path().join("subdir")).unwrap();
        let state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();
        (dir, state)
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn shift_key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::SHIFT))
    }

    #[test]
    fn j_moves_down() {
        let (_dir, mut state) = make_state();
        let initial = state.view.cursor();
        handle_event(&mut state, key(KeyCode::Char('j')));
        assert_eq!(state.view.cursor(), initial + 1);
    }

    #[test]
    fn k_moves_up() {
        let (_dir, mut state) = make_state();
        // Move down first
        handle_event(&mut state, key(KeyCode::Char('j')));
        let after_down = state.view.cursor();
        handle_event(&mut state, key(KeyCode::Char('k')));
        assert_eq!(state.view.cursor(), after_down - 1);
    }

    #[test]
    fn arrow_keys_navigate() {
        let (_dir, mut state) = make_state();
        let initial = state.view.cursor();
        handle_event(&mut state, key(KeyCode::Down));
        assert_eq!(state.view.cursor(), initial + 1);
        handle_event(&mut state, key(KeyCode::Up));
        assert_eq!(state.view.cursor(), initial);
    }

    #[test]
    fn shift_g_moves_to_bottom() {
        let (_dir, mut state) = make_state();
        let count = state.visible_count();
        // 'G' is sent as Shift+g
        handle_event(&mut state, shift_key(KeyCode::Char('G')));
        assert_eq!(state.view.cursor(), count - 1);
    }

    #[test]
    fn space_toggles_selection() {
        let (_dir, mut state) = make_state();
        // Find a file entry (files come after directories)
        let file_idx = state
            .common
            .entries
            .iter()
            .position(|e| e.kind == crate::entry::EntryKind::File)
            .expect("should have a file");
        *state.view.cursor_mut() = file_idx;

        assert!(state.common.selected.is_empty());
        handle_event(&mut state, key(KeyCode::Char(' ')));
        assert_eq!(state.common.selected.len(), 1);
        handle_event(&mut state, key(KeyCode::Char(' ')));
        assert!(state.common.selected.is_empty());
    }

    #[test]
    fn dot_toggles_hidden() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("visible.txt"), b"").unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"").unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .build();
        assert_eq!(state.visible_count(), 1);
        handle_event(&mut state, key(KeyCode::Char('.')));
        assert_eq!(state.visible_count(), 2);
    }

    #[test]
    fn esc_cancels() {
        let (_dir, mut state) = make_state();
        assert_eq!(state.common.result, PickerResult::Pending);
        handle_event(&mut state, key(KeyCode::Esc));
        assert_eq!(state.common.result, PickerResult::Cancelled);
    }

    #[test]
    fn q_cancels() {
        let (_dir, mut state) = make_state();
        handle_event(&mut state, key(KeyCode::Char('q')));
        assert_eq!(state.common.result, PickerResult::Cancelled);
    }

    #[test]
    fn tab_toggles_view() {
        let (_dir, mut state) = make_state();
        assert!(matches!(state.view, crate::view::ViewState::List(_)));
        handle_event(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.view, crate::view::ViewState::Tree(_)));
        handle_event(&mut state, key(KeyCode::Tab));
        assert!(matches!(state.view, crate::view::ViewState::List(_)));
    }

    #[test]
    fn slash_enters_search_mode() {
        let (_dir, mut state) = make_state();
        assert_eq!(state.common.input_mode, InputMode::Normal);
        handle_event(&mut state, key(KeyCode::Char('/')));
        assert_eq!(state.common.input_mode, InputMode::Search);
        assert!(state.common.search_query.is_empty());
    }

    #[test]
    fn search_mode_typing_and_esc_clears() {
        let (_dir, mut state) = make_state();
        // Enter search mode
        handle_event(&mut state, key(KeyCode::Char('/')));
        // Type a character
        handle_event(&mut state, key(KeyCode::Char('a')));
        assert_eq!(state.common.search_query, "a");
        assert!(state.common.filtered_indices.is_some());
        // Esc clears query and filter
        handle_event(&mut state, key(KeyCode::Esc));
        assert_eq!(state.common.input_mode, InputMode::Normal);
        assert!(state.common.search_query.is_empty());
        assert!(state.common.filtered_indices.is_none());
    }

    #[test]
    fn search_mode_enter_keeps_filter() {
        let (_dir, mut state) = make_state();
        // Enter search mode and type a query
        handle_event(&mut state, key(KeyCode::Char('/')));
        handle_event(&mut state, key(KeyCode::Char('a')));
        let filter_before = state.common.filtered_indices.clone();
        assert!(filter_before.is_some());
        // Press Enter - should exit search but keep filter
        handle_event(&mut state, key(KeyCode::Enter));
        assert_eq!(state.common.input_mode, InputMode::Normal);
        assert_eq!(state.common.filtered_indices, filter_before);
    }

    fn ctrl_key(c: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    #[test]
    fn search_mode_accepts_j_and_k_as_text() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), b"").unwrap();
        fs::write(dir.path().join("other.txt"), b"").unwrap();
        let mut state = FilePickerState::builder().start_dir(dir.path()).build();

        handle_event(&mut state, key(KeyCode::Char('/')));
        for c in "json".chars() {
            handle_event(&mut state, key(KeyCode::Char(c)));
        }

        assert_eq!(state.common.search_query, "json");
        assert_eq!(state.visible_count(), 1);
        assert_eq!(state.current_entry().unwrap().name, "package.json");
    }

    #[test]
    fn search_mode_ctrl_keys_navigate() {
        let (_dir, mut state) = make_state();
        handle_event(&mut state, key(KeyCode::Char('/')));

        handle_event(&mut state, ctrl_key('n'));
        assert_eq!(state.view.cursor(), 1);
        handle_event(&mut state, ctrl_key('p'));
        assert_eq!(state.view.cursor(), 0);
        handle_event(&mut state, ctrl_key('j'));
        assert_eq!(state.view.cursor(), 1);
        handle_event(&mut state, ctrl_key('k'));
        assert_eq!(state.view.cursor(), 0);
        assert!(state.common.search_query.is_empty(), "ctrl keys must not enter the query");
    }

    #[test]
    fn error_message_is_cleared_by_next_key() {
        let (_dir, mut state) = make_state();
        state.common.error_message = Some("Circular symlink".to_string());

        handle_event(&mut state, key(KeyCode::Char('j')));

        assert_eq!(state.common.error_message, None);
    }

    #[test]
    fn key_release_events_are_ignored() {
        use ratatui::crossterm::event::{KeyEventKind, KeyEventState};
        let (_dir, mut state) = make_state();
        let release = Event::Key(KeyEvent {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        });

        handle_event(&mut state, release);

        assert_eq!(state.view.cursor(), 0, "a key release must not move the cursor");
    }

    #[test]
    fn l_and_h_expand_and_collapse_in_tree_view() {
        use crate::state::ViewMode;
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("inner.txt"), b"").unwrap();
        let mut state = FilePickerState::builder()
            .start_dir(dir.path())
            .view(ViewMode::Tree)
            .build();
        let root = state.common.current_dir.clone();

        handle_event(&mut state, key(KeyCode::Char('l')));
        assert_eq!(state.visible_count(), 2, "l expands the directory in place");
        assert_eq!(state.common.current_dir, root, "root is unchanged");

        handle_event(&mut state, key(KeyCode::Left));
        assert_eq!(state.visible_count(), 1, "Left collapses it again");
    }

    #[test]
    fn gg_sequence_moves_to_top() {
        let (_dir, mut state) = make_state();
        // Move to bottom first
        let count = state.visible_count();
        *state.view.cursor_mut() = count - 1;
        // Press g once - sets pending key
        handle_event(&mut state, key(KeyCode::Char('g')));
        assert!(state.common.pending_key.is_some());
        // Press g again quickly - should move to top
        handle_event(&mut state, key(KeyCode::Char('g')));
        assert_eq!(state.view.cursor(), 0);
        assert!(state.common.pending_key.is_none());
    }
}
