//! Metric selector matching.

/// Selector match: exact, subtree (`cpu`), or `*` wildcard (`cpu.core.*.util`).
pub fn matches(pattern: &str, id: &str) -> bool {
    if pattern.contains('*') {
        return glob(pattern.as_bytes(), id.as_bytes());
    }
    id == pattern
        || id
            .strip_prefix(pattern)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// True when no selectors are given or any one matches.
pub fn matches_any(patterns: &[String], id: &str) -> bool {
    patterns.is_empty() || patterns.iter().any(|p| matches(p, id))
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some((b'*', rest)) => (0..=s.len()).any(|i| glob(rest, &s[i..])),
        Some((c, rest)) => s.first() == Some(c) && glob(rest, &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_selectors() {
        assert!(matches("cpu", "cpu"));
        assert!(matches("cpu", "cpu.core.0.util"));
        assert!(!matches("cpu", "cpux.util"));
        assert!(matches("cpu.core.*.util", "cpu.core.12.util"));
        assert!(!matches("cpu.core.*.util", "cpu.core.12.freq"));
        assert!(matches("*.util", "cpu.util"));
        assert!(matches_any(&[], "anything"));
    }
}
