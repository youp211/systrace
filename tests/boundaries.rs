//! Layer boundaries inside the single crate, checked from source:
//! modules are plugins on top of core, front ends render core, and core needs neither.

use std::fs;
use std::path::{Path, PathBuf};

/// `crate::<first segment>` paths used by every `.rs` file under `dir`.
fn crate_paths(dir: &str) -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(dir);
    let mut files = vec![];
    collect(&root, &mut files);
    files
        .into_iter()
        .flat_map(|file| {
            let text = fs::read_to_string(&file).unwrap_or_default();
            let segments: Vec<String> = text
                .match_indices("crate::")
                .flat_map(|(i, m)| first_segments(&text[i + m.len()..]))
                .collect();
            segments.into_iter().map(move |s| (file.clone(), s))
        })
        .collect()
}

/// First path segment after `crate::`; for `crate::{a, b::c}` that is `a` and `b`.
fn first_segments(rest: &str) -> Vec<String> {
    let ident = |s: &str| -> String {
        s.trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect()
    };
    match rest.strip_prefix('{') {
        Some(group) => group
            .split('}')
            .next()
            .unwrap_or_default()
            .split(',')
            .map(ident)
            .filter(|s| !s.is_empty())
            .collect(),
        None => vec![ident(rest)],
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "rs") {
            out.push(path.to_path_buf());
        }
        return;
    }
    for entry in fs::read_dir(path).into_iter().flatten().flatten() {
        collect(&entry.path(), out);
    }
}

fn assert_only(dir: &str, allowed: &[&str]) {
    for (file, segment) in crate_paths(dir) {
        assert!(
            allowed.contains(&segment.as_str()),
            "{} uses crate::{segment}; allowed: {allowed:?}",
            file.display()
        );
    }
}

#[test]
fn modules_use_only_the_plugin_api() {
    assert_only("modules", &["prelude", "util", "platform"]);
}

#[test]
fn front_ends_never_touch_the_os_or_modules() {
    let core = [
        "display", "export", "fmt", "host", "layout", "metric", "module", "store", "util",
        "prelude",
    ];
    assert_only("tui", &core);
    assert_only("gui", &core);
}

#[test]
fn core_needs_no_modules_or_front_ends() {
    for (file, segment) in crate_paths("") {
        let in_layer = ["modules", "tui", "gui"].iter().any(|layer| {
            file.starts_with(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("src")
                    .join(layer),
            )
        });
        if !in_layer {
            assert!(
                !["modules", "tui", "gui"].contains(&segment.as_str()),
                "core file {} uses crate::{segment}",
                file.display()
            );
        }
    }
}
