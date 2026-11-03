use crate::{MapXModelCatalog, link_model_placements};
use asset_material::MaterialCatalog;
use asset_world::{
    MapXModelAssetKey, MapXModelSceneAsset, PreparedMapModels, StaticModelDrawError,
    StaticModelPlacement, build_static_model_instances, build_xmodel_mesh, flag_descriptors,
    map_script_structs, script_brush_model_placements, script_model_placements,
};
use fastfile_iw4::{Ptr, ZoneStream};

impl MapXModelCatalog {
    pub(crate) fn capture(
        &mut self,
        stream: &ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: Ptr,
        insert_slot: Option<Ptr>,
        strings: &fastfile_iw4::ScriptStrings,
        phys_presets: &asset_world::PhysPresetCatalog,
    ) {
        let Some(geometry) = stream.xmodel() else {
            self.note_failed_mesh();
            return;
        };
        let slot_live = geometry.phys_preset.is_some();
        let named = geometry
            .phys_preset
            .and_then(|preset_slot| phys_presets.at_slot(preset_slot))
            .is_some();
        self.note_xmodel_phys_preset(slot_live, named);
        let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok()) else {
            self.note_failed_mesh();
            return;
        };
        let scene_key = MapXModelAssetKey(name.to_owned());
        let scene_asset = asset_model::capture_xmodel_skel_with_shared(
            stream,
            strings,
            geometry,
            Some(materials),
            &self.shared_surfaces,
        )
        .map(|skel| MapXModelSceneAsset::Captured {
            family: asset_core::FamilyId::IW4,
            skel: std::sync::Arc::new(skel),
        })
        .unwrap_or(MapXModelSceneAsset::Unavailable {
            reason: "IW4 XModel scene skeleton capture failed",
        });
        self.insert_scene_asset(scene_key, scene_asset);
        let Ok(mesh) = build_xmodel_mesh(stream, geometry, Some(materials)) else {
            self.note_failed_mesh();
            return;
        };
        self.insert_mesh(mesh, slot, insert_slot.map(Into::into));
    }
}
pub(crate) fn build_static_model_draw(
    stream: &ZoneStream<'_>,
    geometry: fastfile_iw4::GfxWorldGeometry,
    catalog: MapXModelCatalog,
) -> PreparedMapModels {
    let placements = build_static_model_instances(stream, geometry).map_err(|_| {
        StaticModelDrawError::InstancesUnavailable {
            smodel_count: geometry.smodel_count,
        }
    });
    let scripts = script_model_placements(stream);
    let brushes = script_brush_model_placements(stream);
    let descriptors = flag_descriptors(stream);
    let structs = map_script_structs(stream);
    link_model_placements(
        placements,
        &scripts,
        brushes,
        descriptors,
        structs,
        catalog,
        geometry.smodel_count,
    )
}
pub fn report_map_models(
    report: &mut Vec<String>,
    models: &PreparedMapModels,
    authored_slots: impl std::fmt::Display,
) {
    if let Some(error) = models.static_error.as_ref() {
        report.push(format!("static models: {error}"));
    }
    let smodels = &models.static_draw;
    report.push(format!(
        "static models: {}/{authored_slots} authored slots resolved to {} unique meshes ({} unresolved)",
        smodels.resolved_count(),
        smodels.meshes.len(),
        smodels.gaps
    ));
    report.push(format!(
        "script_model: {} placements linked (MapEnts props; separate visibility owner)",
        models.script_instances.len()
    ));
    report.push(format!(
        "script_brushmodel: {} *N placements (SP_script_brushmodel, not DrawInst)",
        models.script_brush_models.len()
    ));
}

pub fn report_ffa_spawns(report: &mut Vec<String>, spawns: &[asset_world::SpawnPoint]) {
    report.push(format!(
        "ffa spawns: {} mp_dm_spawn* ({} start)",
        spawns.len(),
        spawns.iter().filter(|p| p.is_initial()).count()
    ));
}

pub fn report_intermission(report: &mut Vec<String>, view: Option<&asset_world::IntermissionView>) {
    report.push(match view {
        Some(view) => format!(
            "camera: mp_global_intermission origin={:?} angles={:?}",
            view.origin, view.angles
        ),
        None => "camera: mp_global_intermission not found".into(),
    });
}

