use ratatree::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatree::{FilePickerState, PickerMode, PickerResult, ViewMode};
use std::fs;
use tempfile::TempDir;

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn key_char(c: char) -> Event {
    key(KeyCode::Char(c))
}

/// Creates a temp directory with the following structure:
/// - src/
///   - main.rs
///   - lib.rs
/// - tests/
///   - test.rs
/// - Cargo.toml
/// - README.md
/// - .gitignore
///
/// Sorted dirs-first alphabetically: src/, tests/, Cargo.toml, README.md
/// Hidden files (.gitignore) hidden by default.
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

/// Test: navigate down to Cargo.toml and press Enter to confirm.
/// Sorted order: src/ (0), tests/ (1), Cargo.toml (2), README.md (3)
#[test]
fn navigate_and_select_single_file() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    // Move down twice to reach index 2 (Cargo.toml)
    state.handle_event(key(KeyCode::Down));
    state.handle_event(key(KeyCode::Down));

    let entry = state.current_entry().expect("should have entry");
    assert_eq!(entry.name, "Cargo.toml");

    state.handle_event(key(KeyCode::Enter));

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 1);
            assert!(
                paths[0].ends_with("Cargo.toml"),
                "expected Cargo.toml, got {:?}",
                paths[0]
            );
        }
        other => panic!("expected Selected, got {:?}", other),
    }
}

/// Test: select Cargo.toml and README.md with Space, then confirm.
#[test]
fn multi_select_across_navigation() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    // Navigate to Cargo.toml (index 2)
    state.handle_event(key(KeyCode::Down));
    state.handle_event(key(KeyCode::Down));
    assert_eq!(state.current_entry().unwrap().name, "Cargo.toml");

    // Select Cargo.toml
    state.handle_event(key_char(' '));
    assert_eq!(state.common.selected.len(), 1);

    // Move down to README.md (index 3)
    state.handle_event(key(KeyCode::Down));
    assert_eq!(state.current_entry().unwrap().name, "README.md");

    // Select README.md
    state.handle_event(key_char(' '));
    assert_eq!(state.common.selected.len(), 2);

    // Confirm
    state.handle_event(key(KeyCode::Enter));

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 2);
            let names: Vec<&str> = paths
                .iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
                .collect();
            assert!(
                names.contains(&"Cargo.toml"),
                "expected Cargo.toml in {:?}",
                names
            );
            assert!(
                names.contains(&"README.md"),
                "expected README.md in {:?}",
                names
            );
        }
        other => panic!("expected Selected, got {:?}", other),
    }
}

/// Test: enter src/ directory, verify current_dir ends with "src", then go back.
#[test]
fn directory_navigation() {
    let tmp = setup_test_dir();
    let original_dir = tmp.path().canonicalize().unwrap();

    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    // First entry should be src/ (dirs-first, alphabetical)
    let first = state.current_entry().expect("should have entry");
    assert_eq!(first.name, "src");

    // Press Enter to enter src/
    state.handle_event(key(KeyCode::Enter));

    // current_dir should end with "src"
    let current = state.common.current_dir.clone();
    assert!(
        current.ends_with("src"),
        "expected current_dir to end with 'src', got {:?}",
        current
    );

    // Press h to go back
    state.handle_event(key_char('h'));

    // Should be back at original dir
    let back = state
        .common
        .current_dir
        .canonicalize()
        .unwrap_or_else(|_| state.common.current_dir.clone());
    assert_eq!(back, original_dir, "expected to be back at original dir");
}

/// Test: toggle view with Tab, verify current_dir and entry count are unchanged.
#[test]
fn view_toggle_preserves_directory() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    let dir_before = state.common.current_dir.clone();
    let count_before = state.visible_count();

    state.handle_event(key(KeyCode::Tab));

    assert_eq!(state.common.current_dir, dir_before);
    assert_eq!(state.visible_count(), count_before);

    // Toggle back
    state.handle_event(key(KeyCode::Tab));

    assert_eq!(state.common.current_dir, dir_before);
    assert_eq!(state.visible_count(), count_before);
}

/// Test: pressing '.' reveals hidden files (adds .gitignore to list).
#[test]
fn hidden_files_toggle() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    let count_before = state.visible_count();
    // .gitignore is hidden so not counted initially
    assert!(
        !state
            .visible_entries()
            .iter()
            .any(|e| e.name == ".gitignore"),
        "should not see .gitignore before toggle"
    );

    state.handle_event(key_char('.'));

    let count_after = state.visible_count();
    assert!(
        count_after > count_before,
        "entry count should increase when showing hidden files ({} -> {})",
        count_before,
        count_after
    );
    assert!(
        state
            .visible_entries()
            .iter()
            .any(|e| e.name == ".gitignore"),
        "should see .gitignore after toggle"
    );
}

/// Test: search for "Car", confirm filter to 1 match, then Enter to confirm selection.
#[test]
fn search_and_confirm() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    // Enter search mode
    state.handle_event(key_char('/'));
    assert_eq!(state.common.input_mode, ratatree::InputMode::Search);

    // Type "Car"
    state.handle_event(key_char('C'));
    state.handle_event(key_char('a'));
    state.handle_event(key_char('r'));

    // Should be filtered to 1 result
    assert_eq!(
        state.visible_count(),
        1,
        "expected 1 match for 'Car', got {}",
        state.visible_count()
    );
    let matched = state.current_entry().expect("should have match");
    assert_eq!(matched.name, "Cargo.toml");

    // Press Enter to exit search (keep filter)
    state.handle_event(key(KeyCode::Enter));
    assert_eq!(state.common.input_mode, ratatree::InputMode::Normal);
    assert_eq!(
        state.visible_count(),
        1,
        "filter should be preserved after Enter"
    );

    // Press Enter again to confirm selection
    state.handle_event(key(KeyCode::Enter));

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 1);
            assert!(
                paths[0].ends_with("Cargo.toml"),
                "expected Cargo.toml, got {:?}",
                paths[0]
            );
        }
        other => panic!("expected Selected, got {:?}", other),
    }
}

