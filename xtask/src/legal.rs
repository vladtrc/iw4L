use std::path::Path;

use serde_json::{Value, json};

use crate::release::{file_sha256, file_size};
use crate::shell::Res;

pub(crate) const FILES: &[(&str, &str)] = &[
    ("LICENSE", "LICENSE"),
    ("NOTICE", "NOTICE"),
    ("crates/ui/assets/OFL-Oxanium.txt", "OFL-Oxanium.txt"),
    ("crates/console/assets/OFL-FiraMono.txt", "OFL-FiraMono.txt"),
];

pub(crate) fn copy_to(root: &Path, dest: &Path) -> Res<()> {
    for (source, name) in FILES {
        let source = root.join(source);
        let dest = dest.join(name);
        std::fs::copy(&source, &dest)
            .map_err(|error| format!("copy {} -> {}: {error}", source.display(), dest.display()))?;
    }
    Ok(())
}

pub(crate) fn inventory(dir: &Path) -> Res<Value> {
    let mut files = serde_json::Map::new();
    for (_, name) in FILES {
        let path = dir.join(name);
        files.insert(
            (*name).to_string(),
            json!({"sha256": file_sha256(&path)?, "size": file_size(&path)?}),
        );
    }
    Ok(Value::Object(files))
}

pub(crate) fn download_name(dir: &Path, name: &str) -> Res<String> {
    Ok(format!("legal-{}-{name}", file_sha256(&dir.join(name))?))
}
