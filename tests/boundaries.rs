//! Layer boundaries inside the single crate, checked by parsing the source:
//! modules are plugins on top of core, front ends render core, core needs neither,
//! and the CLI binary wires them together without touching the OS itself.

use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{UseTree, parse_file};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Layer {
    Core,
    Modules,
    Tui,
    Gui,
    Bin,
}

impl Layer {
    /// Whether code in this layer may reach the top-level item `target`.
    fn allows(self, target: &str) -> bool {
        match self {
            Self::Modules => ["prelude", "util", "platform", "modules"].contains(&target),
            Self::Tui => !["platform", "modules", "gui"].contains(&target),
            Self::Gui => !["platform", "modules", "tui"].contains(&target),
            Self::Core => !["modules", "tui", "gui"].contains(&target),
            Self::Bin => target != "platform",
        }
    }
}

/// Top-level items one file reaches through `crate::`, `super::`, `self::` (or
/// `systrace::` from the binary), plus anything that would dodge the check.
struct Reach {
    /// Module path of the file, e.g. `["modules", "cpu"]`.
    file: Vec<String>,
    /// Inline modules currently being visited inside the file.
    inline: Vec<String>,
    bin: bool,
    targets: Vec<String>,
    problems: Vec<String>,
}

impl Reach {
    /// First segment, from the crate root, of the module a path starts in.
    fn resolve(&self, segments: &[String]) -> Option<String> {
        let first = segments.first()?.as_str();
        if first == "crate" || (self.bin && first == "systrace") {
            return segments.get(1).cloned();
        }
        if first != "super" && first != "self" {
            return None;
        }
        let mut path: Vec<String> = self.file.iter().chain(&self.inline).cloned().collect();
        let mut rest = segments;
        while let Some(s) = rest.first() {
            match s.as_str() {
                "super" => {
                    path.pop();
                }
                "self" => {}
                _ => break,
            }
            rest = &rest[1..];
        }
        path.extend(rest.iter().cloned());
        path.first().cloned()
    }

    fn record(&mut self, segments: &[String]) {
        if let Some(target) = self.resolve(segments) {
            self.targets.push(target);
        }
    }

    /// Paths inside macro bodies (`format!`, `vec!`, `modules!`): best-effort token scan.
    fn scan_tokens(&mut self, tokens: TokenStream) {
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                TokenTree::Group(g) => self.scan_tokens(g.stream()),
                TokenTree::Ident(id)
                    if ["crate", "super", "self", "systrace"]
                        .contains(&id.to_string().as_str()) =>
                {
                    let mut segments = vec![id.to_string()];
                    let mut j = i + 1;
                    while let (
                        Some(TokenTree::Punct(a)),
                        Some(TokenTree::Punct(b)),
                        Some(TokenTree::Ident(next)),
                    ) = (tokens.get(j), tokens.get(j + 1), tokens.get(j + 2))
                    {
                        if a.as_char() != ':' || b.as_char() != ':' {
                            break;
                        }
                        segments.push(next.to_string());
                        j += 3;
                    }
                    if segments.len() > 1 {
                        self.record(&segments);
                    }
                    i = j - 1;
                }
                _ => {}
            }
            i += 1;
        }
    }
}

fn flatten(tree: &UseTree, prefix: Vec<String>, out: &mut Vec<(Vec<String>, bool)>) {
    match tree {
        UseTree::Path(p) => {
            let mut next = prefix;
            next.push(p.ident.to_string());
            flatten(&p.tree, next, out);
        }
        UseTree::Name(n) => {
            let mut next = prefix;
            next.push(n.ident.to_string());
            out.push((next, false));
        }
        UseTree::Rename(r) => {
            let mut next = prefix;
            next.push(r.ident.to_string());
            out.push((next, true));
        }
        UseTree::Glob(_) => out.push((prefix, false)),
        UseTree::Group(g) => {
            for item in &g.items {
                flatten(item, prefix.clone(), out);
            }
        }
    }
}

