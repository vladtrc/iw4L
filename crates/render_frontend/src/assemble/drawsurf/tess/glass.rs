use std::collections::HashSet;
use std::sync::Arc;

use asset_iw4::size::GFX_PACKED_VERTEX;
use bevy::prelude::*;
use fx_iw4::{
    FX_GLASS_SHARD_LIFETIME_MSEC, FX_GLASS_SHARD_VERT_MAX, FX_GLASS_STATE_FLAG_DAMAGED,
    fx_glass_decode_geo, fx_glass_def_color_rgba, fx_glass_emit_slab, fx_glass_piece_verts,
    fx_glass_place_origin, fx_glass_place_quat, fx_glass_scale_color_alpha, fx_glass_slab_counts,
    fx_glass_state_def_index, fx_glass_state_flags, fx_glass_state_init_index,
    fx_glass_state_vert_count, fx_pack_code_mesh_vertex_signed, fx_trail_pack_normal,
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

pub const GFX_GLASS_MESH_VERT_LIMIT: usize = 0xC000;

pub const GFX_GLASS_MESH_INDEX_LIMIT: usize = 0xA000;

#[derive(Clone, Copy, Debug)]
pub struct GfxGlassMeshDraw {
    pub material: u32,
    pub index_start: u32,
    pub index_count: u32,

    pub lighting_handle: u32,

    pub lighting_prev: u32,

    pub pending_lighting: Option<ModelLightingRequest>,

    pub init_index: u16,

    pub piece: u16,

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
    pub last_msec: i32,
    pub generation: u32,
    pub map_round_epoch: u32,
}

impl Default for CgGlassTable {
    fn default() -> Self {
        Self {
            rows: vec![CgGlassPiece::default(); CG_GLASS_PIECE_LIMIT],
            last_msec: 0,
            generation: 0,
            map_round_epoch: 0,
        }
    }
}

impl CgGlassTable {
    pub fn reset(&mut self) {
        self.rows.fill(CgGlassPiece::default());
        self.last_msec = 0;
        self.generation = self.generation.wrapping_add(1);
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
                let Some(place) = host.draw_place(piece) else {
                    continue;
                };
                let Some(state) = host.piece_states.get(piece) else {
                    continue;
                };
                self.emit_glass_piece(
                    piece,
                    &place,
                    state,
                    &host.geo_data,
                    glass,
                    catalog,
                    colors,
                    applied,
                    false,
                    host.half_thickness.get(piece).copied().unwrap_or(0.0),
                    host.piece_fade(piece),
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
                    glass
                        .half_thickness
                        .get(piece)
                        .copied()
                        .flatten()
                        .unwrap_or(0.0),
                    1.0,
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
        half_thickness: f32,
        fade: f32,
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
        let use_shattered = (drop_snapshot_shatter && applied_state == 1)
            || flags & FX_GLASS_STATE_FLAG_DAMAGED != 0;
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
        let mut draw_state = *state;
        if use_shattered {
            fx_iw4::fx_glass_state_set_flags(&mut draw_state, flags | FX_GLASS_STATE_FLAG_DAMAGED);
        }
        // A shard is concave and may carry holes, so the mesh follows the piece's own
        // stored geometry: every border vertex, and the triangulation that spans them.
        let Some(pgeo) = fx_glass_decode_geo(&draw_state, geo) else {
            self.skipped_vert = self.skipped_vert.saturating_add(1);
            *last_why = Some("geo_trunc");
            return;
        };
        let mut cpu = vec![
            fx_iw4::FxGlassIntactVert {
                xyz: [0.0; 3],
                uv: [0.0; 2],
            };
            FX_GLASS_SHARD_VERT_MAX
        ];
        let Some(wrote) = fx_glass_piece_verts(place, &draw_state, def, &pgeo, &mut cpu) else {
            self.skipped_vert = self.skipped_vert.saturating_add(1);
            *last_why = Some("geo_trunc");
            return;
        };
        cpu.truncate(wrote);
        let (need_v, need_i) = fx_glass_slab_counts(&pgeo, half_thickness);
        if need_v == 0
            || self.vertices.len() + need_v > GFX_GLASS_MESH_VERT_LIMIT
            || self.indices.len() + need_i > GFX_GLASS_MESH_INDEX_LIMIT
        {
            self.skipped_cap = self.skipped_cap.saturating_add(1);
            *last_why = Some("mesh_limit");
            return;
        }
        let axis = fx_unit_quat_to_axis(fx_glass_place_quat(place));
        let mut slab_v = vec![
            fx_iw4::FxGlassSlabVert {
                xyz: [0.0; 3],
                uv: [0.0; 2],
                normal: [0.0; 3],
                tangent: [0.0; 3],
                binormal_sign: -1.0,
            };
            need_v
        ];
        let mut slab_i = vec![0u16; need_i];
        let Some((nv, ni)) = fx_glass_emit_slab(
            &cpu,
            &pgeo,
            axis[2],
            axis[0],
            half_thickness,
            &mut slab_v,
            &mut slab_i,
        ) else {
            self.skipped_vert = self.skipped_vert.saturating_add(1);
            *last_why = Some("slab");
            return;
        };
        let color_rgba = fx_glass_scale_color_alpha(fx_glass_def_color_rgba(def), fade);
        let base = self.vertices.len() as u32;
        for v in slab_v.iter().take(nv) {
            self.verts_mut().push(fx_pack_code_mesh_vertex_signed(
                v.xyz,
                color_rgba,
                fx_trail_pack_texcoord(v.uv[0], v.uv[1]),
                fx_trail_pack_normal(v.normal),
                fx_trail_pack_normal(v.tangent),
                v.binormal_sign,
            ));
        }
        let index_start = self.indices.len() as u32;
        for &idx in slab_i.iter().take(ni) {
            self.inds_mut().push(base.saturating_add(u32::from(idx)));
        }
        let material = self.materials.len() as u32;
        self.materials.push(FxPassMaterial {
            color,
            sort_key,
            material_sorted_index: Some(ordinal.get()),
        });
        let init_index = fx_glass_state_init_index(state);
        let origin = fx_glass_place_origin(place);
        self.draws.push(GfxGlassMeshDraw {
            material,
            index_start,
            index_count: ni as u32,
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
            piece: piece as u16,
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

struct GlassSnapRow {
    id: u32,
    state: entity_iw4::GlassPieceState,
    seed: Option<entity_iw4::GlassShatterSeed>,
    deterministic_seed: u64,
    last_change: i32,
    cause: entity_iw4::GlassCause,
    revision: u32,
}

fn rows_from_world_objects(snap: &sim::WorldObjectSnapshot) -> Vec<GlassSnapRow> {
    snap.glass_pieces
        .iter()
        .map(|(id, row)| GlassSnapRow {
            id: *id,
            state: row.state,
            seed: row.shatter_seed,
            deterministic_seed: row.deterministic_seed,
            last_change: row.last_state_change_time,
            cause: row.cause,
            revision: row.revision,
        })
        .collect()
}

fn presented_world_objects<'a>(
    presented: Option<&'a net::PresentedSnapshot>,
    adopted: Option<&'a net::LastAdoptedSnapshot>,
) -> Option<&'a sim::Snapshot> {
    presented
        .and_then(|snap| snap.snapshot())
        .or_else(|| adopted.and_then(|snap| snap.next()))
}

fn glass_snapshot_rows(snap: Option<&sim::Snapshot>) -> Vec<GlassSnapRow> {
    snap.map(|snap| rows_from_world_objects(&snap.meta.world_objects))
        .unwrap_or_default()
}

fn glass_viewer_is_archived(snap: Option<&sim::Snapshot>, local: Option<sim::ClientId>) -> bool {
    let (Some(snap), Some(local)) = (snap, local) else {
        return false;
    };
    snap.players
        .iter()
        .any(|(id, ps)| *id == local && !ps.is_live_frame())
}

fn glass_presentation_now(msec_now: i32, as_of_ms: i32, archived: bool) -> i32 {
    if archived { as_of_ms } else { msec_now }
}

fn glass_shatter_play_oneshot(late: bool, rebuilt: bool) -> bool {
    !late && !rebuilt
}

fn glass_epoch_changed(seen: u32, incoming: u32) -> bool {
    seen != 0 && seen != incoming
}

fn glass_needs_rebuild(
    now: i32,
    last_msec: i32,
    rows: &[GlassSnapRow],
    table: &CgGlassTable,
) -> bool {
    if now < last_msec {
        return true;
    }
    table.rows.iter().enumerate().any(|(i, row)| {
        if row.applied == 0 {
            return false;
        }
        let incoming = rows
            .iter()
            .find(|snap| snap.id == i as u32)
            .map(|snap| snap.state.as_u8())
            .unwrap_or(0);
        row.applied > incoming
    })
}

pub(crate) fn apply_glass_host(
    presented: Option<Res<net::PresentedSnapshot>>,
    adopted: Option<Res<net::LastAdoptedSnapshot>>,
    local: Option<Res<net::LocalPresentClient>>,
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
    mut table: ResMut<CgGlassTable>,
    mut fx_host: Option<ResMut<render_fx::HostFxSystem>>,
) {
    let Some(scene) = scene else {
        return;
    };
    let Some(glass) = scene.fx_glass.as_ref() else {
        return;
    };
    let snap = presented_world_objects(presented.as_deref(), adopted.as_deref());
    let rows = glass_snapshot_rows(snap);
    let msec_now = fx_host.as_ref().map(|h| h.0.msec_now).unwrap_or(0);
    let as_of_ms = snap.map(|s| s.meta.world_objects.as_of_ms).unwrap_or(0);
    let epoch = snap
        .map(|s| s.meta.world_objects.map_round_epoch)
        .unwrap_or(0);
    let now = glass_presentation_now(
        msec_now,
        as_of_ms,
        glass_viewer_is_archived(snap, local.map(|c| c.0)),
    );
    let rebuilt = glass_epoch_changed(table.map_round_epoch, epoch)
        || glass_needs_rebuild(now, table.last_msec, &rows, &table);
    if rebuilt {
        table.reset();
        if let Some(host) = fx_host.as_mut() {
            host.0.glass.reset(fx::FxGlassInitTables {
                piece_limit: glass.piece_limit as u32,
                geo_data_limit: glass.geo_data_limit as u32,
                init_states: &glass.init_piece_states,
                init_geo: &glass.init_geo_data,
                defs: &glass.defs,
            });
            host.0.glass.moved = true;
        }
    }
    table.last_msec = now;
    table.map_round_epoch = epoch;
    for snap in &rows {
        let Some(row) = table.rows.get_mut(snap.id as usize) else {
            continue;
        };
        cg_glass_read_change(row, snap.state, snap.seed);
    }
    for (i, row) in table.rows.iter_mut().enumerate() {
        if row.applied >= row.pending {
            continue;
        }
        let snap = rows.iter().find(|s| s.id == i as u32);
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
                    host.0.marks.hide_glass_marks(i as u16);
                    host.0.glass.free_pane(i as u32);
                    host.0.glass.moved = true;
                }
            }
            CgGlassApplyAction::Weaken => {
                if let Some(host) = fx_host.as_mut() {
                    host.0.glass.damage(i as u32);
                }
            }
            CgGlassApplyAction::Shatter {
                hit,
                dir,
                weakened_first,
            } => {
                if let Some(host) = fx_host.as_mut() {
                    let last_change = snap.map(|s| s.last_change).unwrap_or(0);
                    let late = last_change != 0
                        && now.saturating_sub(last_change) > FX_GLASS_SHARD_LIFETIME_MSEC;
                    let seed = snap.map(|s| s.deterministic_seed).filter(|s| *s != 0);
                    let cause = snap.map(|s| s.cause.as_u8()).unwrap_or(0);
                    let play_oneshot = glass_shatter_play_oneshot(late, rebuilt);
                    let revision = snap.map(|s| s.revision).unwrap_or(0);
                    if weakened_first {
                        host.0.glass.damage(i as u32);
                    }
                    host.0.glass.shatter_caused(
                        i as u32,
                        hit,
                        dir,
                        seed,
                        !late,
                        play_oneshot,
                        cause,
                        revision,
                    );
                    host.0.marks.hide_glass_marks(i as u16);
                    host.0.glass.moved = true;
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn apply_cg_glass_tess(
    scene: Option<Res<crate::prepare::scene::world::WorldScene>>,
    colors: Option<Res<render_fx::FxWorldColorImages>>,
    world_plan: Option<Res<super::world::WorldDrawGpuPlan>>,
    mut plan: ResMut<GfxGlassMeshPlan>,
    table: Res<CgGlassTable>,
    mut fx_host: Option<ResMut<render_fx::HostFxSystem>>,
) {
    let Some(scene) = scene else {
        return;
    };
    if !plan.built {
        if world_plan.is_none() {
            return;
        }
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
            "glass mesh tess plan: surfs={} verts={} indices={} skip_why={} skipped_vert={} skipped_def={} skipped_name={} skipped_ordinal={} skipped_cap={} skipped_shatter={} (two-sided slab; vis_cull=none; not T5 0xF)",
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
    let applied = table.applied_pairs();
    let glass_moved = fx_host.as_ref().is_some_and(|h| h.0.glass.moved);
    if applied == plan.applied && !glass_moved {
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
        let owner = ModelLightingOwner::Glass(draw.piece);
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
