use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

const IW4_ZONE_VERSION: u32 = 0x114;
use asset_core::{FamilyId, families};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamesRoot(pub PathBuf);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneFile {
    pub path: PathBuf,
    pub zone_name: String,

    pub alias_note: Option<String>,
}

static GAME_FOLDERS: std::sync::RwLock<Vec<PathBuf>> = std::sync::RwLock::new(Vec::new());

pub fn set_game_folders(folders: Vec<PathBuf>) {
    let folders = folders
        .into_iter()
        .filter(|folder| folder.join("zone").is_dir())
        .collect();
    *GAME_FOLDERS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = folders;
}

pub fn game_install_root(picked: &Path) -> PathBuf {
    picked
        .ancestors()
        .take(3)
        .find(|dir| dir.join("zone").is_dir())
        .unwrap_or(picked)
        .to_path_buf()
}

pub fn folder_holds_game(folder: &Path, game: crate::FamilyId) -> bool {
    let Some(version) = zone_version(game) else {
        return false;
    };
    let mut pending = vec![(folder.join("zone"), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                if depth < 2 {
                    pending.push((path, depth + 1));
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ff"))
                && peek_zone_version(&path) == Some(version)
            {
                return true;
            }
        }
    }
    false
}

pub fn find_game_install(root: &Path, game: crate::FamilyId) -> Option<PathBuf> {
    search_roots(root)
        .into_iter()
        .find(|folder| folder_holds_game(folder, game))
}

pub fn peek_zone_version(path: &Path) -> Option<u32> {
    read_zone_version(path).ok()
}

fn read_zone_version(path: &Path) -> Result<u32, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0u8; 12];
    use std::io::Read;
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    if !families()
        .iter()
        .any(|family| family.accepts_zone_header(&header))
    {
        return Err(format!("unrecognized zone envelope: {:02x?}", &header[..8]));
    }
    Ok(u32::from_le_bytes(header[8..12].try_into().unwrap()))
}

fn game_files(root: &Path) -> impl Iterator<Item = Result<PathBuf, String>> {
    files_under(search_roots(root))
}

const NOT_GAME_DIRS: [&str; 6] = [
    "incremental",
    "deps",
    "build",
    ".fingerprint",
    "examples",
    "iw4l-artifacts",
];

fn is_not_game_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            NOT_GAME_DIRS
                .iter()
                .any(|skip| name.eq_ignore_ascii_case(skip))
        })
}

pub fn files_under(roots: Vec<PathBuf>) -> impl Iterator<Item = Result<PathBuf, String>> {
    let mut pending: VecDeque<_> = roots.into_iter().map(Ok).collect();
    let mut visited = HashSet::new();
    std::iter::from_fn(move || {
        loop {
            let path = match pending.pop_front()? {
                Ok(path) => path,
                Err(error) => return Some(Err(error)),
            };
            if is_not_game_dir(&path) {
                continue;
            }
            let canonical = match std::fs::canonicalize(&path) {
                Ok(path) => path,
                Err(error) => {
                    return Some(Err(format!("cannot access {}: {error}", path.display())));
                }
            };
            if !visited.insert(canonical) {
                continue;
            }
            let metadata = match std::fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => return Some(Err(format!("cannot stat {}: {error}", path.display()))),
            };
            if metadata.is_file() {
                return Some(Ok(path));
            }
            if !metadata.is_dir() {
                continue;
            }
            let entries = match std::fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) => return Some(Err(format!("cannot read {}: {error}", path.display()))),
            };
            let mut children = entries
                .map(|entry| {
                    entry.map(|entry| entry.path()).map_err(|error| {
                        format!("cannot read entry in {}: {error}", path.display())
                    })
                })
                .collect::<Vec<_>>();
            children.sort();
            pending.extend(children);
        }
    })
}

pub fn zone_game_for_path(path: &Path) -> Option<FamilyId> {
    let version = peek_zone_version(path)?;
    families()
        .iter()
        .find(|family| family.zone_version() == version)
        .map(|family| family.id())
}

pub fn find_zone_file_under(search_root: &Path, zone: &str) -> Result<ZoneFile, String> {
    let mut errors = Vec::new();
    for family in families().iter() {
        match find_zone_file_version(
            &GamesRoot(search_root.to_owned()),
            zone,
            family.zone_version(),
        ) {
            Ok(found) => return Ok(found),
            Err(error) => errors.push(error),
        }
    }
    Err(format!(
        "zone {zone} not found for any registered family: {}",
        errors.join("; ")
    ))
}

