use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::CompressionMethod;
use zip::unstable::write::FileOptionsExt;
use zip::write::SimpleFileOptions;

pub fn run(args: Vec<String>) -> Result<(), String> {
    let [archive, files @ ..] = args.as_slice() else {
        return Err("usage: cargo xtask bundle-zip ARCHIVE FILE...".to_owned());
    };
    if files.is_empty() {
        return Err("at least one input file is required".to_owned());
    }
    let password = std::env::var("IW4L_ARCHIVE_PASSWORD")
        .map_err(|_| "IW4L_ARCHIVE_PASSWORD is not set".to_owned())?;
    if password.is_empty() {
        return Err("IW4L_ARCHIVE_PASSWORD is empty".to_owned());
    }
    write_archive(Path::new(archive), files, password.as_bytes())
}

/// Also the in-process entry point for `cargo xtask release bundles`, which
/// knows the password without going through the environment.
pub fn write_archive(archive: &Path, files: &[String], password: &[u8]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    let inputs = files
        .iter()
        .map(PathBuf::from)
        .map(|path| {
            if !path.is_file() {
                return Err(format!("input is not a file: {}", path.display()));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("input has no UTF-8 file name: {}", path.display()))?
                .to_owned();
            if !names.insert(name.clone()) {
                return Err(format!("duplicate archive name: {name}"));
            }
            Ok((path, name))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let output =
        File::create(archive).map_err(|error| format!("create {}: {error}", archive.display()))?;
    let mut writer = zip::ZipWriter::new(output);
    for (path, name) in inputs {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o644)
            .with_deprecated_encryption(password);
        writer
            .start_file(&name, options)
            .map_err(|error| format!("start {name}: {error}"))?;
        let mut input =
            File::open(&path).map_err(|error| format!("open {}: {error}", path.display()))?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            if count == 0 {
                break;
            }
            writer
                .write_all(&buffer[..count])
                .map_err(|error| format!("write {name}: {error}"))?;
        }
    }
    writer
        .finish()
        .map_err(|error| format!("finish {}: {error}", archive.display()))?;
    Ok(())
}
