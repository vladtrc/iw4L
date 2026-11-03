//! The atlas of a merged map (`assets::MergedMap`): other maps' world clip
//! beside the match's own, and the doors between them (`sim::projectile_flight`).
//!

use std::sync::Arc;

use render_frontend::adapters::anim::killcam::AtlasClips;

pub(crate) struct PreparedAtlas {
    pub atlas: Arc<sim::world_graph::Atlas>,
    pub clips: AtlasClips,
}

pub(crate) fn prepare_atlas(
    merged: Option<&assets::MergedMap>,
    worlds: Vec<assets::PreparedAtlasWorld>,
    base_clip: &mut Option<Arc<asset_world::ClipCollision>>,
    progress: &asset_transport::LoadProgress,
) -> Result<Option<PreparedAtlas>, String> {
    let Some(merged) = merged else {
        if !worlds.is_empty() {
            return Err("atlas worlds supplied without a scene".into());
        }
        return Ok(None);
    };
    if worlds.len() + 1 != merged.scene.worlds.len() {
        return Err("atlas prepared world count differs from the scene".into());
    }
    let _span = perf::span!("atlas_prepare", "iw4l.sim");
    let path = merged.path.display();
    if let Some(clip) = base_clip.take() {
        *base_clip = Some(replace_clip(clip, merged, 0)?);
    }
    if let Some(clip) = base_clip.as_deref() {
        validate_atlas_clip(clip).map_err(|error| format!("atlas base in {path}: {error}"))?;
    }
    let mut atlas = sim::world_graph::Atlas {
        root_zone: merged.zone().into(),
        versions: merged.scene.versions.clone(),
        dvars: merged.scene.dvars.clone(),
        portals: merged
            .scene
            .portals
            .iter()
            .map(|portal| portal.pair())
            .collect(),
        portal_clearance: merged
            .scene
            .portals
            .iter()
            .map(|portal| portal.ends.each_ref().map(|end| end.trim.clearance))
            .collect(),
        portal_thickness: merged
            .scene
            .portals
            .iter()
            .map(|portal| portal.ends.each_ref().map(|end| end.thickness))
            .collect(),
        ..Default::default()
    };
    let mut clips = vec![None];
    for prepared in worlds {
        if progress.is_canceled() {
            return Err("atlas preparation canceled".into());
        }
        let world_id = atlas.worlds.len() + 1;
        if prepared.id.index() != world_id || merged.scene.worlds[world_id] != prepared.zone {
            return Err(format!(
                "atlas prepared world {world_id} differs from the scene"
            ));
        }
        validate_atlas_clip(&prepared.collision)
            .map_err(|error| format!("atlas: {}: {error}", prepared.zone))?;
        let clip = replace_clip(prepared.collision, merged, world_id)?;
        let (brushes, bsp, mesh, cmodels) = crate::sim_clip_map(&clip);
        atlas.worlds.push(Arc::new(sim::world_graph::AtlasWorld {
            zone: prepared.zone,
            brushes,
            bsp,
            mesh,
            cmodels,
        }));
        clips.push(Some(clip));
    }
    let door = |door: &sim::world_graph::AtlasDoor| *door;
    let atlas = Arc::new(atlas);
    let flight = base_clip.as_deref().map(|clip| {
        let (brushes, bsp, mesh, cmodels) = crate::sim_clip_map(clip);
        let base = Arc::new(sim::world_graph::AtlasWorld {
            zone: merged.scene.worlds[0].clone(),
            brushes,
            bsp,
            mesh,
            cmodels,
        });
        let rules = sim::projectile_flight::ProjectileFlightRules::default()
            .with_dvars(|key| merged.scene.dvars.get(key)?.parse().ok());
        (atlas.clone(), base, rules)
    });
    let killcam = AtlasClips {
        flight,
        clips,
        doors: atlas
            .portals
            .iter()
            .map(|portal| render_frame::PortalPair {
                a: door(&portal.a),
                b: door(&portal.b),
                flip: portal.flip,
            })
            .collect(),
    };
    diag::info!(
        World,
        "atlas: {} worlds, {} portals from {path}",
        atlas.worlds.len() + 1,
        atlas.portals.len()
    );
    Ok(Some(PreparedAtlas {
        atlas,
        clips: killcam,
    }))
}

fn validate_atlas_clip(clip: &asset_world::ClipCollision) -> Result<(), String> {
    let model = clip.cmodels.first().ok_or("missing world clip model")?;
    if !asset_core::atlas::atlas_bounds_fit(model.mins[0], model.maxs[0]) {
        return Err(format!(
            "native x bounds {}..{} exceed atlas coordinate domain",
            model.mins[0], model.maxs[0]
        ));
    }
    Ok(())
}

pub fn replace_clip(
    clip: Arc<asset_world::ClipCollision>,
    merged: &assets::MergedMap,
    world: usize,
) -> Result<Arc<asset_world::ClipCollision>, String> {
    let replacements: Vec<_> = merged
        .scene
        .replacements()
        .filter(|&r| r.world == world)
        .cloned()
        .collect();
    let trims: Vec<_> = merged
        .scene
        .trims()
        .filter(|&t| t.world == world)
        .cloned()
        .collect();
    if replacements.is_empty() && trims.is_empty() {
        return Ok(clip);
    }
    let mut clip = Arc::unwrap_or_clone(clip);
    asset_world::PortalDoorReplacement::apply_all_collision(&replacements, &mut clip)?;
    for trim in &trims {
        trim.apply_collision(&mut clip)?;
    }
    if !trims.is_empty() {
        let (brushes, bsp, mesh, _) = crate::sim_clip_map(&clip);
        for trim in &trims {
            let door = sim::world_graph::AtlasDoor {
                two_sided: trim.two_sided,
                world: sim::world_graph::WorldId::ROOT,
                base: trim.aperture.base,
                normal: trim.aperture.normal,
                width: trim.aperture.width,
                height: trim.aperture.height,
            };
            let flight = trim.clearance == asset_world::PortalClearance::Flight;
            let validate = if flight {
                sim::door_exit::validate_flight
            } else {
                sim::door_exit::validate
            };
            validate(&door, trim.thickness, |start, end| {
                sim::trace_world_clip(
                    &brushes,
                    &bsp,
                    &mesh,
                    &clipmap_iw4::TraceExtents {
                        start,
                        end,
                        mins: if flight {
                            [-2.0; 3]
                        } else {
                            [-15.0, -15.0, 0.0]
                        },
                        maxs: if flight { [2.0; 3] } else { [15.0, 15.0, 70.0] },
                        mask: if flight {
                            sim::MASK_SHOT
                        } else {
                            sim::MASK_PLAYER_SOLID
                        },
                    },
                )
            })
            .map_err(|error| {
                format!(
                    "{}: trimmed doorway has no valid accessible exit: {error}",
                    trim.door_id
                )
            })?;
        }
    }
    for replacement in replacements {
        diag::info!(
            World,
            "portal collision replacement: {} world={world} models={:?} brushes={:?}",
            replacement.door_id,
            replacement.clip_models,
            replacement.clip_brushes
        );
    }
    Ok(Arc::new(clip))
}
