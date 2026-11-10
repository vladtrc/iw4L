use crate::shell::{Res, run};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn read(path: &Path) -> Res<String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Res<()> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Rust parsing ignores comments and decodes ordinary, escaped and raw literals.
fn foreign_literal(source: &str) -> Res<bool> {
    use syn::visit::Visit;
    struct ContentKeys(bool);
    impl<'ast> Visit<'ast> for ContentKeys {
        fn visit_lit_str(&mut self, literal: &'ast syn::LitStr) {
            let value = literal.value();
            self.0 |= ["iw5:", "t5:", "t6:"]
                .iter()
                .any(|prefix| value.starts_with(prefix));
        }
        fn visit_macro(&mut self, node: &'ast syn::Macro) {
            // Macro arguments are token trees, not parsed expressions.
            fn literals(tokens: proc_macro2::TokenStream, found: &mut bool) {
                for token in tokens {
                    match token {
                        proc_macro2::TokenTree::Group(group) => literals(group.stream(), found),
                        proc_macro2::TokenTree::Literal(literal) => {
                            if let Ok(value) = syn::parse_str::<syn::LitStr>(&literal.to_string()) {
                                *found |= ["iw5:", "t5:", "t6:"]
                                    .iter()
                                    .any(|prefix| value.value().starts_with(prefix));
                            }
                        }
                        _ => {}
                    }
                }
            }
            literals(node.tokens.clone(), &mut self.0);
        }
    }
    let syntax = syn::parse_file(source).map_err(|e| format!("Rust source: {e}"))?;
    let mut visitor = ContentKeys(false);
    visitor.visit_file(&syntax);
    Ok(visitor.0)
}

pub fn check_boundaries(root: &Path) -> Res<()> {
    let core = root.join("crates");
    let mut files = Vec::new();
    rust_files(&core, &mut files)?;
    files.sort();
    let mut violations = Vec::new();
    for path in files {
        if foreign_literal(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))? {
            violations.push(path.display().to_string());
        }
    }
    if !violations.is_empty() {
        return Err(format!(
            "core names non-reference content: {}",
            violations.join(", ")
        ));
    }
    println!("core content boundary: ok");
    Ok(())
}

pub fn check_workspace(root: &Path, args: &[String]) -> Res<()> {
    if args.iter().any(|a| a != "--structure-only") {
        return Err("usage: cargo xtask check-workspace [--structure-only]".into());
    }
    check_boundaries(root)?;
    let metadata = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| e.to_string())?;
    if !metadata.status.success() {
        return Err(String::from_utf8_lossy(&metadata.stderr).into_owned());
    }
    let data: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).map_err(|e| e.to_string())?;
    let packages = data["packages"]
        .as_array()
        .ok_or("metadata packages missing")?;
    for package in packages {
        let name = package["name"].as_str().ok_or("package name missing")?;
        let manifest = Path::new(
            package["manifest_path"]
                .as_str()
                .ok_or("manifest path missing")?,
        );
        if !manifest.starts_with(root) {
            return Err(format!("{name} leaves the checkout"));
        }
        for dep in package["dependencies"]
            .as_array()
            .ok_or("dependencies missing")?
        {
            if let Some(path) = dep["path"].as_str() {
                let resolved = fs::canonicalize(path).map_err(|e| e.to_string())?;
                if !resolved.starts_with(root) {
                    return Err(format!("{name} dependency leaves checkout: {path}"));
                }
            }
            let dependency = dep["name"].as_str().unwrap_or("");
            if dependency.starts_with("ext_") {
                let owner = name.strip_prefix("ext_").and_then(|s| s.split('_').next());
                let target = dependency
                    .strip_prefix("ext_")
                    .and_then(|s| s.split('_').next());
                if manifest.starts_with(root.join("crates")) || (owner.is_some() && owner != target)
                {
                    return Err(format!(
                        "invalid extension dependency: {name} -> {dependency}"
                    ));
                }
                if owner.is_some()
                    && name != format!("ext_{}", owner.unwrap())
                    && (dependency == format!("ext_{}", owner.unwrap())
                        || dependency.ends_with("_load"))
                {
                    return Err(format!(
                        "lower extension depends on composition: {name} -> {dependency}"
                    ));
                }
            }
        }
    }
    println!("workspace paths and extension dependency boundaries: ok");
    if !args.is_empty() {
        return Ok(());
    }
    for action in ["build", "clippy"] {
        run(Command::new("cargo").current_dir(root).args([
            action,
            "--workspace",
            "--offline",
            "--locked",
        ]))?;
    }
    Ok(())
}
