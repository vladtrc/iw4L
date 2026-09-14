use std::collections::HashSet;
use std::sync::Arc;

use asset_iw4::size::GFX_PACKED_VERTEX;
use bevy::prelude::*;
use fx_iw4::{
    FX_GLASS_STATE_FLAG_SHATTERED, fx_glass_def_color_rgba, fx_glass_init_origin,
    fx_glass_intact_fan_indices, fx_glass_intact_verts, fx_glass_place_origin, fx_glass_place_quat,
    fx_glass_state_def_index, fx_glass_state_flags, fx_glass_state_init_index,
    fx_glass_state_vert_count, fx_pack_code_mesh_vertex, fx_trail_pack_normal,
    fx_trail_pack_texcoord, fx_unit_quat_to_axis,
};

use entity_iw4::{
    CG_GLASS_PIECE_LIMIT, CgGlassApplyAction, CgGlassPiece, GlassPaneBasis, cg_glass_apply_state,
    cg_glass_read_change,
};

use super::fx::FxPassMaterial;
use crate::adapters::anim::dyn_ent::{fpv_frustum_planes, sphere_behind_frustum};
use crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog;
use crate::prepare::scene::camera::FpvLens;
use crate::prepare::scene::model_lighting_cache::{
    ModelLightingOwner, ModelLightingRequest, ModelLightingRequests, ResolvedModelLighting,
    ResolvedModelLightingTable, WorldModelLightingCache,
};
use crate::prepare::scene::world::WorldScene;
use render_frame::RetailPackedVertexRefusal;

pub(crate) const GLASS_FRUSTUM_LIGHT_RADIUS: f32 = 64.0;

pub const GFX_GLASS_SURF_LIMIT: usize = 0x300;

pub const GFX_GLASS_MESH_VERT_LIMIT: usize = 0x4800;

pub const GFX_GLASS_MESH_INDEX_LIMIT: usize = 0x2100;

#[derive(Clone, Copy, Debug)]
pub struct GfxGlassMeshDraw {
    pub material: u32,
    pub index_start: u32,
    pub index_count: u32,

    pub lighting_handle: u32,

    pub lighting_prev: u32,

    pub pending_lighting: Option<ModelLightingRequest>,

    pub init_index: u16,

    pub origin: [f32; 3],

    pub reflection_probe_index: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlassLightingRuntime {
    pub attempted: u32,
    pub assigned: u32,
    pub reused: u32,
    pub failed: u32,
    pub nonzero: u32,
    pub unique: u32,
    pub sample: Option<u32>,
    pub probe_sample: Option<u8>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct GfxGlassMeshPlan {
    pub vertices: Arc<Vec<[u8; GFX_PACKED_VERTEX]>>,
    pub indices: Arc<Vec<u32>>,
    pub materials: Vec<FxPassMaterial>,
    pub draws: Vec<GfxGlassMeshDraw>,
    pub revision: u64,
    pub built: bool,
    pub skip_why: Option<&'static str>,
    pub skipped_vert: u32,
    pub skipped_def: u32,
    pub skipped_name: u32,
    pub skipped_ordinal: u32,
    pub skipped_cap: u32,
    pub skipped_shatter: u32,

    pub weaken_n: u32,

    pub lighting_handle_n: u32,
    pub lighting_handle_nonzero: u32,

    pub lighting_handle_sample: Option<u16>,

    pub lighting_handle_max: Option<u16>,

    pub lighting_runtime: Option<GlassLightingRuntime>,

    pub applied: Vec<(u32, u8)>,
    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
}

#[derive(Resource, Clone, Debug)]
pub struct CgGlassTable {
    pub rows: Vec<CgGlassPiece>,
}

impl Default for CgGlassTable {
    fn default() -> Self {
        Self {
            rows: vec![CgGlassPiece::default(); CG_GLASS_PIECE_LIMIT],
        }
    }
}

impl CgGlassTable {
    pub fn reset(&mut self) {
        self.rows.fill(CgGlassPiece::default());
    }

    pub fn applied_pairs(&self) -> Vec<(u32, u8)> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.applied != 0)
            .map(|(i, row)| (i as u32, row.applied))
            .collect()
    }
}

impl GfxGlassMeshPlan {
    fn verts_mut(&mut self) -> &mut Vec<[u8; GFX_PACKED_VERTEX]> {
        Arc::make_mut(&mut self.vertices)
    }

    fn inds_mut(&mut self) -> &mut Vec<u32> {
        Arc::make_mut(&mut self.indices)
    }

