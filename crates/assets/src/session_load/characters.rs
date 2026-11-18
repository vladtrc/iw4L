use super::*;
use std::collections::{BTreeMap, BTreeSet};

type CharacterPerformances = BTreeMap<(u32, Option<usize>), Arc<asset_anim::NativePerformance>>;

/// Capture only explicitly requested models. Donor scripts, worlds, collision,
/// weapons and campaign animation trees never enter the multiplayer match.
pub(super) fn capture(
    characters: &[crate::SceneCharacter],
    materials: &mut MaterialCatalog,
    bodies: &mut asset_model::BodyMeshBuild,
    progress: &LoadProgress,
    report: &mut Vec<String>,
) -> Result<CharacterPerformances, String> {
    let mut sources: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for character in characters {
        sources.entry(&character.source).or_default().extend(
            std::iter::once(character.kit.body.clone()).chain(character.kit.head.iter().cloned()),
        );
    }
    for character in characters {
        if let Some(source) = &character.native_loop_source {
            sources.entry(source).or_default();
        }
    }
    for character in characters {
        for action in &character.native_actions {
            if let Some(source) = &action.source {
                sources.entry(source).or_default();
            }
        }
    }
    if sources.is_empty() {
        return Ok(BTreeMap::new());
    }
    let sources = sources
        .into_iter()
        .map(|(source, names)| {
            if !crate::scene_recipe::valid_character_source(source) {
                return Err(format!("invalid character source {source}"));
            }
            let (family, zone) = source.split_once(':').unwrap();
            let family = asset_core::FamilyId::parse(family)
                .ok_or_else(|| format!("character family missing: {source}"))?;
            let loader = lane(family)
                .and_then(|lane| lane.character_loader())
                .ok_or_else(|| format!("character donation unsupported: {source}"))?;
            Ok((source, names, family, zone, loader))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let native_kits = bodies.kits().clone();
    let reference = native_kits
        .kit(false)
        .and_then(|kit| bodies.get(&kit.body))
        .and_then(|body| body.skel.pose.as_ref())
        .cloned()
        .map(Arc::new)
        .ok_or("character scene has no native multiplayer animation rig")?;
    let games = games_root()?;
    let sources = sources
        .into_iter()
        .map(|(source, names, family, zone, loader)| {
            asset_transport::find_zone_file_version(
                &games,
                zone,
                asset_transport::zone_version(family).ok_or("character zone version missing")?,
            )
            .map(|zone| (source, names, family, zone.path, loader))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut owners = BTreeMap::new();
    let mut performances = BTreeMap::new();
    let mut animation_sources = BTreeMap::new();
    for (source, names, family, path, load_characters) in sources {
        if progress.is_canceled() {
            return Err("scene character preparation canceled".into());
        }
        let image = open_zone_shared(&path).map_err(|e| e.to_string())?;
        if image.game != family {
            return Err(format!("character donor family mismatch: {source}"));
        }
        let (donor_bodies, mut donor_materials, donor_anims) =
            load_characters(&path, &image, &names, materials.clone())
                .map_err(|error| format!("character donor {source}: {error}"))?;
        animation_sources.insert(source, (image.game, donor_anims.publish()));
        let keep: BTreeSet<_> = names
            .iter()
            .map(|name| {
                let entry = donor_bodies.get(name).ok_or_else(|| {
                    format!("character donor {source} has no captured model {name}")
                })?;
                if entry.skel.pose.is_none() {
                    return Err(format!("character model {name} has no skeleton"));
                }
                if let Some(previous) = owners.insert(name.clone(), source) {
                    return Err(format!(
                        "character model {name} requested from both {previous} and {source}"
                    ));
                }
                Ok(entry
                    .skel
                    .surface_materials
                    .iter()
                    .flatten()
                    .map(|id| id.get())
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .flatten()
            .collect();
        // Capture model material keys before compacting the donor's walk rows.
        for name in &names {
            let mut skel = donor_bodies.get(name).unwrap().skel.clone();
            if characters
                .iter()
                .any(|character| character.kit.body == *name && character.native_loop.is_none())
                && let Some(pose) = skel.pose.as_mut()
            {
                pose.animation_reference = Some(Arc::clone(&reference));
            }
            let namespace = if characters.iter().any(|character| {
                character.kit.body == *name && character.native_loop.is_some()
                    || character.kit.head.as_deref() == Some(name)
                        && character.native_loop.is_some()
            }) {
                image.game
            } else {
                asset_core::FamilyId::IW4
            };
            bodies.insert_in(namespace, skel, Some(&donor_materials));
        }
        let previous = materials.materials.len();
        donor_materials.materials = std::mem::take(&mut donor_materials.materials)
            .into_iter()
            .enumerate()
            .filter(|(index, _)| *index >= previous && keep.contains(index))
            .map(|(_, material)| material)
            .collect();
        materials.absorb_asset_population_host_materials_win(donor_materials);
        report.push(format!(
            "scene characters: donor={source} models={} (no donor world or scripts)",
            names.len()
        ));
    }
    for character in characters {
        if let Some(reference) = &character.native_loop_reference {
            let pose = bodies
                .get(reference)
                .and_then(|body| body.skel.pose.clone())
                .ok_or_else(|| format!("character animation reference {reference} missing"))?;
            if pose.animation_reference.is_some() {
                return Err("character animation reference must be a native rig".into());
            }
            if !bodies.set_animation_reference(&character.kit.body, Arc::new(pose)) {
                return Err("character performance body missing".into());
            }
        }
        if let Some(name) = &character.native_loop {
            let source = character
                .native_loop_source
                .as_deref()
                .unwrap_or(&character.source);
            let (family, animations) = animation_sources
                .get(source)
                .ok_or_else(|| format!("character animation source {source} missing"))?;
            let clip = animations.clip(*family, name).ok_or_else(|| {
                format!("character donor {source} has no decodable animation {name}")
            })?;
            let pose = bodies
                .get(&character.kit.body)
                .and_then(|body| body.skel.pose.as_ref())
                .ok_or("character performance body has no skeleton")?;
            let rig = asset_anim::DObj::build(&[(pose, None)]).map_err(|e| e.to_string())?;
            if !rig.tracks_for(&clip).iter().any(Option::is_some) {
                return Err(format!(
                    "animation {name} has no tracks for {}",
                    character.kit.body
                ));
            }
            let mut clip = (*clip).clone();
            clip.looping = true;
            let mut actions = Vec::new();
            for action in &character.native_actions {
                let action_source = action.source.as_deref().unwrap_or(source);
                let (action_family, action_animations) = animation_sources
                    .get(action_source)
                    .ok_or_else(|| format!("character animation source {action_source} missing"))?;
                let clip = action_animations
                    .clip(*action_family, &action.clip)
                    .ok_or_else(|| {
                        format!("character donor {source} has no animation {}", action.clip)
                    })?;
                if !rig.tracks_for(&clip).iter().any(Option::is_some) {
                    return Err(format!(
                        "animation {} has no tracks for {}",
                        action.clip, character.kit.body
                    ));
                }
                let mut clip = (*clip).clone();
                clip.looping = true;
                actions.push(asset_anim::PerformanceAction {
                    clip: Arc::new(clip),
                    start_ms: action.start_ms,
                    duration_ms: action.duration_ms,
                    blend_ms: action.blend_ms,
                });
            }
            performances.insert(
                (character.client, character.world),
                Arc::new(asset_anim::NativePerformance {
                    idle: Arc::new(clip),
                    actions,
                }),
            );
        }
    }
    bodies.set_kits(native_kits);
    Ok(performances)
}
