pub mod author;
pub mod evidence;
pub mod verify;

use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = std::fs::read(path)?;
    let parsed = match path.extension().and_then(|s| s.to_str()) {
        Some("yml" | "yaml") => serde_yaml_ng::from_slice(&bytes).map_err(|e| e.to_string()),
        _ => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
    };
    parsed.map_err(|e| format!("{}: {e}", path.display()).into())
}
pub fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = match path.extension().and_then(|s| s.to_str()) {
        Some("yml" | "yaml") => serde_yaml_ng::to_string(value)?.into_bytes(),
        _ => serde_json::to_vec_pretty(value)?,
    };
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Fingerprint the compiled tool itself as well as the editable source data.
/// Embedded source bytes describe this binary even when the checkout changes.
pub fn record_sources(output: &Path, paths: &[&Path]) -> Result<()> {
    use sha2::{Digest, Sha256};
    let tool_sources = [
        include_str!("lib.rs"),
        include_str!("main.rs"),
        include_str!("author.rs"),
        include_str!("verify.rs"),
        include_str!("evidence.rs"),
        include_str!("../Cargo.toml"),
        include_str!("../../../Cargo.lock"),
    ];
    let mut hash = Sha256::new();
    for source in tool_sources {
        hash.update((source.len() as u64).to_le_bytes());
        hash.update(source);
    }
    let mut files = Vec::new();
    for path in paths {
        let path = path.canonicalize()?;
        files.push(serde_json::json!({"path":path,"sha256":format!("{:x}",Sha256::digest(std::fs::read(&path)?))}));
    }
    write(
        &output.join("sources.json"),
        &serde_json::json!({"tool":"iw4l-scenario",
        "compiled_tool_sources_sha256":format!("{:x}",hash.finalize()),"files":files}),
    )
}