impl<'ast> Visit<'ast> for Reach {
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        let mut paths = Vec::new();
        flatten(&item.tree, vec![], &mut paths);
        for (segments, renamed) in paths {
            // `use crate as c;` or `use super::super as root;` would hide what is reached.
            if renamed
                && segments
                    .iter()
                    .all(|s| ["crate", "super", "self"].contains(&s.as_str()))
            {
                self.problems
                    .push(format!("aliases the crate root: {}", segments.join("::")));
            }
            self.record(&segments);
        }
    }

    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if item.ident == "self" {
            self.problems
                .push("aliases the crate with `extern crate self`".into());
        }
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if item.attrs.iter().any(|a| a.path().is_ident("path")) {
            self.problems
                .push(format!("`#[path]` on mod {}", item.ident));
        }
        if item.content.is_some() {
            self.inline.push(item.ident.to_string());
            visit::visit_item_mod(self, item);
            self.inline.pop();
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        self.record(&segments);
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.scan_tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

/// Rule violations in one file's source.
fn violations(file: &[&str], layer: Layer, source: &str) -> Vec<String> {
    let ast = match parse_file(source) {
        Ok(ast) => ast,
        Err(e) => return vec![format!("does not parse: {e}")],
    };
    let mut reach = Reach {
        file: file.iter().map(|s| s.to_string()).collect(),
        inline: vec![],
        bin: layer == Layer::Bin,
        targets: vec![],
        problems: vec![],
    };
    reach.visit_file(&ast);
    let mut out = reach.problems;
    for target in reach.targets {
        if !layer.allows(&target) {
            out.push(format!("{layer:?} code reaches `{target}`"));
        }
    }
    out
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
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

/// `src/modules/cpu.rs` → `["modules", "cpu"]`; `mod.rs` and `lib.rs` name their parent.
fn module_path(file: &Path) -> Vec<String> {
    let rel = file.strip_prefix(src()).unwrap_or(file);
    let mut parts: Vec<String> = rel
        .iter()
        .map(|p| p.to_string_lossy().trim_end_matches(".rs").to_string())
        .collect();
    if parts.last().is_some_and(|p| p == "mod" || p == "lib") {
        parts.pop();
    }
    parts
}

/// Files the binary declares (`main.rs` and its `mod` items) rather than the library.
fn bin_files() -> Vec<String> {
    let main = fs::read_to_string(src().join("main.rs")).unwrap_or_default();
    let mut files = vec!["main".to_string()];
    if let Ok(ast) = parse_file(&main) {
        for item in ast.items {
            if let syn::Item::Mod(m) = item
                && m.content.is_none()
            {
                files.push(m.ident.to_string());
            }
        }
    }
    files
}

fn layer_of(path: &[String], bin: &[String]) -> Layer {
    match path.first().map(String::as_str) {
        Some("modules") => Layer::Modules,
        Some("tui") => Layer::Tui,
        Some("gui") => Layer::Gui,
        Some(name) if path.len() == 1 && bin.iter().any(|b| b == name) => Layer::Bin,
        _ => Layer::Core,
    }
}

#[test]
fn source_respects_layers() {
    let mut files = vec![];
    collect(&src(), &mut files);
    let bin = bin_files();
    let mut seen = vec![];
    let mut problems = vec![];
    for file in &files {
        let path = module_path(file);
        let layer = layer_of(&path, &bin);
        seen.push(layer);
        let source = fs::read_to_string(file).unwrap_or_default();
        let parts: Vec<&str> = path.iter().map(String::as_str).collect();
        for v in violations(&parts, layer, &source) {
            problems.push(format!("{}: {v}", file.display()));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    for layer in [
        Layer::Core,
        Layer::Modules,
        Layer::Tui,
        Layer::Gui,
        Layer::Bin,
    ] {
        assert!(seen.contains(&layer), "no file found for {layer:?}");
    }
}

#[test]
fn checker_catches_known_bypasses() {
    let module = ["modules", "cpu"];
    let bad = [
        "use super::super::host::Host;",
        "use crate as root; fn f() { root::host::Host::new(vec![]); }",
        "fn f() { crate::store::Store::default(); }",
        "fn f() { let _ = format!(\"{:?}\", crate::host::Host::new(vec![])); }",
        "extern crate self as me;",
        "#[path = \"../host.rs\"] mod sneaky;",
    ];
    for source in bad {
        assert!(
            !violations(&module, Layer::Modules, source).is_empty(),
            "missed: {source}"
        );
    }
    assert!(!violations(&["host"], Layer::Core, "use super::modules::all;").is_empty());
    assert!(!violations(&["gui"], Layer::Gui, "use crate::platform::macos;").is_empty());
    assert!(!violations(&["commands"], Layer::Bin, "use systrace::platform::macos;").is_empty());
}

#[test]
fn checker_accepts_legitimate_code() {
    let module = ["modules", "cpu"];
    let good = [
        "use crate::{prelude::*, util::{id, rate::Rates}};",
        "use super::*;",
        "mod tests { use super::*; use crate::prelude::Level; }",
        "/// Mentions crate::host in a doc comment.\nfn f() {}",
        "fn f() { let s = \"crate::host\"; }",
    ];
    for source in good {
        let found = violations(&module, Layer::Modules, source);
        assert!(found.is_empty(), "{source}: {found:?}");
    }
    assert!(violations(&["commands"], Layer::Bin, "use systrace::modules::all;").is_empty());
}
