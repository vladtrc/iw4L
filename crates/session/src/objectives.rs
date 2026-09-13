use assets::{MapUseTrigger, PreparedWorld};
use bevy::prelude::*;
use gamemode_iw4::{GameModeKind, Team};

pub(crate) fn prepare(
    world: &mut PreparedWorld,
    triggers: &[MapUseTrigger],
    kind: GameModeKind,
) -> Result<Vec<(u32, Vec<u32>, Vec<u32>)>, String> {
    let mut bindings = Vec::new();
    for trigger in triggers {
        let model = match kind {
            GameModeKind::Domination
                if matches!(
                    trigger.targetname.as_str(),
                    "flag_primary" | "flag_secondary"
                ) && gamemode_iw4::gameobject_survives(&trigger.gameobject, kind) =>
            {
                gamemode_iw4::dom::FLAG_MODEL_NEUTRAL
            }
            GameModeKind::Demolition
                if trigger.targetname == "bombzone"
                    && gamemode_iw4::gameobject_survives(&trigger.gameobject, kind) =>
            {
                gamemode_iw4::dd::PLANTED_MODEL
            }
            _ => continue,
        };
        if !matches!(
            world.map_xmodel_scene_assets.get_name(model),
            Some(assets::MapXModelSceneAsset::Iw4(_))
        ) {
            return Err(format!("objective model {model} not captured"));
        }
        if kind == GameModeKind::Demolition {
            if trigger.angles != [0.0; 3] || trigger.hulls.as_ref().is_none_or(Vec::is_empty) {
                return Err(format!(
                    "DD {} requires unrotated authored IW4 trigger hulls",
                    trigger.script_label
                ));
            }
            let visuals: Vec<_> = world
                .script_model_instances
                .iter()
                .filter(|m| !trigger.target.is_empty() && m.metadata.targetname == trigger.target)
                .collect();
            if visuals.is_empty() {
                return Err(format!(
                    "DD {} has no target visuals {}",
                    trigger.script_label, trigger.target
                ));
            }
            let mut intact: Vec<u32> = visuals.iter().map(|m| m.id.source_ordinal()).collect();
            for visual in &visuals {
                if let assets::ScriptBrushModelLink::Linked(brush) = &visual.metadata.brush_link {
                    intact.push(brush.source_ordinal);
                }
            }
            let groups: Vec<&str> = visuals
                .iter()
                .map(|m| m.metadata.script_exploder.as_str())
                .filter(|g| !g.is_empty())
                .collect();
            let destroyed: Vec<u32> = world
                .script_model_instances
                .iter()
                .filter(|m| {
                    groups.contains(&m.metadata.script_exploder.as_str())
                        && gamemode_iw4::setup_exploders_hides(
                            &m.current_model.0,
                            &m.metadata.targetname,
                            &m.metadata.script_exploder,
                        )
                })
                .map(|m| m.id.source_ordinal())
                .collect();
            bindings.push((trigger.source_ordinal, intact, destroyed));
        }
        let [pitch, yaw, roll] = trigger.angles.map(f32::to_radians);
        world
            .script_model_instances
            .push(assets::ScriptModelSceneInstance {
                id: assets::ScriptModelId::from_source_ordinal(trigger.source_ordinal),
                current_model: assets::MapXModelAssetKey(model.to_owned()),
                transform: Transform {
                    translation: Vec3::from_array(trigger.origin),
                    rotation: Quat::from_euler(EulerRot::ZYX, yaw, pitch, roll),
                    scale: Vec3::ONE,
                },
                lighting_origin: trigger.origin,
                dobj_state: assets::dobj::DObjSemanticState::bind_pose(model.to_owned(), 1, 1),
                metadata: assets::ScriptModelMetadata {
                    gameobject: kind.token().to_owned(),
                    ..Default::default()
                },
            });
    }
    if kind == GameModeKind::Demolition && bindings.len() != 2 {
        return Err("DD requires exactly two authored sites".into());
    }
    Ok(bindings)
}

