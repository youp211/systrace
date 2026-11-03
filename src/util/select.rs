//! Metric selector matching.

/// Selector match: exact, subtree (`cpu`), or `*` wildcard (`cpu.core.*.util`).
pub fn matches(pattern: &str, id: &str) -> bool {
    if pattern.contains('*') {
        glob(pattern.as_bytes(), id.as_bytes())
    } else {
        in_subtree(pattern, id)
    }
}

/// True when no selectors are given or any one matches.
pub fn matches_any(patterns: &[String], id: &str) -> bool {
    patterns.is_empty() || patterns.iter().any(|p| matches(p, id))
}

/// `root` itself or anything under `root.`: `cpu` covers `cpu.util`, not `cpux.util`.
pub fn in_subtree(root: &str, id: &str) -> bool {
    id.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// `*` matches any run of characters, dots included. Iterative, so a pattern with
/// many stars costs O(pattern × id) rather than exponential backtracking.
fn glob(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si) = (0, 0);
    // Where to resume after the most recent star: (pattern index, id index).
    let mut resume: Option<(usize, usize)> = None;
    while si < s.len() {
        match p.get(pi) {
            Some(b'*') => {
                pi += 1;
                resume = Some((pi, si));
            }
            Some(&c) if c == s[si] => {
                pi += 1;
                si += 1;
            }
            _ => match resume {
                // Let the last star swallow one more character and retry.
                Some((rp, rs)) => {
                    pi = rp;
                    si = rs + 1;
                    resume = Some((rp, rs + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn matches_selectors() {
        assert!(matches("cpu", "cpu"));
        assert!(matches("cpu", "cpu.core.0.util"));
        assert!(!matches("cpu", "cpux.util"));
        assert!(matches("cpu.core.*.util", "cpu.core.12.util"));
        assert!(!matches("cpu.core.*.util", "cpu.core.12.freq"));
        assert!(matches("*.util", "cpu.util"));
        assert!(matches("*", ""));
        assert!(matches("a*b*c", "aXbYc"));
        assert!(!matches("a*b*c", "aXbY"));
        assert!(matches("**a", "a"));
        assert!(!matches("a*", "b"));
        assert!(matches_any(&[], "anything"));
    }

    #[test]
    fn many_stars_stay_fast() {
        let pattern = format!("{}#", "*".repeat(20));
        let id = "cpu.core.15.util.and.a.long.tail.of.text";
        let start = Instant::now();
        assert!(!matches(&pattern, id));
        assert!(start.elapsed() < Duration::from_millis(50));
    }
}
