use asset_core::FamilyId;
use asset_transport::discover::files_under;
use asset_transport::{ZoneFile, game_root_for_zone, zone_game_for_path};
use std::path::{Path, PathBuf};
pub fn find_t5_localized_zones(
    anchor: &Path,
    language: Option<&str>,
) -> Result<Vec<ZoneFile>, String> {
    let (dir, prefix) = language_archive(anchor, language, FamilyId::new("t5"))?;
    Ok(["code_post_gfx_mp", "common_mp", "ui_mp"]
        .into_iter()
        .filter_map(|stem| localized_zone(&dir, &prefix, stem))
        .collect())
}

pub fn find_t5_localized_zone(
    anchor: &Path,
    language: Option<&str>,
    stem: &str,
) -> Result<Option<ZoneFile>, String> {
    let (dir, prefix) = language_archive(anchor, language, FamilyId::new("t5"))?;
    Ok(localized_zone(&dir, &prefix, stem))
}

fn localized_zone(dir: &Path, prefix: &str, stem: &str) -> Option<ZoneFile> {
    let zone_name = format!("{prefix}{stem}");
    let path = dir.join(format!("{zone_name}.ff"));
    path.is_file().then_some(ZoneFile {
        path,
        zone_name,
        alias_note: None,
    })
}

fn language_archive(
    anchor: &Path,
    language: Option<&str>,
    game: FamilyId,
) -> Result<(PathBuf, String), String> {
    let root = game_root_for_zone(anchor)?.join("zone");
    let mut choices: Vec<_> = files_under(vec![root])
        .filter_map(Result::ok)
        .filter(|path| {
            path.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.ends_with("_code_post_gfx_mp.ff"))
                && zone_game_for_path(path) == Some(game)
        })
        .collect();
    choices.sort();
    let preferred = choices.iter().position(|path| {
        path.parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .zip(language)
            .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
    });
    let chosen = match preferred {
        Some(index) => &choices[index],
        None if choices.len() == 1 => &choices[0],
        _ => {
            return Err(format!(
                "{} language archive selection is ambiguous or missing: {} candidates",
                game.as_str().to_uppercase(),
                choices.len()
            ));
        }
    };
    let name = chosen
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("language archive name")?;
    let prefix = name
        .strip_suffix("code_post_gfx_mp.ff")
        .ok_or("language prefix")?;
    let dir = chosen.parent().ok_or("language directory")?;
    Ok((dir.to_path_buf(), prefix.to_owned()))
}
