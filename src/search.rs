//! Ranking names against a search query.
//!
//! A score is a tier plus a bonus. The tier says how the query matched at all: matching the whole name beats matching its start, which beats matching somewhere inside it, which beats merely having the letters in order. The bonus orders names within one tier and is capped well below the gap between tiers, so a better bonus can never lift a name past a name in a higher tier.

/// Tier floors. The gap between any two is wider than every bonus added together, which is what keeps the tiers absolute.
const TIER_EXACT: i32 = 4000;
const TIER_PREFIX: i32 = 3000;
const TIER_SUBSTRING: i32 = 2000;
const TIER_SUBSEQUENCE: i32 = 1000;

/// Ceiling for any single bonus component.
const BONUS_CAP: usize = 100;

/// Rewards shorter names, on the assumption that a query matching a short name matched most of it.
fn shortness_bonus(name: &str) -> i32 {
    (BONUS_CAP - name.chars().count().min(BONUS_CAP)) as i32
}

/// Rewards a match that starts near the front of the name.
fn earliness_bonus(start: usize) -> i32 {
    (BONUS_CAP - start.min(BONUS_CAP)) as i32
}

/// Rewards a match that starts where a reader would see a new word beginning: at the front of the name, or just after a separator.
fn word_boundary_bonus(name: &str, start: usize) -> i32 {
    if start == 0 {
        return BONUS_CAP as i32;
    }
    match name.chars().nth(start - 1) {
        Some('_') | Some('-') | Some('.') | Some(' ') | Some('/') => BONUS_CAP as i32,
        _ => 0,
    }
}

/// Rewards matched letters that sit close together. Full marks when the run is unbroken, tailing off as the gaps grow.
fn tightness_bonus(matched: usize, span: usize) -> i32 {
    if span == 0 {
        return BONUS_CAP as i32;
    }
    (BONUS_CAP * matched / span) as i32
}

/// Returns a score for how well `name` matches `query`. Higher is better. `None` means no match.
///
/// An empty query matches everything with the same score, leaving the caller's original order intact.
pub fn fuzzy_score(name: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();

    if name_lower == query_lower {
        return Some(TIER_EXACT);
    }

    if name_lower.starts_with(&query_lower) {
        return Some(TIER_PREFIX + shortness_bonus(&name_lower));
    }

    if let Some(pos) = name_lower.find(&query_lower) {
        let start = name_lower[..pos].chars().count();
        return Some(
            TIER_SUBSTRING
                + shortness_bonus(&name_lower)
                + earliness_bonus(start)
                + word_boundary_bonus(&name_lower, start),
        );
    }

    let mut query_chars = query_lower.chars().peekable();
    let mut matched = 0usize;
    let mut first = None;
    let mut last = 0usize;
    for (idx, ch) in name_lower.chars().enumerate() {
        if let Some(&qch) = query_chars.peek() {
            if ch == qch {
                matched += 1;
                first.get_or_insert(idx);
                last = idx;
                query_chars.next();
            }
        }
    }
    if query_chars.peek().is_some() {
        return None;
    }
    let start = first.unwrap_or(0);
    let span = last - start + 1;
    Some(
        TIER_SUBSEQUENCE
            + tightness_bonus(matched, span)
            + shortness_bonus(&name_lower)
            + earliness_bonus(start)
            + word_boundary_bonus(&name_lower, start),
    )
}

/// Returns indices of entries whose names match the query, best first. Ties keep the caller's original order.
pub fn filter_by_query(names: &[&str], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, i32)> = names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| fuzzy_score(name, query).map(|s| (i, s)))
        .collect();
    scored.sort_by_key(|a| std::cmp::Reverse(a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match_scores_highest() {
        let exact = fuzzy_score("lib.rs", "lib.rs").unwrap();
        let partial = fuzzy_score("lib.rs", "lib").unwrap();
        assert!(exact > partial);
    }

    #[test]
    fn substring_match() {
        assert!(fuzzy_score("my_module.rs", "module").is_some());
    }

    #[test]
    fn no_match_returns_none() {
        assert!(fuzzy_score("lib.rs", "xyz").is_none());
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("Cargo.toml", "cargo").is_some());
    }

    #[test]
    fn subsequence_match() {
        // "l" and "r" appear in order in "lib.rs"
        assert!(fuzzy_score("lib.rs", "lr").is_some());
    }

    /// Names in the order the picker would show them for this query.
    fn ranked<'a>(names: &[&'a str], query: &str) -> Vec<&'a str> {
        filter_by_query(names, query)
            .into_iter()
            .map(|i| names[i])
            .collect()
    }

    #[test]
    fn shorter_name_outranks_longer_one_in_the_same_tier() {
        // Both are substring matches at the same offset, so length is the only
        // thing left to separate them.
        let names = ["x_testxxxxxxxxxx.rs", "x_test.rs"];
        assert_eq!(ranked(&names, "test"), ["x_test.rs", "x_testxxxxxxxxxx.rs"]);
    }

    #[test]
    fn earlier_match_outranks_later_one_in_the_same_tier() {
        // Same length and same tier, so the match offset is the only difference.
        let names = ["utils_a_test.rs", "a_test_utils.rs"];
        assert_eq!(
            ranked(&names, "test"),
            ["a_test_utils.rs", "utils_a_test.rs"]
        );
    }

    #[test]
    fn tightly_grouped_subsequence_outranks_a_scattered_one() {
        // Neither name contains "abc" outright, and both are the same length, so
        // how far apart the matched letters sit is the only difference.
        let names = ["a_xxxx_b_c", "a_b_c_xxxx"];
        assert_eq!(ranked(&names, "abc"), ["a_b_c_xxxx", "a_xxxx_b_c"]);
    }

    #[test]
    fn a_substring_match_always_outranks_a_subsequence_match() {
        // The tiers are absolute: bonuses order names within a tier and must
        // never be large enough to lift one tier above another. Worst possible
        // substring match (very long, matching right at the end) against the
        // best possible subsequence match (short, starting at the front).
        let buried = format!("{}abc", "x".repeat(100));
        let names = ["a_b_c", buried.as_str()];
        assert_eq!(ranked(&names, "abc"), [buried.as_str(), "a_b_c"]);
    }

    #[test]
    fn a_match_starting_at_a_word_boundary_outranks_one_buried_mid_word() {
        // Same length, same offset, same tier. In "my_test.rs" the query starts a
        // word; in "latest_ab.rs" it straddles the middle of one.
        let names = ["latest_ab.rs", "my_test_a.rs"];
        assert_eq!(ranked(&names, "test"), ["my_test_a.rs", "latest_ab.rs"]);
    }

    #[test]
    fn bonuses_cannot_bridge_the_gap_between_tiers() {
        // Guards the tier spacing against a future bonus being added or widened.
        // Four components, each capped at BONUS_CAP.
        let max_bonus = 4 * BONUS_CAP as i32;
        assert!(max_bonus < TIER_SUBSTRING - TIER_SUBSEQUENCE);
        assert!(max_bonus < TIER_PREFIX - TIER_SUBSTRING);
        assert!(max_bonus < TIER_EXACT - TIER_PREFIX);
    }

    #[test]
    fn filter_entries_by_query() {
        let names = ["lib.rs", "main.rs", "Cargo.toml", "README.md"];
        let result = filter_by_query(&names, "rs");
        // Should contain indices 0 (lib.rs) and 1 (main.rs)
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(!result.contains(&2));
        assert!(!result.contains(&3));
    }
}
