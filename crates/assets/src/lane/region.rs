use super::sink::ZoneWalkSink;
use asset_material::AssetPointerIdentity;
use asset_world::WorldRegion;
use fastfile_iw4::{GfxWorldGeometry, ZoneStream};
use std::collections::{HashMap, HashSet};

pub(super) fn compile_models(
    sink: &mut ZoneWalkSink,
    stream: &ZoneStream<'_>,
    geometry: GfxWorldGeometry,
    region: WorldRegion,
) -> Result<(Vec<asset_core::MaterialKey>, Vec<bool>), String> {
    let mask = region
        .smodels(stream, geometry)
        .map_err(|e| e.to_string())?;
    let instances =
        asset_world::build_static_model_instances(stream, geometry).map_err(|e| e.to_string())?;
    let mut needed: HashSet<AssetPointerIdentity> = instances
        .iter()
        .zip(&mask)
        .filter(|(_, keep)| **keep)
        .map(|(inst, _)| inst.model_slot)
        .collect();
    let mut aliases: HashMap<AssetPointerIdentity, AssetPointerIdentity> = sink
        .deferred_aliases
        .iter()
        .map(|(slot, target)| ((*slot).into(), (*target).into()))
        .collect();
    for model in &sink.deferred_models {
        if let Some(insert) = model.insert_slot {
            aliases.insert(insert.into(), model.slot.into());
        }
    }
    for original in needed.clone() {
        let mut slot = original;
        for _ in 0..=aliases.len() {
            let Some(next) = aliases.get(&slot) else {
                break;
            };
            if !needed.insert(*next) {
                break;
            }
            slot = *next;
        }
    }
    let scripts = asset_world::script_model_placements(stream);
    let mut script_names: HashSet<String> = scripts
        .iter()
        .filter(|p| region.contains(p.origin))
        .map(|p| p.model.clone())
        .collect();
    if let Some(clip) = stream.clip_map() {
        let names: HashMap<AssetPointerIdentity, String> = sink
            .deferred_models
            .iter()
            .filter_map(|model| {
                Some((
                    model.slot.into(),
                    stream.cstr(model.geometry.name?).ok()?.to_owned(),
                ))
            })
            .collect();
        let dynents = asset_world::build_dyn_ent_catalog(
            stream,
            clip,
            |slot| {
                let mut slot: AssetPointerIdentity = slot.into();
                for _ in 0..=aliases.len() {
                    if let Some(name) = names.get(&slot) {
                        return Some(name.clone());
                    }
                    slot = *aliases.get(&slot)?;
                }
                None
            },
            |_| None,
            |_| None,
        );
        script_names.extend(
            dynents
                .models
                .iter()
                .filter(|p| region.contains(p.origin))
                .filter_map(|p| p.xmodel.clone()),
        );
    }
    let total = sink.deferred_models.len();
    let mut captured = 0;
    let mut material_ids = HashSet::new();
    let surfaces = region
        .surfaces(stream, geometry)
        .map_err(|e| e.to_string())?;
    if let Some(table) = geometry.surfaces {
        for (i, keep) in surfaces.iter().enumerate() {
            if *keep
                && let Some(id) = sink.materials.material_index(
                    table.at(i * stream.layout(asset_iw4::size::GFX_SURFACE, 32) + 16),
                )
            {
                material_ids.insert(id.get());
            }
        }
    }
    for model in std::mem::take(&mut sink.deferred_models) {
        let name = model
            .geometry
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("");
        if !needed.contains(&model.slot.into()) && !script_names.contains(name) {
            continue;
        }
        captured += 1;
        sink.map_xmodels.capture_geometry(
            stream,
            &sink.materials,
            model.slot,
            model.insert_slot,
            &sink.script_strings,
            &sink.phys_presets,
            model.geometry,
        );
        sink.xmodel_coll
            .insert_geometry(stream, model.slot, model.insert_slot, model.geometry);
        if let Some(handles) = model.geometry.material_handles {
            for i in 0..model.geometry.material_handle_count {
                if let Some(id) = sink
                    .materials
                    .material_index(handles.at(i * stream.layout(4, 8)))
                {
                    material_ids.insert(id.get());
                }
            }
        }
    }
    for (slot, target) in &sink.deferred_aliases {
        sink.map_xmodels.alias(*slot, *target);
        sink.xmodel_coll.alias(*slot, *target);
    }
    diag::info!(
        World,
        "world region: model meshes decoded={captured}/{total}, static instances selected={}/{}",
        mask.iter().filter(|v| **v).count(),
        mask.len()
    );
    let keys = material_ids
        .into_iter()
        .filter_map(|id| sink.materials.materials.get(id))
        .map(|m| asset_core::MaterialKey {
            namespace: asset_core::FamilyId::IW4,
            name: m.name.to_string(),
        })
        .collect();
    Ok((keys, mask))
}
