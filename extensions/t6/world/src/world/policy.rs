use asset_world::*;

pub const POLICY: WorldFamily = WorldFamily {
    draw: WorldDrawPolicy {
        family: crate::ID,
        resolve_specular_env: true,
        lightmap_requires_image: false,
        decode_color_at_convert: false,
        sun_sample_size_near: 0.5,
    },
    complete_objective_models: Some(complete_objective_models),
    normalize_entities,
    spawn_points,
};

fn normalize_entities(text: &str) -> String {
    normalize_map_entities(
        text,
        &EntityRules {
            overtime: Some(OvertimeEntityRule {
                target: "bombzone_dem",
                label: "_overtime",
                replacement: "dem_overtime_bombzone",
                gameobject: "dem_overtime",
            }),
            gametype_aliases: &[("dem", "dd"), ("tdm", "war")],
            augment: augment_sabotage,
        },
    )
}

fn augment_sabotage(blocks: &[EntityBlock], spawned: &mut Vec<EntityBlock>) {
    let source = blocks.to_vec();
    let has = |key: &str, name: &str| {
        source
            .iter()
            .any(|block| entity_value(block, key) == Some(name))
    };
    let authored = |block: &EntityBlock, classname: &str, targetname: &str| {
        let mut copy = block.clone();
        set_entity_value(&mut copy, "classname", classname.to_owned());
        set_entity_value(&mut copy, "targetname", targetname.to_owned());
        set_entity_value(&mut copy, "script_gameobjectname", "sab".to_owned());
        copy
    };
    if !has("targetname", "sab_bomb_pickup_trig")
        && let Some(trigger) = source
            .iter()
            .find(|block| entity_value(block, "targetname") == Some("bombtrigger"))
            .or_else(|| {
                source
                    .iter()
                    .find(|block| entity_value(block, "targetname") == Some("sd_bomb_pickup_trig"))
            })
    {
        spawned.push(authored(
            trigger,
            "trigger_multiple",
            "sab_bomb_pickup_trig",
        ));
        if !has("targetname", "sab_bomb") {
            let mut bomb = trigger.clone();
            set_entity_value(&mut bomb, "model", "prop_suitcase_bomb".to_owned());
            spawned.push(authored(&bomb, "script_model", "sab_bomb"));
        }
    }
    for team in ["allies", "axis"] {
        let site = format!("sab_bomb_{team}");
        if !has("targetname", &site)
            && let Some(trigger) = source.iter().find(|block| {
                entity_value(block, "targetname") == Some(&format!("ctf_zone_{team}"))
            })
        {
            let mut copy = trigger.clone();
            let visual = format!("iw4l_sab_site_{team}");
            set_entity_value(&mut copy, "target", visual.clone());
            spawned.push(authored(&copy, "trigger_use_touch", &site));
            set_entity_value(&mut copy, "model", "prop_suitcase_bomb".to_owned());
            spawned.push(authored(&copy, "script_model", &visual));
        }
        for suffix in ["", "_start", "_planted"] {
            let classname = format!("mp_sab_spawn_{team}{suffix}");
            if has("classname", &classname) {
                continue;
            }
            let native = format!(
                "mp_ctf_spawn_{team}{}",
                if suffix == "_planted" { "" } else { suffix }
            );
            for block in source
                .iter()
                .filter(|block| entity_value(block, "classname") == Some(&native))
            {
                let mut copy = block.clone();
                set_entity_value(&mut copy, "classname", classname.clone());
                set_entity_value(&mut copy, "script_gameobjectname", "sab".to_owned());
                spawned.push(copy);
            }
        }
    }
}

pub fn spawn_points(text: &str) -> Vec<SpawnPoint> {
    let names: Vec<_> = REFERENCE_MP_SPAWN_CLASSNAMES
        .iter()
        .map(|name| name.replace("mp_dd_", "mp_dem_"))
        .collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    let mut spawns = parse_spawn_points(text, &names);
    for spawn in &mut spawns {
        if let Some(suffix) = spawn.classname.strip_prefix("mp_dem_") {
            spawn.classname = format!("mp_dd_{suffix}");
        }
    }
    spawns
}

fn complete_objective_models(
    scene: &mut MapXModelSceneCatalog,
    names: &[&str],
    models: &asset_model::WorldWeaponCatalog,
) {
    let mut objectives = MapXModelSceneCatalog::default();
    for &name in names {
        if !matches!(
            scene.get_name(name),
            Some(MapXModelSceneAsset::Captured { family, .. }) if *family == crate::ID
        ) && let Some(index) = models.index_by_name(crate::ID, name)
            && let Some(model) = models.get_at(index)
        {
            objectives.set_capture_zone(models.zone_of(index));
            objectives.insert(
                MapXModelAssetKey(name.to_owned()),
                MapXModelSceneAsset::Captured {
                    family: crate::ID,
                    skel: model.skel.clone(),
                },
            );
        }
    }
    scene.absorb_captured(objectives);
}
