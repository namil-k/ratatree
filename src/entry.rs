//! Directory entries and the code that reads them off disk.

use std::fs;
use std::path::{Path, PathBuf};

/// What a directory entry is, as reported by [`std::fs::symlink_metadata`].
///
/// Symlinks are their own kind rather than being resolved to what they point at, so the picker can show them as links and decide separately whether following one is safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A symbolic link, whatever its target turns out to be.
    Symlink,
}

/// One row in the picker: a single file, directory or symlink.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The final component of the path, without any directory part.
    pub name: String,
    /// Full path to the entry. This is what ends up in [`PickerResult::Selected`](crate::PickerResult::Selected).
    pub path: PathBuf,
    /// Whether this is a file, a directory or a symlink.
    pub kind: EntryKind,
    /// Whether the name starts with a dot. Hidden entries are filtered out unless the picker is showing them.
    pub is_hidden: bool,
    /// Nesting level below the picker's current directory. Always `0` in list view; in tree view, children of an expanded directory are one deeper.
    pub depth: usize,
}

impl Entry {
    /// Reads the entry at `path`, or returns `None` if it has no file name or cannot be stat'd.
    ///
    /// The kind comes from [`symlink_metadata`](std::fs::symlink_metadata) rather than [`metadata`](std::fs::metadata), so a symlink is reported as [`EntryKind::Symlink`] instead of as whatever it points at. `depth` is always `0` here; the tree view sets it while flattening.
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
        Some(Entry { name, path: path.to_path_buf(), kind, is_hidden, depth: 0 })
    }
}

/// Reads `dir` into a sorted list of entries, dropping hidden ones unless `show_hidden` is set.
///
/// `filter` is applied to files and symlinks only. Directories always pass, because hiding them would make their contents unreachable in a picker that navigates by entering them.
///
/// Directories sort before everything else, then names sort case-insensitively. An unreadable directory yields an empty list rather than an error, so a permission problem shows up as an empty pane instead of breaking the render.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn entry_from_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, b"").unwrap();
        let entry = Entry::from_path(&file_path).unwrap();
        assert_eq!(entry.name, "hello.txt");
        assert_eq!(entry.kind, EntryKind::File);
        assert!(!entry.is_hidden);
    }

    #[test]
    fn entry_from_path_has_depth_zero() {
        let dir = TempDir::new().unwrap();
        let entry = Entry::from_path(dir.path()).unwrap();
        assert_eq!(entry.depth, 0);
    }

    #[test]
    fn entry_from_directory() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        let entry = Entry::from_path(&sub).unwrap();
        assert_eq!(entry.kind, EntryKind::Directory);
    }

    #[test]
    fn entry_hidden_detection() {
        let dir = TempDir::new().unwrap();
        let hidden = dir.path().join(".hidden");
        fs::write(&hidden, b"").unwrap();
        let entry = Entry::from_path(&hidden).unwrap();
        assert!(entry.is_hidden);
    }

    #[test]
    fn read_entries_filters_hidden() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("visible.txt"), b"").unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"").unwrap();
        let entries = read_entries(dir.path(), false, None);
        assert!(entries.iter().all(|e| !e.is_hidden));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "visible.txt");
    }

    #[test]
    fn read_entries_shows_hidden() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("visible.txt"), b"").unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"").unwrap();
        let entries = read_entries(dir.path(), true, None);
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn read_entries_sorts_dirs_first() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("aaa.txt"), b"").unwrap();
        fs::write(dir.path().join("zzz.txt"), b"").unwrap();
        fs::create_dir(dir.path().join("mmm")).unwrap();
        let entries = read_entries(dir.path(), false, None);
        assert_eq!(entries[0].name, "mmm");
        assert_eq!(entries[0].kind, EntryKind::Directory);
        assert_eq!(entries[1].name, "aaa.txt");
        assert_eq!(entries[2].name, "zzz.txt");
    }

    #[test]
    fn read_entries_with_filter() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("notes.txt"), b"").unwrap();
        fs::write(dir.path().join("image.png"), b"").unwrap();
        fs::create_dir(dir.path().join("mydir")).unwrap();
        // filter: only .txt files (but dirs always pass)
        let filter: &dyn Fn(&std::path::Path) -> bool =
            &|p| p.extension().and_then(|e| e.to_str()) == Some("txt");
        let entries = read_entries(dir.path(), false, Some(filter));
        // directory always passes
        assert!(entries.iter().any(|e| e.name == "mydir"));
        // .txt passes
        assert!(entries.iter().any(|e| e.name == "notes.txt"));
        // .png filtered out
        assert!(!entries.iter().any(|e| e.name == "image.png"));
    }

    #[cfg(unix)]
    #[test]
    fn read_entries_symlink_detection() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.txt");
        fs::write(&target, b"").unwrap();
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let entries = read_entries(dir.path(), false, None);
        let sym = entries.iter().find(|e| e.name == "link.txt").unwrap();
        assert_eq!(sym.kind, EntryKind::Symlink);
    }
}
