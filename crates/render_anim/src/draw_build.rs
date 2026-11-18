use crate::geometry::{append_mesh, install_retained_packed};
use bevy::prelude::*;
use render_material::{RuntimeMaterialCatalog, SortedMaterialOrdinal};
use render_scene::{SmodelPassMaterial, WorldModelLightingAtlas};

use crate::anim::fpv_pose::PosedModelSurface;
use crate::anim::remote_body::RemoteBodySkinnedItem;
use crate::anim::xmodel_pose::PosedSmodelSurface;
use crate::draw::{
    BODY_PACKED_UNAVAILABLE, DynEntAssetDraw, DynEntDrawPlan, FPV_PACKED_EMPTY_PLAN,
    FPV_PACKED_UNAVAILABLE, FpvDrawPlan, ItemDrawPlan, MergedModelRows, MissileDrawPlan,
    RemoteBodyDrawPlan, RemoteBodySurfaceDraw, ScriptModelAssetDraw, ScriptModelDrawPlan,
    stamp_plan_geometry, topology_fingerprint,
};

pub const XMODEL_PACKED_UNAVAILABLE: &str =
    "xmodel merge GfxPackedVertex missing or count-mismatched; decoded float is not packed VB";
pub const XMODEL_PACKED_EMPTY_PLAN: &str = "xmodel plan has no vertices";

impl MergedModelRows {
    pub fn append_posed(
        &mut self,
        surfaces: &[PosedModelSurface],
        materials: &[Option<SmodelPassMaterial>],
    ) -> Vec<(u32, u32)> {
        let mut packed_ok = self.vertices.is_empty()
            || matches!(
                self.packed_vertices,
                asset_world::PackedVertexPayload::Iw4(_)
            );
        let mut packed = match std::mem::take(&mut self.packed_vertices) {
            asset_world::PackedVertexPayload::Iw4(rows) => rows,
            asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
        };
        let mut rows = Vec::new();
        for (surface, material) in surfaces.iter().zip(materials) {
            let Some(material) = material else { continue };
            let before = self.vertices.len();
            let Some(range) = append_mesh(&surface.mesh, &mut self.vertices, &mut self.indices)
            else {
                continue;
            };
            if packed_ok && surface.packed_vertices.len() == self.vertices.len() - before {
                packed.extend_from_slice(&surface.packed_vertices);
            } else {
                packed_ok = false;
                packed.clear();
            }
            rows.push((
                self.surface_ranges.len() as u32,
                self.materials.len() as u32,
            ));
            self.surface_ranges.push(range);
            self.materials.push(material.clone());
        }
        self.packed_vertices = install_retained_packed(
            packed_ok,
            packed,
            self.vertices.len(),
            XMODEL_PACKED_EMPTY_PLAN,
            XMODEL_PACKED_UNAVAILABLE,
        );
        rows
    }
}

pub fn body_lit_pass_material(
    lighting: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    material_key: &asset_core::MaterialKey,
) -> Option<SmodelPassMaterial> {
    let material_sorted_index = catalog
        .material_for_key(material_key)
        .and_then(|material| {
            catalog
                .parts()
                .sorted_materials
                .ordinal_for_asset_id(usize::from(material.asset_id.0))
        })
        .map(SortedMaterialOrdinal::get);
    let material_sorted_index = material_sorted_index?;
    lit_xmodel_pass_material(lighting, material_sorted_index)
}

pub fn bound_lit_xmodel_pass_material(
    lighting: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    material_index: assets::MaterialIndex,
) -> Option<SmodelPassMaterial> {
    let ordinal = catalog.ordinal_for_asset_id(material_index)?.get();
    lit_xmodel_pass_material(lighting, ordinal)
}

pub fn authored_lit_xmodel_pass_material(
    lighting: &WorldModelLightingAtlas,
    catalog: &RuntimeMaterialCatalog,
    image_handles: &[Option<Handle<Image>>],
    authored: assets::MaterialIndex,
) -> Option<SmodelPassMaterial> {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };
    let world_material = catalog.derived(authored)?;
    let ordinal = catalog.ordinal_for_asset_id(authored)?;
    let maps = render_scene::runtime_maps(Some(authored), catalog, image_handles);
    let inv_h = model_lighting_inv_image_height(lighting.dims.image_height)?;
    let scale = model_lighting_lookup_scale(inv_h);
    Some(SmodelPassMaterial {
        model_lighting_required: true,
        color: maps.color,
        specular: maps.specular,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode: maps.alpha_mode,
        draw_mode: maps.draw_mode,
        cull_mode: maps.cull_mode,
        env_map_parms: maps.env_map_parms,
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: world_material.sort_key,
        material_sorted_index: Some(ordinal.get()),
    })
}