pub(crate) fn install(
    world: &mut sim::SimWorld,
    weapons: &assets::WeaponRegistry,
    triggers: &[MapUseTrigger],
    kind: GameModeKind,
) -> Result<(), String> {
    world.objectives = sim::ObjectiveMatch::default();
    if kind == GameModeKind::Domination {
        world.objectives.flags = world
            .use_objects()
            .iter()
            .filter(|o| o.callback_kind == gamemode_iw4::UseCallbackKind::DomFlag)
            .map(|o| {
                let trigger = triggers
                    .iter()
                    .find(|t| {
                        t.origin == o.script_origin
                            && t.script_label.trim_start_matches('_').eq_ignore_ascii_case(
                                o.script_label.as_str().trim_start_matches('_'),
                            )
                    })
                    .ok_or("DOM flag model has no authored trigger")?;
                Ok(sim::ObjectiveView {
                    id: o.id,
                    model_source: trigger.source_ordinal,
                    label: trigger.script_label.trim_start_matches('_').to_uppercase(),
                    origin: o.script_origin,
                    ..Default::default()
                })
            })
            .collect::<Result<_, String>>()?;
    }
    if kind == GameModeKind::Demolition {
        for (slot, name) in [
            gamemode_iw4::dd::BRIEFCASE_PLANT,
            gamemode_iw4::dd::BRIEFCASE_DEFUSE,
        ]
        .into_iter()
        .enumerate()
        {
            let id = weapons
                .resolve_index(&format!("iw4:weapon/{name}"))
                .map_err(|_| format!("DD weapon missing: {name}"))?
                .ok_or("empty DD weapon")?;
            diag::info!(
                Sim,
                "DD use weapon: {name} id={id} gun={:?} idle={:?}",
                weapons.gun_xmodel_of(id),
                weapons.idle_anim_of(id)
            );
            world.objectives.use_weapons[slot] = id;
        }
        for trigger in triggers.iter().filter(|t| {
            t.targetname == "bombzone" && gamemode_iw4::gameobject_survives(&t.gameobject, kind)
        }) {
            if trigger.angles != [0.0; 3] {
                return Err("rotated DD trigger requires transformed hulls".to_owned());
            }
            let hulls: Vec<sim::ObjectiveHull> = if let Some(hulls) = &trigger.hulls {
                hulls
                    .iter()
                    .map(|h| sim::ObjectiveHull {
                        mid: std::array::from_fn(|i| h.mid[i] + trigger.origin[i]),
                        half: h.half,
                        slabs: h
                            .slabs
                            .iter()
                            .map(|(dir, mid, half)| {
                                (
                                    *dir,
                                    mid + (0..3).map(|i| dir[i] * trigger.origin[i]).sum::<f32>(),
                                    *half,
                                )
                            })
                            .collect(),
                    })
                    .collect()
            } else {
                return Err(format!(
                    "DD site {} missing authored IW4 trigger hulls ({})",
                    trigger.script_label, trigger.model
                ));
            };
            if hulls.is_empty() {
                return Err("DD trigger has no hulls".to_owned());
            }
            let mins = std::array::from_fn(|i| {
                hulls
                    .iter()
                    .map(|h| h.mid[i] - h.half[i])
                    .fold(f32::INFINITY, f32::min)
            });
            let maxs = std::array::from_fn(|i| {
                hulls
                    .iter()
                    .map(|h| h.mid[i] + h.half[i])
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            world.objectives.bombs.push(sim::BombSite {
                view: sim::ObjectiveView {
                    id: trigger.source_ordinal,
                    model_source: trigger.source_ordinal,
                    label: trigger.script_label.trim_start_matches('_').to_uppercase(),
                    origin: trigger.origin,
                    owner: Team::Axis,
                    ..Default::default()
                },
                intact_sources: Vec::new(),
                destroyed_sources: Vec::new(),
                hulls,
                mins,
                maxs,
                planted_at_ms: None,
                planter: None,
                bomb_origin: trigger.origin,
                bomb_angles: [0.0; 3],
                destroyed: false,
                user: None,
                return_weapon: None,
                hold: gamemode_iw4::UseHoldLoopState::begin(),
            });
        }
        world
            .objectives
            .bombs
            .sort_by(|a, b| a.view.label.cmp(&b.view.label));
        if world.objectives.bombs.len() != 2
            || world.objectives.bombs[0].view.label != "A"
            || world.objectives.bombs[1].view.label != "B"
        {
            return Err("Demolition requires authored bomb sites A and B".to_owned());
        }
    }
    Ok(())
}

pub(crate) fn flag_models(
    catalog: &assets::MenuCatalog,
    arena: &str,
    zone: &str,
) -> Result<[String; 3], String> {
    let row = assets::arena_charsets(arena, zone).ok_or("DOM map faction row missing")?;
    let table = catalog
        .string_table(gamemode_iw4::FACTION_TABLE)
        .ok_or("DOM faction table missing")?;
    let axis = row
        .axischar
        .as_deref()
        .unwrap_or(gamemode_iw4::DEFAULT_AXIS_CHARSET);
    let allies = row
        .allieschar
        .as_deref()
        .unwrap_or(gamemode_iw4::DEFAULT_ALLIES_CHARSET);
    let models = [
        gamemode_iw4::dom::FLAG_MODEL_NEUTRAL.to_owned(),
        table.lookup_col(axis, 10).to_owned(),
        table.lookup_col(allies, 10).to_owned(),
    ];
    if models.iter().any(String::is_empty) {
        return Err("DOM faction flag model missing".to_owned());
    }
    Ok(models)
}
