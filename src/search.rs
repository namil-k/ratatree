/// Returns a score for how well `name` matches `query`.
/// Higher is better. None means no match.
pub fn fuzzy_score(name: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let name_lower = name.to_lowercase();
    let query_lower = query.to_lowercase();
    // Exact match: 1000
    if name_lower == query_lower {
        return Some(1000);
    }
    // Prefix match: 500 + len
    if name_lower.starts_with(&query_lower) {
        return Some(500 + query.len() as i32);
    }
    // Substring match: 200 + len
    if name_lower.contains(&query_lower) {
        return Some(200 + query.len() as i32);
    }
    // Subsequence match
    let mut query_chars = query_lower.chars().peekable();
    let mut score = 0i32;
    for ch in name_lower.chars() {
        if let Some(&qch) = query_chars.peek() {
            if ch == qch {
                score += 10;
                query_chars.next();
            }
        }
    }
    if query_chars.peek().is_none() {
        Some(score)
    } else {
        None
    }
}

/// Returns indices of entries whose names match the query, sorted by score (best first).
pub fn filter_by_query(names: &[&str], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, i32)> = names
        .iter()
        .enumerate()
        .filter_map(|(i, name)| fuzzy_score(name, query).map(|s| (i, s)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
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