fn lit_xmodel_pass_material(
    lighting: &WorldModelLightingAtlas,
    material_sorted_index: u32,
) -> Option<SmodelPassMaterial> {
    use lighting_iw4::{
        MODEL_LIGHTING_INV_ATLAS_WIDTH, MODEL_LIGHTING_VOLUME_W, model_lighting_inv_image_height,
        model_lighting_lookup_scale,
    };
    let inv_h = model_lighting_inv_image_height(lighting.dims.image_height)?;
    let scale = model_lighting_lookup_scale(inv_h);
    Some(SmodelPassMaterial {
        model_lighting_required: true,
        color: None,
        specular: None,
        probe: None,
        atlas: Some(lighting.image.clone()),
        alpha_mode: AlphaMode::Opaque,
        draw_mode: None,
        cull_mode: None,
        env_map_parms: [0.0; 4],
        lighting_lookup_scale: [scale.u, scale.v, scale.w, scale.q],
        atlas_lookup: [
            MODEL_LIGHTING_INV_ATLAS_WIDTH as f32,
            inv_h,
            MODEL_LIGHTING_VOLUME_W,
            0.0,
        ],
        sort_key: 0,
        material_sorted_index: Some(material_sorted_index),
    })
}

pub struct BodyPackedSession {
    packed_ok: bool,
    packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
}

impl BodyPackedSession {
    pub fn reserve_packed(&mut self, n: usize) {
        self.packed.reserve(n);
    }
}

pub fn take_body_packed_session(plan: &mut RemoteBodyDrawPlan) -> BodyPackedSession {
    let vertices_empty = plan.decoded_n == 0 && plan.vertices.is_empty();
    let packed_ok = vertices_empty
        || matches!(
            plan.packed_vertices,
            asset_world::PackedVertexPayload::Iw4(_)
        );
    let packed = match std::mem::replace(
        &mut plan.packed_vertices,
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: BODY_PACKED_UNAVAILABLE,
        },
    ) {
        asset_world::PackedVertexPayload::Iw4(rows) => rows,
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    BodyPackedSession { packed_ok, packed }
}

pub fn install_body_packed_session(plan: &mut RemoteBodyDrawPlan, session: BodyPackedSession) {
    plan.packed_vertices = install_retained_packed(
        session.packed_ok,
        session.packed,
        plan.decoded_n,
        XMODEL_PACKED_EMPTY_PLAN,
        BODY_PACKED_UNAVAILABLE,
    );
}

pub fn append_remote_body_cpu_blob(
    plan: &mut RemoteBodyDrawPlan,
    session: &mut BodyPackedSession,
    packed_rows: &[[u8; asset_iw4::size::GFX_PACKED_VERTEX]],
    indices: &[u32],
    decoded_n: usize,
) -> Option<(u32, u32)> {
    if decoded_n == 0 || indices.is_empty() {
        return None;
    }
    let vert_base = plan.decoded_n as u32;
    let index_base = plan.indices.len() as u32;
    plan.decoded_n = plan.decoded_n.saturating_add(decoded_n);
    plan.indices
        .extend(indices.iter().map(|&index| vert_base + index));
    if session.packed_ok && packed_rows.len() == decoded_n {
        session.packed.extend_from_slice(packed_rows);
    } else {
        session.packed_ok = false;
        session.packed.clear();
    }
    Some((vert_base, index_base))
}

pub fn push_remote_body_cpu_draw(
    plan: &mut RemoteBodyDrawPlan,
    (index_start, index_count): (u32, u32),
    item: &RemoteBodySkinnedItem,
    lighting_handle: u32,
    scene_light_index: u8,
    reflection_probe_index: u8,
    material: u32,
) {
    if lighting_handle == 0 || index_count == 0 {
        return;
    }
    let range_idx = plan.surface_ranges.len() as u32;
    plan.surface_ranges.push((index_start, index_count));
    plan.draws.push(RemoteBodySurfaceDraw {
        surface: range_idx,
        material,
        world_from_local: item.world_from_local,
        lighting_handle,
        scene_light_index,
        reflection_probe_index,
        scene_entnum: Some(item.client),
    });
}