pub fn report_world_batches(report: &mut Vec<String>, draw: &asset_world::WorldDraw) {
    let lightmapped = draw.surface_lightmapped.iter().filter(|&&lit| lit).count();
    report.push(format!(
        "world batches: {lightmapped} lightmapped, {} fallback surfaces",
        draw.surface_lightmapped.len() - lightmapped
    ));
}

pub fn report_dpvs(report: &mut Vec<String>, draw: &asset_world::WorldDraw) {
    report.push(format!(
        "dpvs: cells={} planes={} nodes={} sorted={} portal_verts={} cleared_boxes={}",
        draw.dpvs.cell_count,
        draw.dpvs.planes.len(),
        draw.dpvs.nodes.len(),
        draw.dpvs.sorted_surf_index.len(),
        draw.dpvs.portal_verts.len(),
        draw.dpvs.cleared_boxes
    ));
}

pub fn decode_reflection_probes(
    report: &mut Vec<String>,
    draw: &asset_world::WorldDraw,
    materials: &MaterialCatalog,
) -> Vec<Option<bevy::prelude::Image>> {
    let images: Vec<_> = draw
        .reflection_probes
        .iter()
        .map(|probe| {
            let source = materials.images.get(probe.image?)?;
            if let Some(decoded) = &source.decoded {
                return Some(decoded.as_ref().clone());
            }
            asset_material::decode_reflection_probe_cubemap(source)
                .map_err(|error| {
                    report.push(format!("reflection probe {} gap: {error}", source.name))
                })
                .ok()
        })
        .collect();
    report.push(format!(
        "reflection probes: {}/{} cubemaps decoded",
        images.iter().flatten().count(),
        images.len()
    ));
    images
}

pub fn smodel_lighting_samples(
    report: &mut Vec<String>,
    grid: &asset_model::OwnedLightGrid,
    origins: Vec<(usize, [f32; 3])>,
    placements: &[Option<StaticModelPlacement>],
    clip: Option<&asset_world::ClipCollision>,
) -> Vec<asset_model::SmodelLightingSample> {
    use asset_model::model_lighting::{
        build_smodel_lighting_samples_with_sight, census_lit_fragment_tiles,
    };
    use lighting_iw4::LIGHT_GRID_SIGHT_CONTENT_MASK as MASK;
    let origins: Vec<_> = origins
        .into_iter()
        .filter(|(slot, _)| placements.get(*slot).is_some_and(Option::is_some))
        .collect();
    let tiles = match clip {
        Some(clip_map) => {
            let clear = |start, end| clip_map.box_sight_clear(start, end, MASK);
            let (tiles, census) =
                build_smodel_lighting_samples_with_sight(&grid.view(), &origins, Some(&clear));
            report.push(format!(
                "smodel lighting: lit={} / candidates={} (blocked row={} trunc={} empty={}; CM sight mask=0x{MASK:x} corners need={} cleared={} suppressed={})",
                census.lit,
                census.candidates,
                census.blocked_unmodelled_row,
                census.blocked_truncated,
                census.blocked_no_live_corner,
                census.corners_needing_sight,
                census.corners_needing_sight.saturating_sub(census.corners_sight_suppressed),
                census.corners_sight_suppressed,
            ));
            tiles
        }
        None => {
            let (tiles, census) =
                build_smodel_lighting_samples_with_sight(&grid.view(), &origins, None);
            report.push(format!(
                "smodel lighting: lit={} / candidates={} (no clipmap — needsTrace corners suppressed; blocked row={} trunc={} empty={})",
                census.lit,
                census.candidates,
                census.blocked_unmodelled_row,
                census.blocked_truncated,
                census.blocked_no_live_corner,
            ));
            tiles
        }
    };
    if let Some(frag) = census_lit_fragment_tiles(&tiles) {
        report.push(format!(
            "smodel lit_fragment mid-grey: tiles={} lum min={:.4} max={:.4} mean={:.4} (specular=0)",
            frag.tiles, frag.lum_min, frag.lum_max, frag.lum_mean
        ));
    }
    tiles
}
