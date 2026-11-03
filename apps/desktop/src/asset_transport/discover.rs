use asset_transport::GamesRoot;
use std::path::{Path, PathBuf};
pub fn load_dotenv() {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            let _ = dotenvy::from_path(&candidate);
            return;
        }
    }
    let Ok(mut dir) = std::env::current_dir() else {
        return;
    };
    loop {
        let candidate = dir.join(".env");
        if candidate.is_file() {
            let _ = dotenvy::from_path(&candidate);
            return;
        }
        if !dir.pop() {
            return;
        }
    }
}

pub fn games_root_from_env() -> Result<GamesRoot, String> {
    load_dotenv();
    let path = match std::env::var_os("IW4L_GAMES") {
        Some(raw) => PathBuf::from(raw),
        None => default_games_root()?,
    };
    if !path.is_dir() {
        return Err(format!("IW4L_GAMES is not a directory: {}", path.display()));
    }
    Ok(GamesRoot(path))
}

#[cfg(windows)]
fn default_games_root() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .ok_or_else(|| {
            "IW4L_GAMES is unset and the portable launcher directory is unavailable".to_owned()
        })
}

#[cfg(not(windows))]
fn default_games_root() -> Result<PathBuf, String> {
    Err("IW4L_GAMES is not set — copy .env.example to .env and set the games root".to_owned())
}

const SIBLING_TITLES: [&str; 3] = [
    "Call of Duty Black Ops",
    "Call of Duty Black Ops II",
    "Call of Duty Modern Warfare 3",
];

pub fn search_roots(root: &Path) -> Vec<PathBuf> {
    let roots = configured_search_roots(root);
    #[cfg(not(windows))]
    let mut roots = roots;
    #[cfg(not(windows))]
    for installed in super::steam::installed_game_roots(&roots) {
        if !roots.contains(&installed) {
            roots.push(installed);
        }
    }
    roots
}

pub(crate) fn configured_search_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = asset_transport::discover::configured_search_roots(root);
    if let Some(library) = root.parent() {
        for title in SIBLING_TITLES {
            let sibling = library.join(title);
            if sibling != root && sibling.join("zone").is_dir() && !roots.contains(&sibling) {
                roots.push(sibling);
            }
        }
    }
    #[cfg(windows)]
    {
        let Ok(entries) = std::fs::read_dir(root) else {
            return roots;
        };
        let mut links = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
            })
            .collect::<Vec<_>>();
        links.sort();
        for link_path in links {
            match shortcut_target_root(&link_path) {
                Ok(target) if !roots.contains(&target) => roots.push(target),
                Ok(_) => {}
                Err(error) => diag::warn!(
                    Zone,
                    "game shortcut {} ignored: {error}",
                    link_path.display()
                ),
            }
        }
    }
    roots
}

#[cfg(windows)]
fn shortcut_target_root(link_path: &Path) -> Result<PathBuf, String> {
    let link = lnk::ShellLink::open(link_path, lnk::encoding::WINDOWS_1252)
        .map_err(|error| format!("cannot parse .lnk: {error}"))?;
    let info = link
        .link_info()
        .as_ref()
        .ok_or_else(|| ".lnk has no LinkInfo filesystem target".to_owned())?;
    if !info.link_info_flags().has_volume_id_and_local_base_path() {
        return Err(".lnk target is not on a local filesystem".to_owned());
    }
    let mut target = info
        .local_base_path_unicode()
        .as_deref()
        .or_else(|| info.local_base_path())
        .map(PathBuf::from)
        .ok_or_else(|| ".lnk has no local base path".to_owned())?;
    let suffix = info
        .common_path_suffix_unicode()
        .as_deref()
        .unwrap_or_else(|| info.common_path_suffix());
    if !suffix.is_empty() {
        target.push(suffix);
    }
    game_root_for_shortcut_target(&target)
}

#[cfg(windows)]
fn game_root_for_shortcut_target(target: &Path) -> Result<PathBuf, String> {
    if target.is_dir() {
        return Ok(target.to_path_buf());
    }
    if target.is_file()
        && let Some(parent) = target.parent()
    {
        return Ok(parent.to_path_buf());
    }
    Err(format!("target does not exist: {}", target.display()))
}

pub struct DesktopGamesProvider;
impl asset_transport::GamesProvider for DesktopGamesProvider {
    fn game_root(&self) -> Result<GamesRoot, String> {
        games_root_from_env()
    }
    fn search_roots(&self, root: &Path) -> Vec<PathBuf> {
        search_roots(root)
    }
}