pub fn finish_remote_body_draw_plan(plan: &mut RemoteBodyDrawPlan) {
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.decoded_n);
    if plan.revisions.topology != topology {
        plan.revisions.topology = topology;
    }
}

pub fn append_script_model_asset(
    plan: &mut ScriptModelDrawPlan,
    key: asset_world::MapXModelAssetKey,
    dobj_state: xmodel_runtime::DObjSemanticState,
    camera_lods: Vec<Option<u8>>,
    surfaces: &[PosedSmodelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> usize {
    let mut packed = match std::mem::take(&mut plan.rows.packed_vertices) {
        asset_world::PackedVertexPayload::Iw4(rows) => rows,
        asset_world::PackedVertexPayload::Unavailable { .. } => Vec::new(),
    };
    let mut rows = Vec::new();
    let mut gpu_surfaces = Vec::new();
    for (source_surface, (surface, material)) in surfaces.iter().zip(materials).enumerate() {
        let Some(material) = material else { continue };
        if surface.vert_n == 0
            || surface.indices.is_empty()
            || surface
                .indices
                .iter()
                .any(|&index| index as usize >= surface.vert_n)
        {
            continue;
        }
        assert_eq!(
            surface.packed_vertices.len(),
            surface.vert_n,
            "script packed vertex count"
        );
        let first_vertex = packed.len() as u32;
        let first_index = plan.rows.indices.len() as u32;
        packed.extend_from_slice(&surface.packed_vertices);
        plan.rows
            .indices
            .extend(surface.indices.iter().map(|index| first_vertex + index));
        gpu_surfaces.push(crate::draw::ScriptGpuPlacement {
            source_surface,
            first_vertex,
            vertex_count: surface.vert_n as u32,
        });
        rows.push((
            plan.rows.surface_ranges.len() as u32,
            plan.rows.materials.len() as u32,
        ));
        plan.rows
            .surface_ranges
            .push((first_index, surface.indices.len() as u32));
        plan.rows.materials.push(material.clone());
    }
    plan.rows.packed_vertices = if packed.is_empty() {
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: XMODEL_PACKED_EMPTY_PLAN,
        }
    } else {
        asset_world::PackedVertexPayload::Iw4(packed)
    };
    plan.assets.push(ScriptModelAssetDraw {
        gpu_geometry_id: 0,
        gpu_surfaces,
        key,
        dobj_state,
        camera_lods,
        surfaces: rows,
    });
    plan.revisions.bump_surfaces();
    plan.geometry_dirty = true;
    plan.assets.len() - 1
}

pub fn retain_script_model_assets(plan: &mut ScriptModelDrawPlan, keep: &[bool]) {
    debug_assert_eq!(keep.len(), plan.assets.len());
    if keep.iter().all(|k| *k) {
        return;
    }
    let packed_src = match &plan.rows.packed_vertices {
        asset_world::PackedVertexPayload::Iw4(rows) => Some(rows.as_slice()),
        asset_world::PackedVertexPayload::Unavailable { .. } => None,
    };
    let mut next = ScriptModelDrawPlan::default();

    next.rows.indices.reserve(plan.rows.indices.len());
    next.rows
        .surface_ranges
        .reserve(plan.rows.surface_ranges.len());
    next.rows.materials.reserve(plan.rows.materials.len());
    next.assets.reserve(plan.assets.len());
    let mut packed_dst = Vec::with_capacity(packed_src.map_or(0, <[_]>::len));
    let mut packed_ok = packed_src.is_some();
    for (asset_index, asset) in plan.assets.iter().enumerate() {
        if !keep.get(asset_index).copied().unwrap_or(false) {
            continue;
        }
        let mut new_surfaces = Vec::new();
        let mut gpu_surfaces = Vec::new();
        for (placement_index, &(surf_i, mat_i)) in asset.surfaces.iter().enumerate() {
            let Some(&(index_start, count)) = plan.rows.surface_ranges.get(surf_i as usize) else {
                continue;
            };
            let start = index_start as usize;
            let end = start.saturating_add(count as usize);
            let Some(slice) = plan.rows.indices.get(start..end) else {
                continue;
            };
            let placement = &asset.gpu_surfaces[placement_index];
            let vmin = placement.first_vertex;
            let vertex_count = placement.vertex_count;
            let vbase = packed_dst.len() as u32;
            if packed_ok && let Some(src) = packed_src {
                if let Some(rows) = src.get(vmin as usize..vmin as usize + vertex_count as usize) {
                    packed_dst.extend_from_slice(rows);
                } else {
                    packed_ok = false;
                    packed_dst.clear();
                }
            }
            let index_start_new = next.rows.indices.len() as u32;
            next.rows
                .indices
                .extend(slice.iter().map(|index| vbase + (*index - vmin)));
            let surface_index = next.rows.surface_ranges.len() as u32;
            next.rows.surface_ranges.push((index_start_new, count));
            let Some(material) = plan.rows.materials.get(mat_i as usize) else {
                continue;
            };
            let material_index = next.rows.materials.len() as u32;
            next.rows.materials.push(material.clone());
            new_surfaces.push((surface_index, material_index));
            gpu_surfaces.push(crate::draw::ScriptGpuPlacement {
                source_surface: asset
                    .gpu_surfaces
                    .get(placement_index)
                    .map_or(placement_index, |p| p.source_surface),
                first_vertex: vbase,
                vertex_count,
            });
        }
        next.assets.push(ScriptModelAssetDraw {
            gpu_geometry_id: asset.gpu_geometry_id,
            gpu_surfaces,
            key: asset.key.clone(),
            dobj_state: asset.dobj_state.clone(),
            camera_lods: asset.camera_lods.clone(),
            surfaces: new_surfaces,
        });
    }
    next.rows.packed_vertices = if packed_ok && !packed_dst.is_empty() {
        asset_world::PackedVertexPayload::Iw4(packed_dst)
    } else {
        asset_world::PackedVertexPayload::Unavailable {
            source_layout: if packed_ok {
                XMODEL_PACKED_EMPTY_PLAN
            } else {
                XMODEL_PACKED_UNAVAILABLE
            },
        }
    };

    // Carry the plan's revisions across the retain: a retain reshuffles surface
    // indices, it does not make this a different plan, and a revision that
    // restarts at zero is one a consumer can mistake for the one it last saw.
    next.revisions = plan.revisions;
    next.revisions.bump_surfaces();
    next.generation = plan.generation.wrapping_add(1);
    next.revision = plan.revision;
    next.revisions.bump_admission();
    next.geometry_dirty = true;
    *plan = next;
}

pub fn finish_script_model_draw_plan(plan: &mut ScriptModelDrawPlan) {
    if !std::mem::take(&mut plan.geometry_dirty) {
        return;
    }
    let _scope = perf::span!("script_model_geometry_publish", "iw4l.render");
    let topology = plan.rows.topology();
    if plan.revisions.topology != topology {
        plan.revisions.topology = topology;
        plan.revisions.bump_draws();
    }
    plan.revision = plan.revision.wrapping_add(1);
    perf::instant!("script_model_geometry_publication", "iw4l.render", |ctx| {
        ctx.arg("assets", plan.assets.len() as u64);
        ctx.arg("vertices", plan.vertex_count() as u64);
        ctx.arg("indices", plan.rows.indices.len() as u64);
    });
}

pub fn append_missile_surfaces(
    plan: &mut MissileDrawPlan,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let surfaces = plan.rows.append_posed(surfaces, materials);
    plan.revisions.bump_surfaces();
    surfaces
}

pub fn append_item_surfaces(
    plan: &mut ItemDrawPlan,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let surfaces = plan.rows.append_posed(surfaces, materials);
    plan.revisions.bump_surfaces();
    surfaces
}

pub fn append_dynent_surfaces(
    plan: &mut DynEntDrawPlan,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let surfaces = plan.rows.append_posed(surfaces, materials);
    plan.revisions.bump_surfaces();
    surfaces
}

pub fn append_dynent_asset(
    plan: &mut DynEntDrawPlan,
    key: asset_world::MapXModelAssetKey,
    camera_lod: Option<u8>,
    surfaces: &[PosedModelSurface],
    materials: &[Option<SmodelPassMaterial>],
) -> Vec<(u32, u32)> {
    let asset_surfaces = append_dynent_surfaces(plan, surfaces, materials);
    plan.assets.push(DynEntAssetDraw {
        key,
        camera_lod,
        surfaces: asset_surfaces.clone(),
    });
    asset_surfaces
}

/// Publish the rows a prepared rig settled: its indices, its surface ranges,
/// its materials and its draws, plus a packed vertex buffer sized to the layout
/// it laid out. This runs when the composition changes. A pose writes into the
/// buffer below and touches nothing else here.
pub fn install_prepared_fpv_plan(
    plan: &mut FpvDrawPlan,
    geometry: &crate::anim::fpv_rig::PreparedFpvGeometry,
    camo: Option<&std::collections::HashMap<usize, SmodelPassMaterial>>,
    lighting_handle: u32,
) {
    geometry.write_indices(&mut plan.indices);
    plan.surface_ranges.clear();
    plan.surface_ranges
        .extend_from_slice(&geometry.surface_ranges);
    plan.materials.clear();
    plan.materials.extend(
        geometry
            .materials
            .iter()
            .zip(&geometry.material_authored)
            .map(|(material, authored)| {
                camo.and_then(|swaps| swaps.get(authored))
                    .unwrap_or(material)
                    .clone()
            }),
    );
    plan.draws.clear();
    plan.draws.extend_from_slice(&geometry.draws);
    plan.decoded_n = geometry.dest_n;
    plan.lighting_handle = lighting_handle;
    plan.geometry_ok = false;
    plan.packed_vertices = install_retained_packed(
        geometry.packed_ok,
        vec![[0u8; asset_iw4::size::GFX_PACKED_VERTEX]; geometry.dest_n],
        geometry.dest_n,
        FPV_PACKED_EMPTY_PLAN,
        FPV_PACKED_UNAVAILABLE,
    );
    plan.hands_plan_n = Some(geometry.hands_plan_n);
    plan.gun_plan_n = Some(geometry.gun_plan_n);
    plan.scope_plan_n = Some(geometry.scope_plan_n);
    plan.scope_house_plan_n = Some(geometry.scope_house_plan_n);
    plan.scope_lens_plan_n = Some(geometry.scope_lens_plan_n);
    plan.plan_draw_n = Some(geometry.plan_draw_n);
    plan.plan_skip_n = Some(geometry.plan_skip_n);
    plan.revisions.bump_surfaces();
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.decoded_n);
    let revision = plan.revision;
    plan.revision = stamp_plan_geometry(&mut plan.revisions, revision, topology);
}

