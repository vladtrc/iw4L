use std::sync::Arc;

use super::LaunchIdentity;

pub(super) fn startup(
    sources: &assets::ScriptSources,
    level_data: &mut sim::script::LevelData,
    weapons: &asset_game::WeaponRegistry,
    zone: &str,
) -> sim::script::Iw4Startup {
    let level = format!("maps/{zone}");
    let mut roots = vec![
        "codescripts/delete".into(),
        "codescripts/struct".into(),
        "animscripts/init".into(),
        level.clone(),
    ];
    let aitypes: std::collections::BTreeSet<String> = level_data
        .entities
        .iter()
        .flat_map(|entity| entity.iter())
        .filter(|(key, _)| key == "classname")
        .filter_map(|(_, value)| value.strip_prefix("actor_"))
        .map(|aitype| format!("aitype/{}", aitype.to_ascii_lowercase()))
        .filter(|module| sources.read(module).is_ok())
        .collect();
    roots.extend(aitypes);
    for (weapon, script) in weapons.ai_scripts() {
        let script = script.replace('\\', "/").to_ascii_lowercase();
        let module = format!("animscripts/{script}");
        if sources.read(&module).is_ok() {
            level_data.turret_ai_scripts.insert(weapon, script);
            if !roots.contains(&module) {
                roots.push(module);
            }
        }
    }
    sim::script::Iw4Startup {
        entries: vec![format!("{level}::main")],
        roots,
    }
}

pub(super) fn level_media(
    level_data: &mut sim::script::LevelData,
    sources: &assets::ScriptSources,
    scripts: &sim::script::Program,
    xanims: &Arc<asset_anim::XAnimCatalog>,
    identity: Option<&LaunchIdentity>,
) {
    if CINEMATIC_NATIVES
        .iter()
        .any(|name| scripts.uses_native(name))
        && let Some(id) = identity
    {
        level_data.videos = video_lengths(&id.games_root);
    }
    level_data.anim = script_anim_library(sources, scripts, xanims);
}

const CINEMATIC_NATIVES: [&str; 3] = [
    "cinematicingame",
    "cinematicingamesync",
    "cinematicingameloop",
];

fn script_anim_library(
    sources: &assets::ScriptSources,
    scripts: &sim::script::Program,
    xanims: &Arc<asset_anim::XAnimCatalog>,
) -> sim::script::AnimLibrary {
    let mut trees = std::collections::BTreeMap::new();
    for (tree, names) in scripts.animations() {
        let Some(atr) = sources.config(&format!("animtrees/{tree}.atr")) else {
            diag::warn!(Sim, "gsc: animtrees/{tree}.atr is not in the zones");
            continue;
        };
        let names = names.into_iter().collect();
        match asset_anim::compile_script_tree(atr.as_bytes(), &names) {
            Ok(compiled) => {
                let nodes = compiled
                    .nodes()
                    .iter()
                    .map(|node| sim::script::AnimTreeNode {
                        name: node.name.as_str().into(),
                        parent: node.parent,
                        flags: node.flags,
                        first_child: node.first_child,
                        child_count: node.child_count,
                    })
                    .collect();
                trees.insert(tree, Arc::new(sim::script::AnimTree::new(nodes)));
            }
            Err(error) => diag::warn!(Sim, "gsc: animtrees/{tree}.atr: {error}"),
        }
    }
    let xanims = Arc::clone(xanims);
    sim::script::AnimLibrary {
        trees,
        clips: Some(Arc::new(move |name: &str| {
            xanims.clip(asset_core::AssetNamespace::Iw4, name)
        })),
    }
}

fn video_lengths(games_root: &std::path::Path) -> std::collections::BTreeMap<String, i32> {
    let mut lengths = std::collections::BTreeMap::new();
    for dir in asset_transport::video_dirs(games_root) {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("bik"))
            {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let stem = stem.to_ascii_lowercase();
            if lengths.contains_key(&stem) {
                continue;
            }
            match bink::Bink::open(&path) {
                Ok(video) => {
                    let h = video.header();
                    let ms = u64::from(h.frames) * u64::from(h.fps_den) * 1000
                        / u64::from(h.fps_num.max(1));
                    lengths.insert(stem, i32::try_from(ms).unwrap_or(i32::MAX));
                }
                Err(error) => diag::warn!(Sim, "video {}: {error}", path.display()),
            }
        }
    }
    lengths
}
