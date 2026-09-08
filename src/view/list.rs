//! List view helpers.

use super::ListViewState;

impl ListViewState {
    /// The range of entry indices that fit on screen, given `total` entries and a pane `height` rows tall. Clamped to `total`, so a short listing yields a short range.
    pub fn visible_range(&self, total: usize, height: usize) -> std::ops::Range<usize> {
        let start = self.scroll_offset;
        let end = (start + height).min(total);
        start..end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_range_basic() {
        let state = ListViewState {
            cursor: 0,
            scroll_offset: 0,
        };
        let range = state.visible_range(20, 10);
        assert_eq!(range, 0..10);
    }

    #[test]
    fn visible_range_scrolled() {
        let state = ListViewState {
            cursor: 15,
            scroll_offset: 10,
        };
        let range = state.visible_range(20, 10);
        assert_eq!(range, 10..20);
    }

    #[test]
    fn visible_range_at_end() {
        let state = ListViewState {
            cursor: 18,
            scroll_offset: 15,
        };
        let range = state.visible_range(20, 10);
        assert_eq!(range, 15..20);
    }
}