pub fn find_zone_file_version(
    root: &GamesRoot,
    zone: &str,
    version: u32,
) -> Result<ZoneFile, String> {
    let file_name = format!("{zone}.ff");
    let mut found: Option<PathBuf> = None;
    let mut rejected = Vec::new();
    for entry in game_files(&root.0) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                rejected.push(error);
                continue;
            }
        };
        if !path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case(&file_name))
        {
            continue;
        }
        let actual = read_zone_version(&path);
        if actual == Ok(version) {
            found = Some(path);
            break;
        }
        rejected.push(format!(
            "{} ({})",
            path.display(),
            actual.map_or_else(|error| error, |v| format!("version {v:#x}"))
        ));
    }
    let path = found.ok_or_else(|| {
        format!(
            "zone `{file_name}` version {version:#x} not found; {}; other candidates: {}",
            games_root_report(root),
            if rejected.is_empty() {
                "none".to_owned()
            } else {
                rejected.join("; ")
            }
        )
    })?;
    Ok(ZoneFile {
        path,
        zone_name: zone.to_owned(),
        alias_note: None,
    })
}

pub fn split_zone_key(zone: &str) -> (Option<crate::FamilyId>, &str) {
    let Some((prefix, rest)) = zone.split_once(':') else {
        return (None, zone);
    };
    match FamilyId::parse(prefix) {
        Some(game) if !rest.is_empty() => (Some(game), rest),
        _ => (None, zone),
    }
}

pub fn map_load_title(key: &str, game: Option<crate::FamilyId>) -> String {
    let (parsed, stem) = split_zone_key(key);
    match parsed.or(game) {
        Some(game) => format!("{}:{stem}", game.as_str()),
        None => stem.to_owned(),
    }
}

pub fn group_mp_maps(maps: &[String]) -> BTreeMap<FamilyId, Vec<&str>> {
    let mut groups: BTreeMap<FamilyId, Vec<&str>> = BTreeMap::new();
    for map in maps {
        let family = split_zone_key(map).0.unwrap_or(FamilyId::IW4);
        groups.entry(family).or_default().push(map.as_str());
    }
    groups
}

pub fn find_zone_file(root: &GamesRoot, zone: &str) -> Result<ZoneFile, String> {
    let zone = zone.trim().to_ascii_lowercase();
    if zone.is_empty() {
        return Err("zone name is empty".into());
    }
    let (game, stem) = split_zone_key(&zone);
    find_zone_stem(root, game, stem)
}

pub fn zone_version(game: FamilyId) -> Option<u32> {
    families().get(game).map(|family| family.zone_version())
}

fn find_stem_file(
    root: &GamesRoot,
    game: Option<crate::FamilyId>,
    stem: &str,
) -> Result<ZoneFile, String> {
    match game {
        Some(game) => find_zone_file_version(
            root,
            stem,
            zone_version(game).ok_or_else(|| format!("unregistered family {}", game.as_str()))?,
        ),
        None => find_zone_file_under(&root.0, stem),
    }
}

fn find_zone_stem(
    root: &GamesRoot,
    game: Option<crate::FamilyId>,
    stem: &str,
) -> Result<ZoneFile, String> {
    if stem.starts_with("mp_") {
        return find_stem_file(root, game, stem);
    }
    let Some(prefixed) = resolve_mp_zone_alias(stem) else {
        return find_stem_file(root, game, stem);
    };
    match find_stem_file(root, game, &prefixed) {
        Ok(mut found) => {
            let note = format!(
                "zone alias: `{stem}` -> `{prefixed}` (MP stem preferred over bare `{stem}.ff`)"
            );
            found.alias_note = Some(note.clone());
            match std::fs::metadata(&found.path) {
                Ok(meta) => diag::info!(Zone, "{note} ({} bytes)", meta.len()),
                Err(_) => diag::info!(Zone, "{note}"),
            }
            Ok(found)
        }
        Err(mp_err) => match find_stem_file(root, game, stem) {
            Ok(found) => Ok(found),
            Err(bare_err) => Err(format!("{mp_err}; {bare_err}")),
        },
    }
}

