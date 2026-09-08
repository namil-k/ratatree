//! Colors and modifiers for every part the picker draws.

use ratatui::style::{Color, Modifier, Style};

/// Styles for each visual element of the picker.
///
/// Pass one to [`FilePickerBuilder::theme`](crate::FilePickerBuilder::theme). [`Default`] gives a palette built from the terminal's own 16 colors, so it inherits whatever scheme the user already runs.
///
/// The styles compose rather than override each other. A row is drawn in its kind style ([`directory`](Self::directory), [`symlink`](Self::symlink) or [`normal`](Self::normal)); if it is multi-selected, [`selected`](Self::selected) is patched over that; if the cursor is on it, [`cursor`](Self::cursor) is then applied to the full row width. That last step means a `cursor` with a background covers the row's own background, so keep `cursor` and `selected` distinguishable by more than background alone.
///
/// ```
/// use ratatree::FilePickerTheme;
/// use ratatui::style::{Color, Style};
///
/// let theme = FilePickerTheme {
///     directory: Style::default().fg(Color::Magenta),
///     ..FilePickerTheme::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct FilePickerTheme {
    /// Regular files, and the base style everything else builds on.
    pub normal: Style,
    /// The row under the cursor. Applied last, across the full row width.
    pub cursor: Style,
    /// Rows toggled into the multi-selection with `Space`, including the `*` marker.
    pub selected: Style,
    /// Directory names and their trailing `/`, plus the `▾`/`▸` expansion markers in tree view.
    pub directory: Style,
    /// Symlink names and their trailing `->`.
    pub symlink: Style,
    /// The current directory path along the top.
    pub path_bar: Style,
    /// The counters along the bottom, and the `(empty)` placeholder.
    pub status_bar: Style,
    /// The query line shown while in search mode.
    pub search_input: Style,
    /// Messages such as `Circular symlink`, which replace the status bar until the next keypress.
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
