//! Every module file is registered with a feature that exists and is on by default.
//! A missing `modules!` line or feature would compile the module out without a word.

use std::fs;
use std::path::Path;

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

/// Feature names in Cargo.toml's `[features]` table, and the `default` list.
fn features() -> (Vec<String>, Vec<String>) {
    let text = fs::read_to_string(root().join("Cargo.toml")).unwrap_or_default();
    let table = text
        .split("\n[features]\n")
        .nth(1)
        .and_then(|t| t.split("\n[").next())
        .unwrap_or_default();
    let mut names = vec![];
    let mut default = vec![];
    for line in table.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().to_string();
        if name.starts_with('#') {
            continue;
        }
        if name == "default" {
            default = value
                .split('"')
                .skip(1)
                .step_by(2)
                .map(String::from)
                .collect();
        }
        names.push(name);
    }
    (names, default)
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
fn every_registered_feature_exists_and_is_default() {
    let (names, default) = features();
    for (feature, _) in registry() {
        assert!(
            names.contains(&feature),
            "feature `{feature}` missing from Cargo.toml"
        );
        assert!(
            default.contains(&feature),
            "feature `{feature}` not in `default`"
        );
    }
}