    pub fn clear(&mut self) {
        super::reset_rows(&mut self.vertices);
        super::reset_rows(&mut self.indices);
        self.range_share = None;
        self.materials.clear();
        self.draws.clear();
        self.skip_why = None;
        self.skipped_vert = 0;
        self.skipped_def = 0;
        self.skipped_name = 0;
        self.skipped_ordinal = 0;
        self.skipped_cap = 0;
        self.skipped_shatter = 0;
        self.weaken_n = 0;
        self.lighting_handle_n = 0;
        self.lighting_handle_nonzero = 0;
        self.lighting_handle_sample = None;
        self.lighting_handle_max = None;
        self.lighting_runtime = None;
    }

    pub fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn publish_share(&mut self) {
        self.range_share = Some(super::publish_index_ranges(
            self.draws
                .iter()
                .map(|draw| (draw.index_start, draw.index_count)),
        ));
    }

    pub fn packed_rows(&self) -> &[[u8; GFX_PACKED_VERTEX]] {
        self.vertices.as_slice()
    }

    pub fn index_rows(&self) -> &[u32] {
        self.indices.as_slice()
    }

    pub fn exact_packed_vertices(
        &self,
    ) -> Result<&[[u8; GFX_PACKED_VERTEX]], RetailPackedVertexRefusal> {
        let table_stride =
            asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::PACKED_VERTEX_TYPE, 0);
        if table_stride != Some(GFX_PACKED_VERTEX as u16) {
            return Err(RetailPackedVertexRefusal::RetailStrideMismatch { table_stride });
        }
        Ok(self.packed_rows())
    }

    pub fn build_intact(
        &mut self,
        glass: &assets::FxGlassReset,
        catalog: &RuntimeMaterialCatalog,
        colors: &std::collections::HashMap<usize, Handle<Image>>,
        applied: &[(u32, u8)],
        host: Option<&fx::FxGlassSystemHost>,
    ) {
        self.clear();
        self.built = true;
        self.applied = applied.to_vec();
        self.lighting_handle_n = glass.lighting_handles.len() as u32;
        self.lighting_handle_nonzero =
            glass.lighting_handles.iter().filter(|h| **h != 0).count() as u32;
        self.lighting_handle_sample = glass.lighting_handles.get(78).copied();
        self.lighting_handle_max = glass.lighting_handles.iter().copied().max();
        let mut last_why = None;
        if let Some(host) = host.filter(|h| h.piece_limit > 0) {
            for piece in 0..host.piece_limit as usize {
                if !host.is_in_use(piece as u32) {
                    continue;
                }
                let Some(place) = host.piece_places.get(piece) else {
                    continue;
                };
                let Some(state) = host.piece_states.get(piece) else {
                    continue;
                };
                self.emit_glass_piece(
                    piece,
                    place,
                    state,
                    &host.geo_data,
                    glass,
                    catalog,
                    colors,
                    applied,
                    false,
                    &mut last_why,
                );
            }
        } else {
            for (piece, (place, state)) in glass
                .piece_places
                .iter()
                .zip(glass.piece_states.iter())
                .enumerate()
            {
                self.emit_glass_piece(
                    piece,
                    place,
                    state,
                    &glass.geo_data,
                    glass,
                    catalog,
                    colors,
                    applied,
                    true,
                    &mut last_why,
                );
            }
        }
        if self.draws.is_empty() {
            self.skip_why = last_why.or(Some("empty"));
        }
        self.bump();
        self.publish_share();
    }

    fn emit_glass_piece(
        &mut self,
        piece: usize,
        place: &[u8; fx_iw4::FX_GLASS_PIECE_PLACE],
        state: &[u8; fx_iw4::FX_GLASS_PIECE_STATE],
        geo: &[[u8; fx_iw4::FX_GLASS_GEOMETRY_DATA]],
        glass: &assets::FxGlassReset,
        catalog: &RuntimeMaterialCatalog,
        colors: &std::collections::HashMap<usize, Handle<Image>>,
        applied: &[(u32, u8)],
        drop_snapshot_shatter: bool,
        last_why: &mut Option<&'static str>,
    ) {
        if self.draws.len() >= GFX_GLASS_SURF_LIMIT {
            self.skipped_cap = self.skipped_cap.saturating_add(1);
            *last_why = Some("surf_limit");
            return;
        }
        let applied_state = applied_glass_state(applied, piece as u32);
        if drop_snapshot_shatter && applied_state >= 2 {
            self.skipped_shatter = self.skipped_shatter.saturating_add(1);
            *last_why = Some("shattered");
            return;
        }
        let vert_n = fx_glass_state_vert_count(state);
        if vert_n < 3 {
            self.skipped_vert = self.skipped_vert.saturating_add(1);
            *last_why = Some("vert_count");
            return;
        }
        let def_i = usize::from(fx_glass_state_def_index(state));
        let Some(def) = glass.defs.get(def_i) else {
            self.skipped_def = self.skipped_def.saturating_add(1);
            *last_why = Some("missing_def");
            return;
        };
        let flags = fx_glass_state_flags(state);
        let use_shattered = applied_state == 1 || (flags & FX_GLASS_STATE_FLAG_SHATTERED) != 0;
        let Some(edge) = glass.material_edge(def_i, use_shattered) else {
            self.skipped_name = self.skipped_name.saturating_add(1);
            *last_why = Some("missing_material_edge");
            return;
        };
        let Some(asset_id) = edge.bound() else {
            self.skipped_name = self.skipped_name.saturating_add(1);
            *last_why = Some(edge.edge_kind());
            return;
        };
        let Some(ordinal) = catalog.ordinal_for_asset_id(asset_id) else {
            self.skipped_ordinal = self.skipped_ordinal.saturating_add(1);
            *last_why = Some("no_ordinal");
            return;
        };
        let sort_key = catalog
            .derived(asset_id)
            .and_then(|m| m.baked_draw_surf)
            .map(|packed| dpvs_iw4::unpack(dpvs_iw4::GfxDrawSurf { packed }).primary_sort_key)
            .unwrap_or(0);
        let color = colors.get(&asset_id.order()).cloned();
        let mut cpu = vec![
            fx_iw4::FxGlassIntactVert {
                xyz: [0.0; 3],
                uv: [0.0; 2],
            };
            usize::from(vert_n)
        ];
        let Some(wrote) = fx_glass_intact_verts(place, state, geo, def, &mut cpu) else {
            self.skipped_vert = self.skipped_vert.saturating_add(1);
            *last_why = Some("geo_trunc");
            return;
        };
        let mut fan = [0u16; 3 * 254];
        let Some(idx_n) = fx_glass_intact_fan_indices(vert_n, &mut fan) else {
            self.skipped_vert = self.skipped_vert.saturating_add(1);
            *last_why = Some("fan");
            return;
        };
        if self.vertices.len() + wrote > GFX_GLASS_MESH_VERT_LIMIT
            || self.indices.len() + idx_n > GFX_GLASS_MESH_INDEX_LIMIT
        {
            self.skipped_cap = self.skipped_cap.saturating_add(1);
            *last_why = Some("mesh_limit");
            return;
        }
        let axis = fx_unit_quat_to_axis(fx_glass_place_quat(place));
        let normal = fx_trail_pack_normal(axis[2]);
        let tangent = fx_trail_pack_normal(axis[0]);
        let color_rgba = fx_glass_def_color_rgba(def);
        let base = self.vertices.len() as u32;
        for v in cpu.iter().take(wrote) {
            self.verts_mut().push(fx_pack_code_mesh_vertex(
                v.xyz,
                color_rgba,
                fx_trail_pack_texcoord(v.uv[0], v.uv[1]),
                normal,
                tangent,
            ));
        }
        let index_start = self.indices.len() as u32;
        for &idx in fan.iter().take(idx_n) {
            self.inds_mut().push(base.saturating_add(u32::from(idx)));
        }
        let material = self.materials.len() as u32;
        self.materials.push(FxPassMaterial {
            color,
            sort_key,
            material_sorted_index: Some(ordinal.get()),
        });
        let init_index = fx_glass_state_init_index(state);
        let origin = glass
            .init_piece_states
            .get(usize::from(init_index))
            .map(fx_glass_init_origin)
            .unwrap_or_else(|| fx_glass_place_origin(place));
        self.draws.push(GfxGlassMeshDraw {
            material,
            index_start,
            index_count: idx_n as u32,
            lighting_handle: u32::from(
                glass
                    .lighting_handles
                    .get(usize::from(init_index))
                    .copied()
                    .unwrap_or(0),
            ),
            lighting_prev: 0,
            pending_lighting: None,
            init_index,
            origin,
            reflection_probe_index: 0,
        });
        if applied_state == 1 {
            self.weaken_n = self.weaken_n.saturating_add(1);
        }
    }
}

