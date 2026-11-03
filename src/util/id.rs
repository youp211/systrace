//! Metric ID segments built from OS-provided names.

use std::collections::HashSet;

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

/// Unique segments for a list of names. A name keeps its own segment when it can;
/// repeats get the lowest `_2`, `_3`, ... that no other name sanitizes to, so
/// `Data, Data, Data 2` becomes `data, data_3, data_2`.
pub fn segments<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let bases: Vec<String> = names.into_iter().map(segment).collect();
    let reserved: HashSet<&str> = bases.iter().map(String::as_str).collect();
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(bases.len());
    for base in &bases {
        let chosen = if taken.contains(base) {
            (2..)
                .map(|n| format!("{base}_{n}"))
                .find(|c| !reserved.contains(c.as_str()) && !taken.contains(c))
                .unwrap_or_else(|| base.clone())
        } else {
            base.clone()
        };
        taken.insert(chosen.clone());
        out.push(chosen);
    }
    out
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

    #[test]
    fn repeat_suffixes_never_collide_with_real_names() {
        assert_eq!(
            segments(["Data", "Data", "Data 2"]),
            ["data", "data_3", "data_2"]
        );
        assert_eq!(segments(["x", "x", "x_2"]), ["x", "x_3", "x_2"]);
        let many = segments(["u", "u", "u", "u 2", "u_3"]);
        let unique: HashSet<_> = many.iter().collect();
        assert_eq!(unique.len(), many.len(), "{many:?}");
    }
}
