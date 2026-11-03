//! Every module file is registered, and the `modules` feature, which `default`
//! turns on, lists exactly the registered modules and no front end. A missing
//! `modules!` line or feature would compile the module out without a word.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `(feature, file)` for each `"feature" => file::Type [if "os"];` line.
fn registry() -> Vec<(String, String)> {
    let text = fs::read_to_string(root().join("src/modules/mod.rs")).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix('"')?;
            let (feature, rest) = rest.split_once('"')?;
            let (file, _) = rest.trim().strip_prefix("=>")?.trim().split_once("::")?;
            Some((feature.to_string(), file.to_string()))
        })
        .collect()
}

/// Each feature and what it lists, as cargo itself reads Cargo.toml.
fn features() -> BTreeMap<String, BTreeSet<String>> {
    // `--no-deps` reads only this package, so it never touches the network.
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version=1"])
        .arg("--manifest-path")
        .arg(root().join("Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("cargo metadata prints JSON");
    serde_json::from_value(metadata["packages"][0]["features"].clone())
        .expect("features map names to lists")
}

/// Everything `feature` turns on, directly or through other features.
fn turned_on(features: &BTreeMap<String, BTreeSet<String>>, feature: &str) -> BTreeSet<String> {
    let mut on = BTreeSet::new();
    let mut todo = vec![feature.to_string()];
    while let Some(f) = todo.pop() {
        if on.insert(f.clone()) {
            todo.extend(features.get(&f).into_iter().flatten().cloned());
        }
    }
    on
}

#[test]
fn every_module_file_is_registered() {
    let registered = registry();
    assert!(!registered.is_empty(), "no modules! lines found");
    let files = fs::read_dir(root().join("src/modules"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".rs").map(String::from)
        })
        .filter(|stem| stem != "mod");
    for stem in files {
        assert!(
            registered
                .iter()
                .any(|(feature, file)| file == &stem && feature == &stem),
            "src/modules/{stem}.rs has no `\"{stem}\" => {stem}::…` line in modules!"
        );
    }
}

#[test]
fn modules_lists_exactly_the_registered_modules() {
    let features = features();
    let listed = |name: &str| features.get(name).cloned().unwrap_or_default();
    assert!(
        listed("default").contains("modules"),
        "`modules` not in `default`"
    );
    let registered: BTreeSet<String> = registry().into_iter().map(|(f, _)| f).collect();
    assert_eq!(listed("modules"), registered, "`modules` vs modules! lines");
}

/// Built alone, `modules` is the CLI with every module and no front end.
#[test]
fn modules_turn_on_no_front_end() {
    let on = turned_on(&features(), "modules");
    for front in ["tui", "gui"] {
        assert!(!on.contains(front), "`modules` turns on `{front}`");
    }
}