fn applied_glass_state(applied: &[(u32, u8)], piece: u32) -> u8 {
    applied
        .iter()
        .find(|(id, _)| *id == piece)
        .map(|(_, state)| *state)
        .unwrap_or(0)
}

fn glass_snapshot_rows(
    authority: Option<&net::AuthorityWorld>,
    adopted: Option<&net::LastAdoptedSnapshot>,
) -> Vec<(
    u32,
    entity_iw4::GlassPieceState,
    Option<entity_iw4::GlassShatterSeed>,
)> {
    if let Some(world) = authority {
        return world
            .0
            .world_objects()
            .to_snapshot()
            .glass_pieces
            .into_iter()
            .map(|(id, row)| (id, row.state, row.shatter_seed))
            .collect();
    }
    adopted
        .and_then(|snap| snap.next())
        .map(|snap| {
            snap.meta
                .world_objects
                .glass_pieces
                .iter()
                .map(|(id, row)| (*id, row.state, row.shatter_seed))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn apply_cg_glass_tess(
    authority: Option<Res<net::AuthorityWorld>>,
    adopted: Option<Res<net::LastAdoptedSnapshot>>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
    colors: Option<Res<render_fx::FxWorldColorImages>>,
    world_plan: Option<Res<super::world::WorldDrawGpuPlan>>,
    mut plan: ResMut<GfxGlassMeshPlan>,
    mut table: ResMut<CgGlassTable>,
    mut fx_host: Option<ResMut<render_fx::HostFxSystem>>,
) {
    let Some(scene) = scene else {
        return;
    };
    if !plan.built {
        if world_plan.is_none() {
            return;
        }
        table.reset();
        let empty = std::collections::HashMap::new();
        let colors = colors
            .as_ref()
            .map(|imgs| &imgs.colors_by_asset)
            .unwrap_or(&empty);
        match scene.fx_glass.as_ref() {
            Some(glass) => plan.build_intact(
                glass,
                &scene.runtime_material_catalog,
                colors,
                &[],
                fx_host.as_deref().map(|h| &h.0.glass),
            ),
            None => {
                plan.built = true;
                plan.skip_why = Some("no_fx_glass");
                plan.bump();
                plan.publish_share();
            }
        }
        diag::info!(
            World,
            "glass mesh tess plan: surfs={} verts={} indices={} skip_why={} skipped_vert={} skipped_def={} skipped_name={} skipped_ordinal={} skipped_cap={} skipped_shatter={} (intact fan; vis_cull=none; not T5 0xF)",
            plan.draws.len(),
            plan.packed_rows().len(),
            plan.index_rows().len(),
            plan.skip_why.unwrap_or("-"),
            plan.skipped_vert,
            plan.skipped_def,
            plan.skipped_name,
            plan.skipped_ordinal,
            plan.skipped_cap,
            plan.skipped_shatter,
        );
        return;
    }
    let Some(glass) = scene.fx_glass.as_ref() else {
        return;
    };
    for (id, state, seed) in glass_snapshot_rows(authority.as_deref(), adopted.as_deref()) {
        let Some(row) = table.rows.get_mut(id as usize) else {
            continue;
        };
        cg_glass_read_change(row, state, seed);
    }
    let mut host_mutated = false;
    for (i, row) in table.rows.iter_mut().enumerate() {
        if row.applied >= row.pending {
            continue;
        }
        let pane = glass
            .pane_basis(i)
            .map(|(origin, axis_s, axis_t)| GlassPaneBasis {
                origin,
                axis_s,
                axis_t,
            });
        match cg_glass_apply_state(row, pane) {
            CgGlassApplyAction::Delete => {
                if let Some(host) = fx_host.as_mut() {
                    host.0.glass.free(i as u32);
                    host_mutated = true;
                }
            }
            CgGlassApplyAction::Shatter { hit, dir, .. } => {
                if let Some(host) = fx_host.as_mut() {
                    host_mutated |= host.0.glass.shatter(i as u32, hit, dir);
                }
            }
            _ => {}
        }
    }
    let applied = table.applied_pairs();
    let glass_moved = fx_host.as_ref().is_some_and(|h| h.0.glass.moved);
    if applied == plan.applied && !host_mutated && !glass_moved {
        return;
    }
    let empty = std::collections::HashMap::new();
    let colors = colors
        .as_ref()
        .map(|imgs| &imgs.colors_by_asset)
        .unwrap_or(&empty);
    plan.build_intact(
        glass,
        &scene.runtime_material_catalog,
        colors,
        &applied,
        fx_host.as_deref().map(|h| &h.0.glass),
    );
    if let Some(host) = fx_host.as_mut() {
        host.0.glass.moved = false;
    }
}

pub(crate) fn enqueue_glass_model_lighting(
    cache: Option<Res<WorldModelLightingCache>>,
    scene: Option<Res<WorldScene>>,
    cameras: Query<(&GlobalTransform, &Projection, &Camera), With<FpvLens>>,
    mut requests: ResMut<ModelLightingRequests>,
    mut plan: ResMut<GfxGlassMeshPlan>,
) {
    if !plan.built {
        return;
    }
    let attempted = plan.draws.len() as u32;
    plan.lighting_runtime = None;
    let Some(cache) = cache else {
        stamp_glass_lighting_failed(&mut plan, attempted);
        return;
    };
    let box_half = lighting_iw4::lighting_query_box_half(1.0);
    let planes = fpv_frustum_planes(&cameras);
    for draw in plan.draws.iter_mut() {
        let owner = ModelLightingOwner::Glass(draw.init_index);
        draw.lighting_prev = u32::from(cache.handle_for(owner));
        draw.pending_lighting = None;
        if sphere_behind_frustum(draw.origin, GLASS_FRUSTUM_LIGHT_RADIUS, &planes) {
            draw.lighting_handle = 0;
            draw.reflection_probe_index = 0;
            continue;
        }
        let lookup_fallback = scene
            .as_ref()
            .map(|s| s.dyn_atpoint_lookup_fallback(draw.origin, Some(box_half)))
            .unwrap_or(lighting_iw4::LIGHT_GRID_ATPOINT_EMPTY_PRIMARY);
        draw.pending_lighting = Some(requests.request(ModelLightingRequest {
            owner,
            origin: draw.origin,
            lookup_fallback,
        }));
    }
}

pub(crate) fn apply_glass_model_lighting(
    resolved: Res<ResolvedModelLightingTable>,
    mut plan: ResMut<GfxGlassMeshPlan>,
) {
    if !plan.built || plan.lighting_runtime.is_some() {
        return;
    }
    let attempted = plan.draws.len() as u32;
    let mut assigned = 0u32;
    let mut reused = 0u32;
    let mut failed = 0u32;
    let mut unique = HashSet::new();
    let mut sample = None;
    let mut probe_sample = None;
    for draw in plan.draws.iter_mut() {
        let Some(request) = draw.pending_lighting.take() else {
            if draw.init_index == 78 {
                sample = Some(draw.lighting_handle);
                probe_sample = Some(draw.reflection_probe_index);
            }
            continue;
        };
        match resolved.get(request.owner) {
            Some(ResolvedModelLighting::Seated {
                handle,
                reflection_probe_index,
                ..
            }) => {
                if draw.lighting_prev == handle {
                    reused = reused.saturating_add(1);
                } else {
                    assigned = assigned.saturating_add(1);
                }
                draw.lighting_handle = handle;
                draw.reflection_probe_index = reflection_probe_index;
                unique.insert(handle);
            }
            Some(ResolvedModelLighting::Failed) | None => {
                failed = failed.saturating_add(1);
                draw.lighting_handle = 0;
                draw.reflection_probe_index = 0;
            }
        }
        if draw.init_index == 78 {
            sample = Some(draw.lighting_handle);
            probe_sample = Some(draw.reflection_probe_index);
        }
    }
    let nonzero = plan.draws.iter().filter(|d| d.lighting_handle != 0).count() as u32;
    plan.lighting_runtime = Some(GlassLightingRuntime {
        attempted,
        assigned,
        reused,
        failed,
        nonzero,
        unique: unique.len() as u32,
        sample,
        probe_sample,
    });
}

fn stamp_glass_lighting_failed(plan: &mut GfxGlassMeshPlan, attempted: u32) {
    let sample = plan
        .draws
        .iter()
        .find(|d| d.init_index == 78)
        .map(|d| d.lighting_handle);
    let probe_sample = plan
        .draws
        .iter()
        .find(|d| d.init_index == 78)
        .map(|d| d.reflection_probe_index);
    plan.lighting_runtime = Some(GlassLightingRuntime {
        attempted,
        assigned: 0,
        reused: 0,
        failed: attempted,
        nonzero: 0,
        unique: 0,
        sample,
        probe_sample,
    });
}
