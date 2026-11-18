use super::*;
use std::fmt::Write;

/// Keep the MP donor's player products, but publish only the scene's world.
/// Seeding with the donor pool preserves its player model material indices.
pub(super) fn replace_world(
    gameplay: &mut LoadedWorld,
    recipe: &crate::SceneMultiplayer,
    world: &str,
    path: &Path,
    gameplay_zone: &str,
    progress: &LoadProgress,
    visions: &crate::FilmVisionCatalog,
) -> Result<(), String> {
    let image = open_zone_shared(path).map_err(|e| e.to_string())?;
    if image.game != asset_core::FamilyId::IW4 {
        return Err("scene multiplayer world must belong to IW4".into());
    }
    let selected = lane(image.game).ok_or("IW4 world lane missing")?;
    let geometry = selected.load_world(
        path,
        &image,
        progress,
        Default::default(),
        gameplay.materials.clone(),
        visions,
    );
    if progress.is_canceled() {
        return Err("scene multiplayer world load canceled".into());
    }
    if geometry.collision.is_none() {
        return Err(format!("scene multiplayer world {world} has no collision"));
    }
    gameplay.world = geometry.world;
    gameplay.materials = geometry.materials;
    gameplay.collision = geometry.collision;
    gameplay.facts.minimap_corners = geometry.facts.minimap_corners;
    gameplay.facts.north_yaw = geometry.facts.north_yaw;
    gameplay.facts.airstrike_height = geometry.facts.airstrike_height;
    gameplay.facts.compass = geometry.facts.compass;
    gameplay.sound = geometry.sound;
    gameplay.arena_bytes += geometry.arena_bytes;
    gameplay.report.extend(geometry.report);
    gameplay.gaps.extend(geometry.gaps);
    gameplay.spawns = recipe.spawns.clone();

    // Mission triggers, actors and donor-map entities must not enter FFA.
    // Authored visual/brush placements remain in PreparedWorld independently.
    let mut entities = String::from("{\n\"classname\" \"worldspawn\"\n}\n");
    let view = &recipe.spawns[0];
    writeln!(entities, "{{\n\"classname\" \"mp_global_intermission\"\n\"origin\" \"{} {} {}\"\n\"angles\" \"{} {} {}\"\n}}",
        view.origin[0], view.origin[1], view.origin[2] + 60.0,
        view.angles[0], view.angles[1], view.angles[2]).unwrap();
    for spawn in &recipe.spawns {
        writeln!(entities, "{{\n\"classname\" \"mp_dm_spawn\"\n\"origin\" \"{} {} {}\"\n\"angles\" \"{} {} {}\"\n}}",
            spawn.origin[0],spawn.origin[1],spawn.origin[2],
            spawn.angles[0],spawn.angles[1],spawn.angles[2]).unwrap();
    }
    gameplay.scripts.set_entities(entities);
    gameplay.scripts.set_path_nodes(Vec::new(), Vec::new());
    gameplay.scripts.insert_source(&format!("maps/mp/{gameplay_zone}"),
        "main() { maps\\mp\\_load::main(); game[\"attackers\"] = \"allies\"; game[\"defenders\"] = \"axis\"; }".into());
    gameplay.world.intermission_view = Some(asset_world::IntermissionView {
        origin: recipe.spawns[0].origin,
        angles: recipe.spawns[0].angles,
    });
    gameplay.report.push(format!("scene multiplayer: world={world} source={} gametype=dm spawns={} campaign scripts/entities excluded",
        recipe.source, recipe.spawns.len()));
    Ok(())
}
