//! Metric ID segments built from OS-provided names.

use std::collections::HashMap;

/// Lowercase `[a-z0-9_]` segment: `Macintosh HD` → `macintosh_hd`.
pub fn segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_end_matches('_').len();
    out.truncate(trimmed);
    if out.is_empty() {
        out.push_str("unknown");
    }
    out
}

/// Segments for a list of names, suffixing repeats with `_2`, `_3`, ...
pub fn segments<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    names
        .into_iter()
        .map(|name| {
            let base = segment(name);
            let n = seen.entry(base.clone()).or_default();
            *n += 1;
            if *n == 1 { base } else { format!("{base}_{n}") }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_segments() {
        assert_eq!(segment("Macintosh HD"), "macintosh_hd");
        assert_eq!(segment("PMU tdie1"), "pmu_tdie1");
        assert_eq!(segment("  --en0--  "), "en0");
        assert_eq!(segment("Ünïcode"), "n_code");
        assert_eq!(segment("..."), "unknown");
        assert_eq!(segments(["a b", "A-B", "c"]), ["a_b", "a_b_2", "c"]);
    }
}