pub fn resolve_mp_zone_alias(zone: &str) -> Option<String> {
    if zone.is_empty() || zone.starts_with("mp_") || zone.ends_with("_mp") {
        return None;
    }
    Some(format!("mp_{zone}"))
}

pub fn find_common_mp_for_zone(zone_ff: &Path) -> Result<ZoneFile, String> {
    find_named_zone_for_tree(zone_ff, "common_mp")
}

pub fn find_runtime_zone(root: &GamesRoot, zone_ff: &Path, zone: &str) -> Result<ZoneFile, String> {
    match zone_game_for_path(zone_ff) {
        Some(crate::FamilyId::IW4) | None => find_zone_for_tree(zone_ff, zone),
        Some(_) => {
            let found = find_zone_file_under(&root.0, zone)?;
            match peek_zone_version(&found.path) {
                Some(IW4_ZONE_VERSION) => Ok(found),
                Some(version) => Err(format!(
                    "runtime profile is IW4 FFA; {zone} at {} is version {version:#x} \
                     (want IW4 {IW4_ZONE_VERSION:#x})",
                    found.path.display()
                )),
                None => Err(format!(
                    "runtime profile is IW4 FFA; {zone} at {} is not a readable IWff envelope",
                    found.path.display()
                )),
            }
        }
    }
}

pub fn find_runtime_common_mp(root: &GamesRoot, zone_ff: &Path) -> Result<ZoneFile, String> {
    find_runtime_zone(root, zone_ff, "common_mp")
}

pub fn find_common_mp_for_envelope(root: &GamesRoot, version: u32) -> Result<ZoneFile, String> {
    find_zone_file_version(root, "common_mp", version)
}

pub fn find_localized_common_mp_for_zone(zone_ff: &Path) -> Result<ZoneFile, String> {
    find_named_zone_for_tree(zone_ff, "localized_common_mp")
}

pub fn find_zone_for_tree(zone_ff: &Path, zone: &str) -> Result<ZoneFile, String> {
    find_named_zone_for_tree(zone_ff, zone)
}

fn find_named_zone_for_tree(zone_ff: &Path, zone: &str) -> Result<ZoneFile, String> {
    let tree = game_root_for_zone(zone_ff)?;
    if let Some(version) = peek_zone_version(zone_ff)
        && let Ok(found) = find_zone_file_version(&GamesRoot(tree.clone()), zone, version)
    {
        return Ok(found);
    }
    find_zone_file_under(&tree, zone)
}

pub fn game_root_for_zone(zone_ff: &Path) -> Result<PathBuf, String> {
    let mut cursor = zone_ff.parent();
    while let Some(dir) = cursor {
        if dir.join("zone").is_dir() {
            return Ok(dir.to_path_buf());
        }
        cursor = dir.parent();
    }
    Err(format!(
        "no game tree with a `zone/` directory above {}",
        zone_ff.display()
    ))
}

pub fn games_root_report(root: &GamesRoot) -> String {
    let roots = search_roots(&root.0);
    let mut line = format!("games root: {}", root.0.display());
    for extra in roots.iter().skip(1) {
        line.push_str(&format!("; shortcut root: {}", extra.display()));
    }
    line
}

pub fn games_content_report(root: &GamesRoot) -> Vec<String> {
    #[derive(Default)]
    struct Content {
        versions: BTreeMap<u32, usize>,
        files: usize,
        dlc: usize,
        iw4_common: bool,
    }
    let mut trees: BTreeMap<PathBuf, Content> = BTreeMap::new();
    let mut report = Vec::new();
    for entry in game_files(&root.0) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                report.push(format!("asset scan: {error}"));
                continue;
            }
        };
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ff"))
        {
            continue;
        }
        let tree = game_root_for_zone(&path).unwrap_or_else(|_| root.0.clone());
        let is_dlc = path.strip_prefix(&tree).ok().is_some_and(|relative| {
            relative.parent().is_some_and(|parent| {
                parent
                    .components()
                    .any(|part| part.as_os_str().eq_ignore_ascii_case("dlc"))
            })
        });
        let content = trees.entry(tree).or_default();
        content.files += 1;
        if is_dlc {
            content.dlc += 1;
        }
        match read_zone_version(&path) {
            Ok(version) => {
                *content.versions.entry(version).or_default() += 1;
                if version == IW4_ZONE_VERSION
                    && path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("common_mp.ff"))
                {
                    content.iw4_common = true;
                }
            }
            Err(error) => report.push(format!("asset header {}: {error}", path.display())),
        }
    }
    if trees.is_empty() {
        report.push(
            "No .ff files found in the configured game folders and shortcut targets.".to_owned(),
        );
    }
    for (tree, content) in trees {
        let versions = content
            .versions
            .iter()
            .map(|(version, count)| format!("{version:#x}: {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        report.push(format!(
            "assets {}: {} FastFiles, {} in DLC folders; envelope versions {{{versions}}}",
            tree.display(),
            content.files,
            content.dlc
        ));
        if content.files == content.dlc && !content.iw4_common {
            report.push("Only DLC FastFiles were found in this tree; DLC maps do not supply the base multiplayer menu assets.".to_owned());
        }
        if content.versions.contains_key(&IW4_ZONE_VERSION) {
            report.push(format!(
                "IW4 common_mp.ff: {}",
                if content.iw4_common {
                    "found"
                } else {
                    "missing from this tree"
                }
            ));
        }
    }
    report
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapPack {
    pub label: String,
    pub maps: Vec<String>,
}