/// Test: FilesOnly mode should not allow Space-selecting a directory.
#[test]
fn files_only_mode_blocks_dir_selection() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .mode(PickerMode::FilesOnly)
        .build();

    // First entry should be src/ (a directory)
    let first = state.current_entry().expect("should have entry");
    assert_eq!(
        first.kind,
        ratatree::EntryKind::Directory,
        "first entry should be a dir"
    );

    // Attempt to select via Space
    state.handle_event(key_char(' '));

    assert_eq!(
        state.common.selected.len(),
        0,
        "FilesOnly mode should block directory selection"
    );
}

/// Test: pressing Esc returns PickerResult::Cancelled.
#[test]
fn cancel_returns_cancelled() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    assert_eq!(state.result(), PickerResult::Pending);
    state.handle_event(key(KeyCode::Esc));
    assert_eq!(state.result(), PickerResult::Cancelled);
}

/// Test: in tree view, `l` expands src/ in place, `j` moves onto lib.rs and
/// Enter picks it with its full nested path.
#[test]
fn tree_view_expand_and_select_nested_file() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder()
        .start_dir(tmp.path())
        .view(ViewMode::Tree)
        .build();
    let root = state.common.current_dir.clone();

    state.handle_event(key_char('l'));

    assert_eq!(
        state.common.current_dir, root,
        "expanding does not change the root"
    );
    let names: Vec<&str> = state
        .visible_entries()
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "src",
            "lib.rs",
            "main.rs",
            "tests",
            "Cargo.toml",
            "README.md"
        ]
    );

    state.handle_event(key_char('j'));
    assert_eq!(state.current_entry().unwrap().name, "lib.rs");

    state.handle_event(key(KeyCode::Enter));

    match state.result() {
        PickerResult::Selected(paths) => {
            assert_eq!(paths.len(), 1);
            assert!(
                paths[0].ends_with("src/lib.rs"),
                "expected src/lib.rs, got {:?}",
                paths[0]
            );
        }
        other => panic!("expected Selected, got {:?}", other),
    }
}

/// Test: symlink cycle detection on Unix.
/// A symlink that resolves to the current directory (or an ancestor of it) is
/// blocked with an error message, while a symlink to an unrelated directory
/// is followed normally.
#[cfg(unix)]
#[test]
fn symlink_cycle_detection() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let canonical_tmp = tmp.path().canonicalize().unwrap();

    // self_link -> tmp (self-loop), other_link -> tmp/other (fine)
    symlink(tmp.path(), tmp.path().join("self_link")).unwrap();
    fs::create_dir(tmp.path().join("other")).unwrap();
    symlink(tmp.path().join("other"), tmp.path().join("other_link")).unwrap();

    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    let idx_of = |state: &FilePickerState, name: &str| {
        state
            .common
            .entries
            .iter()
            .position(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} should be listed"))
    };

    // self_link is listed as a symlink and entering it is blocked immediately.
    // Note: Enter/confirm does not enter symlinks; 'l'/Right arrow calls enter_directory directly.
    *state.view.cursor_mut() = idx_of(&state, "self_link");
    assert_eq!(
        state.current_entry().unwrap().kind,
        ratatree::EntryKind::Symlink
    );
    state.handle_event(key(KeyCode::Right));

    assert_eq!(
        state.common.error_message.as_deref(),
        Some("Circular symlink"),
        "entering a symlink to the current directory should be blocked"
    );
    assert_eq!(
        state.common.current_dir.canonicalize().unwrap(),
        canonical_tmp,
        "current_dir should remain unchanged after circular symlink block"
    );

    // other_link resolves to an unrelated directory and is followed.
    *state.view.cursor_mut() = idx_of(&state, "other_link");
    state.handle_event(key(KeyCode::Right));

    assert_eq!(
        state.common.current_dir.canonicalize().unwrap(),
        canonical_tmp.join("other"),
        "symlink to an unrelated directory should be followed"
    );
}

/// `clamp_cursor` is public so a custom key map can pull the cursor back into range after it changes the entry list itself.
#[test]
fn clamp_cursor_is_public_and_pulls_cursor_into_range() {
    let tmp = setup_test_dir();
    let mut state = FilePickerState::builder().start_dir(tmp.path()).build();

    *state.view.cursor_mut() = 100;
    state.clamp_cursor();

    assert_eq!(state.view.cursor(), state.visible_count() - 1);
}

/// `FilePickerState` is `Debug` so an application can log it or hold it inside its own `#[derive(Debug)]` state.
#[test]
fn state_is_debug() {
    let tmp = setup_test_dir();
    let state = FilePickerState::builder().start_dir(tmp.path()).build();

    let dump = format!("{:?}", state);

    assert!(dump.starts_with("FilePickerState"), "got {dump}");
    assert!(dump.contains("current_dir"), "got {dump}");
}