/// Nothing to draw. A plan that was already empty publishes no new revision:
/// a hidden viewmodel is not a reason for the merge downstream to re-copy a
/// vertex bank every frame.
pub fn clear_fpv_draw_plan(plan: &mut FpvDrawPlan, lighting_handle: u32) {
    plan.skin_jobs.clear();
    let was_empty = plan.draws.is_empty() && plan.decoded_n == 0;
    plan.lighting_handle = lighting_handle;
    plan.geometry_ok = false;
    plan.settle_visible();
    // The rows are gone, so the rig that published them has to publish them
    // again before this plan draws anything: a viewmodel hidden without a
    // respawn behind it comes back to the same composition, not to an empty
    // plan.
    plan.rig_generation = 0;
    if was_empty {
        return;
    }
    plan.indices.clear();
    plan.surface_ranges.clear();
    plan.materials.clear();
    plan.draws.clear();
    plan.decoded_n = 0;
    plan.packed_vertices = asset_world::PackedVertexPayload::Unavailable {
        source_layout: FPV_PACKED_EMPTY_PLAN,
    };
    plan.hands_plan_n = None;
    plan.gun_plan_n = None;
    plan.scope_plan_n = Some(0);
    plan.scope_house_plan_n = Some(0);
    plan.scope_lens_plan_n = Some(0);
    plan.plan_draw_n = Some(0);
    plan.plan_skip_n = Some(0);
    plan.revisions.bump_surfaces();
    let topology = topology_fingerprint(&plan.indices, &plan.surface_ranges, 0);
    let revision = plan.revision;
    plan.revision = stamp_plan_geometry(&mut plan.revisions, revision, topology);
}