pub fn list_mp_maps(root: &GamesRoot) -> Vec<String> {
    let mut maps: Vec<String> = list_mp_map_packs(root)
        .into_iter()
        .flat_map(|pack| pack.maps)
        .collect();
    maps.sort();
    maps
}

pub fn list_mp_map_packs(root: &GamesRoot) -> Vec<MapPack> {
    let mut zones = Vec::new();
    for entry in game_files(&root.0) {
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                diag::warn!(Zone, "map discovery: {error}");
                continue;
            }
        };
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let stem = stem.to_ascii_lowercase();
        let is_ff = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ff"));
        if !(is_ff && stem.starts_with("mp_")) {
            continue;
        }
        if let Some(game) = zone_game_for_path(&path) {
            zones.push((game.as_str(), stem, map_pack_folder(&path)));
        }
    }
    let loads: HashSet<(&str, &str)> = zones
        .iter()
        .filter_map(|(game, stem, _)| stem.strip_suffix("_load").map(|map| (*game, map)))
        .collect();
    let mut packs: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for (game, stem, folder) in &zones {
        if stem.ends_with("_load")
            || !(loads.contains(&(*game, stem.as_str())) || !loads.iter().any(|(g, _)| g == game))
        {
            continue;
        }
        let map = format!("{game}:{stem}");
        if seen.insert(map.clone()) {
            packs.entry((game, folder)).or_default().push(map);
        }
    }
    packs
        .into_iter()
        .map(|((game, folder), mut maps)| {
            maps.sort();
            MapPack {
                label: format!("{} {folder}", game.to_ascii_uppercase()),
                maps,
            }
        })
        .collect()
}

fn map_pack_folder(zone_ff: &Path) -> String {
    game_root_for_zone(zone_ff)
        .ok()
        .and_then(|tree| {
            zone_ff
                .strip_prefix(tree.join("zone"))
                .ok()?
                .components()
                .next()?
                .as_os_str()
                .to_str()
                .map(str::to_ascii_uppercase)
        })
        .filter(|folder| folder.starts_with("DLC"))
        .unwrap_or_else(|| "BASE".to_owned())
}

pub fn ensure_artifacts_dir() -> Result<PathBuf, String> {
    let dir = std::env::var_os("IW4L_ARTIFACTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("iw4l-artifacts"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

pub trait GamesProvider: Send + Sync + 'static {
    fn game_root(&self) -> Result<GamesRoot, String>;
    fn search_roots(&self, root: &Path) -> Vec<PathBuf>;
}

static PROVIDER: std::sync::OnceLock<Box<dyn GamesProvider>> = std::sync::OnceLock::new();

pub fn install_games_provider(provider: impl GamesProvider) -> Result<(), &'static str> {
    PROVIDER
        .set(Box::new(provider))
        .map_err(|_| "games provider already installed")
}

pub fn games_root() -> Result<GamesRoot, String> {
    PROVIDER
        .get()
        .ok_or("builder must install a games provider")?
        .game_root()
}

pub fn search_roots(root: &Path) -> Vec<PathBuf> {
    PROVIDER
        .get()
        .expect("builder must install a games provider before discovery")
        .search_roots(root)
}

pub fn configured_search_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    for folder in GAME_FOLDERS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
    {
        if !roots.contains(folder) {
            roots.push(folder.clone());
        }
    }
    roots
}
