use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;

use super::smodel::{RetailPackedVertexRefusal, SmodelPassMaterial, SmodelVertex};

pub use render_anim::{
    BODY_PACKED_UNAVAILABLE, DynEntAssetDraw, DynEntDrawPlan, DynEntOwnerDraw,
    FPV_PACKED_EMPTY_PLAN, FPV_PACKED_UNAVAILABLE, FpvDrawPlan, FpvPlanSurface, FpvSurfaceDraw,
    ItemAssetDraw, ItemDrawPlan, ItemOwnerDraw, MissileDrawPlan, MissileOwnerDraw,
    RemoteBodyDrawPlan, RemoteBodySurfaceDraw, ScriptModelAssetDraw, ScriptModelDrawPlan,
    ScriptModelOwnerDraw, XMODEL_OBJECT_ID_BODY_BASE, XMODEL_OBJECT_ID_DYNENT_BASE,
    XMODEL_OBJECT_ID_ITEM_BASE, XMODEL_OBJECT_ID_MISSILE_BASE,
};
pub use render_anim::{
    BodyPackedSession, append_dynent_asset, append_dynent_surfaces, append_item_surfaces,
    append_missile_surfaces, append_remote_body_cpu_blob, append_script_model_asset,
    authored_lit_xmodel_pass_material, body_lit_pass_material, bound_lit_xmodel_pass_material,
    finish_remote_body_draw_plan, install_body_packed_session, overwrite_script_model_asset,
    push_remote_body_cpu_draw, rebuild_fpv_draw_plan, retain_script_model_assets,
    take_body_packed_session,
};
pub use render_fx::append_fx_model_asset;

pub use render_frame::XMODEL_OBJECT_ID_VIEWMODEL;
pub use render_fx::{FxModelAssetDraw, FxModelDrawPlan, XMODEL_OBJECT_ID_FX_BASE};
pub use render_scene::{XModelColourRefusal, XModelSurfaceDraw};

#[derive(Resource, Clone, Debug, Default)]
pub struct XModelDrawPlan {
    pub vertices: Vec<SmodelVertex>,

    pub decoded_n: usize,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub draws: Vec<XModelSurfaceDraw>,

    pub fx_object_id_exhausted: u32,
    pub revision: u64,

    pub topology_revision: u64,

    pub packed_vertices: assets::RetailPackedVertexPayload,

    pub packed_share: Option<Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>>,

    pub packed_segments: render_frame::PackedSegments,

    published_packed: Option<PackedContentKey>,

    pub index_share: Option<Arc<Vec<u32>>>,
    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
    packed_banks: super::ShareBanks<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    index_banks: super::ShareBanks<u32>,
    range_banks: super::ShareBanks<(u32, u32)>,

    last_input: Option<XModelMergeStamp>,
    last_topology: Option<XModelTopologyKey>,
    concat_layout: bool,
    last_admitted: u8,

    packed_owner_vertices: [[u64; 8]; super::SHARE_BANKS],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct XModelMergeStamps {
    pub append_ms: f32,

    pub packed_ms: f32,

    pub concatenated: bool,
}

pub const XMODEL_PACKED_UNAVAILABLE: &str =
    "xmodel merge GfxPackedVertex missing or count-mismatched; decoded float is not packed VB";
pub const XMODEL_PACKED_EMPTY_PLAN: &str = "xmodel plan has no vertices";

impl XModelDrawPlan {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.decoded_n = 0;
        self.indices.clear();
        self.surface_ranges.clear();
        self.materials.clear();
        self.draws.clear();
        self.fx_object_id_exhausted = 0;
        self.packed_share = None;
        self.packed_segments.forget();
        self.index_share = None;
        self.range_share = None;
        self.last_input = None;
        self.last_topology = None;
        self.concat_layout = false;
        self.last_admitted = 0;
        self.packed_owner_vertices = [[0; 8]; super::SHARE_BANKS];
        self.packed_vertices = assets::RetailPackedVertexPayload::Unavailable {
            source_layout: XMODEL_PACKED_UNAVAILABLE,
        };
    }

    pub fn packed_rows(&self) -> Option<&[[u8; asset_iw4::size::GFX_PACKED_VERTEX]]> {
        if let Some(share) = self.packed_share.as_ref() {
            return Some(share.as_slice());
        }
        match &self.packed_vertices {
            assets::RetailPackedVertexPayload::Iw4(rows) => Some(rows.as_slice()),
            assets::RetailPackedVertexPayload::Unavailable { .. } => None,
        }
    }

    pub fn index_rows(&self) -> &[u32] {
        super::published_or_live(self.index_share.as_ref(), &self.indices)
    }

    pub fn range_rows(&self) -> &[(u32, u32)] {
        super::published_or_live(self.range_share.as_ref(), &self.surface_ranges)
    }

    pub fn exact_packed_vertices(
        &self,
    ) -> Result<&[[u8; asset_iw4::size::GFX_PACKED_VERTEX]], RetailPackedVertexRefusal> {
        let table_stride =
            asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::PACKED_VERTEX_TYPE, 0);
        if table_stride != Some(asset_iw4::size::GFX_PACKED_VERTEX as u16) {
            return Err(RetailPackedVertexRefusal::RetailStrideMismatch { table_stride });
        }
        let vertices = match self.packed_rows() {
            Some(vertices) => vertices,
            None => {
                let source_layout = match &self.packed_vertices {
                    assets::RetailPackedVertexPayload::Unavailable { source_layout } => {
                        *source_layout
                    }
                    assets::RetailPackedVertexPayload::Iw4(_) => XMODEL_PACKED_UNAVAILABLE,
                };
                return Err(RetailPackedVertexRefusal::ForeignLayout { source_layout });
            }
        };
        let decoded = if self.vertices.is_empty() {
            self.decoded_n
        } else {
            self.vertices.len()
        };
        if vertices.len() != decoded {
            return Err(RetailPackedVertexRefusal::VertexCountMismatch {
                retail: vertices.len(),
                decoded,
            });
        }
        Ok(vertices)
    }
}

pub const HOST_XMODEL_RIGID_TESS_INFO_PACKED_ARM: u8 = 1;

pub fn xmodel_rigid_vert_decl_type(tess_info_byte_10: u8) -> u8 {
    render_frame::xmodel_tess_info_vert_decl_type(tess_info_byte_10)
}

#[must_use]
fn fpv_scene_colour(fpv: &FpvDrawPlan, scene: Option<&crate::GfxScene>) -> bool {
    fpv.admits_colour() && scene.is_some_and(|s| s.scene_dobj_live(crate::SCENE_VIEWMODEL_ENTNUM))
}

#[must_use]
fn scene_ent_admitted(scene: Option<&crate::GfxScene>, entnum: Option<u32>) -> bool {
    match entnum {
        None => true,
        Some(n) => scene.is_some_and(|s| s.scene_ent_live(n) && !s.scene_ent_skips_draw(n)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct XModelTopologyKey {
    generation: [u64; 8],
    topology: [u64; 8],
    admission: [u64; 8],
    occupancy: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct XModelMergeStamp {
    topology: XModelTopologyKey,
    vertices: [u64; 8],
    draws: [u64; 8],
    producer_revision: [u64; 8],
    fpv_world: [u32; 16],
    fpv_lighting: u32,
    fpv_scene_light: u8,
    fpv_probe: u8,
    fpv_visible: bool,
    fpv_drawgun: Option<i32>,
    fpv_colour: bool,
    sky_eye: Option<[u32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedContentKey {
    generation: [u64; 8],
    admission: [u64; 8],
    occupancy: u64,
    admitted: u8,
    rows: usize,
    vertices: [u64; 8],
}

impl PackedContentKey {
    fn new(stamp: &XModelMergeStamp, admitted: u8, rows: usize) -> Self {
        Self {
            generation: stamp.topology.generation,
            admission: stamp.topology.admission,
            occupancy: stamp.topology.occupancy,
            admitted,
            rows,
            vertices: stamp.vertices,
        }
    }
}

const ADMIT_BODY: u8 = 1 << 0;
const ADMIT_FPV: u8 = 1 << 1;
const ADMIT_SCRIPT: u8 = 1 << 2;
const ADMIT_MISSILE: u8 = 1 << 3;
const ADMIT_ITEM: u8 = 1 << 4;
const ADMIT_FX: u8 = 1 << 5;
const ADMIT_DYNENT: u8 = 1 << 6;
const ADMIT_SKY: u8 = 1 << 7;

fn producer_keys(
    fpv: &FpvDrawPlan,
    bodies: &RemoteBodyDrawPlan,
    scripts: &ScriptModelDrawPlan,
    missiles: &MissileDrawPlan,
    items: &ItemDrawPlan,
    fx_models: &FxModelDrawPlan,
    dynents: &DynEntDrawPlan,
    sky: Option<&super::sky::SkyModelDrawPlan>,
) -> ([u64; 8], [u64; 8], [u64; 8], [u64; 8], [u64; 8], [u64; 8]) {
    let sky_rev = sky.map(|sky| sky.revisions).unwrap_or_default();
    let sky_gen = sky.map(|sky| sky.generation).unwrap_or(0);
    (
        [
            fpv.generation,
            bodies.generation,
            scripts.generation,
            missiles.generation,
            items.generation,
            fx_models.generation,
            dynents.generation,
            sky_gen,
        ],
        [
            fpv.revisions.topology,
            bodies.revisions.topology,
            scripts.revisions.topology,
            missiles.revisions.topology,
            items.revisions.topology,
            fx_models.revisions.topology,
            dynents.revisions.topology,
            sky_rev.topology,
        ],
        [
            fpv.revisions.admission,
            bodies.revisions.admission,
            scripts.revisions.admission,
            missiles.revisions.admission,
            items.revisions.admission,
            fx_models.revisions.admission,
            dynents.revisions.admission,
            sky_rev.admission,
        ],
        [
            fpv.revisions.vertices,
            bodies.revisions.vertices,
            scripts.revisions.vertices,
            missiles.revisions.vertices,
            items.revisions.vertices,
            fx_models.revisions.vertices,
            dynents.revisions.vertices,
            sky_rev.vertices,
        ],
        [
            fpv.revisions.draws,
            bodies.revisions.draws,
            scripts.revisions.draws,
            missiles.revisions.draws,
            items.revisions.draws,
            fx_models.revisions.draws,
            dynents.revisions.draws,
            sky_rev.draws,
        ],
        [
            fpv.revision,
            bodies.revision,
            scripts.revision,
            missiles.revision,
            items.revision,
            fx_models.revision,
            dynents.revision,
            0,
        ],
    )
}

fn xmodel_merge_stamp(
    fpv: &FpvDrawPlan,
    bodies: &RemoteBodyDrawPlan,
    scripts: &ScriptModelDrawPlan,
    missiles: &MissileDrawPlan,
    items: &ItemDrawPlan,
    fx_models: &FxModelDrawPlan,
    dynents: &DynEntDrawPlan,
    scene: Option<&crate::GfxScene>,
    sky: Option<(&super::sky::SkyModelDrawPlan, Vec3)>,
) -> XModelMergeStamp {
    // The one thing still read out of the producers' rows here is which scene
    // entity each draw belongs to, because whether that entity is admitted this
    // frame is the scene's answer, not the producer's — a frame input, not a
    // guess at whether the payload changed. The payload fingerprint that used
    // to sit next to it is gone: `vertices`, `draws` and `topology` below are
    // the producers' own published verdicts now.
    let mut occupancy = DefaultHasher::new();
    fpv_scene_colour(fpv, scene).hash(&mut occupancy);
    for draw in bodies.draws() {
        scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
    }
    for draw in scripts.draws() {
        scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
    }
    for draw in missiles.draws() {
        scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
    }
    for draw in items.draws() {
        scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
    }
    let sky_eye = sky.map(|(_, eye)| [eye.x.to_bits(), eye.y.to_bits(), eye.z.to_bits()]);
    let (generation, topology, admission, vertices, draws, producer_revision) = producer_keys(
        fpv,
        bodies,
        scripts,
        missiles,
        items,
        fx_models,
        dynents,
        sky.map(|(sky, _)| sky),
    );
    XModelMergeStamp {
        topology: XModelTopologyKey {
            generation,
            topology,
            admission,
            occupancy: occupancy.finish(),
        },
        vertices,
        draws,
        producer_revision,
        fpv_world: mat4_bits(fpv.world_from_local),
        fpv_lighting: fpv.lighting_handle,
        fpv_scene_light: fpv.scene_light_index,
        fpv_probe: fpv.reflection_probe_index,
        fpv_visible: fpv.visible,
        fpv_drawgun: fpv.drawgun,
        fpv_colour: fpv_scene_colour(fpv, scene),
        sky_eye,
    }
}

fn mat4_bits(m: Mat4) -> [u32; 16] {
    let cols = m.to_cols_array();
    std::array::from_fn(|i| cols[i].to_bits())
}

pub fn merge_xmodel_draw_plan(
    merged: &mut XModelDrawPlan,
    fpv: &FpvDrawPlan,
    bodies: &RemoteBodyDrawPlan,
    scripts: &ScriptModelDrawPlan,
    missiles: &MissileDrawPlan,
    items: &ItemDrawPlan,
    fx_models: &FxModelDrawPlan,
    dynents: &DynEntDrawPlan,
    scene: Option<&crate::GfxScene>,
    sky: Option<(&super::sky::SkyModelDrawPlan, Vec3)>,
) -> XModelMergeStamps {
    let stamp = xmodel_merge_stamp(
        fpv, bodies, scripts, missiles, items, fx_models, dynents, scene, sky,
    );
    if merged.last_input == Some(stamp) {
        return XModelMergeStamps {
            concatenated: false,
            ..XModelMergeStamps::default()
        };
    }
    if merged.last_topology == Some(stamp.topology) && merged.concat_layout {
        let prev = merged.last_input;
        let vertices_changed = prev.is_none_or(|p| p.vertices != stamp.vertices);
        let draws_changed = prev.is_none_or(|p| concat_draws_changed(p, &stamp));
        let mut packed_ms = 0.0;
        if vertices_changed {
            let packed_started = Instant::now();
            let prev_packed = merged.packed_share.take();
            let (mut packed, fresh) = merged.packed_banks.take_write_keep();
            let write = merged.packed_banks.write_index();
            if fresh {
                merged.packed_owner_vertices[write] = [0; 8];
            }
            let mut packed_ok = true;
            recopy_concat_packed_bank(
                &mut packed,
                &mut packed_ok,
                fresh,
                &mut merged.packed_owner_vertices[write],
                stamp.vertices,
                merged.last_admitted,
                concat_packed_owners(
                    fpv, bodies, scripts, missiles, items, fx_models, dynents, sky,
                ),
                &mut merged.packed_segments,
            );
            let key = packed_ok
                .then(|| PackedContentKey::new(&stamp, merged.last_admitted, packed.len()));
            let payload = install_retained_packed(
                packed_ok,
                packed,
                merged.decoded_n,
                XMODEL_PACKED_EMPTY_PLAN,
                XMODEL_PACKED_UNAVAILABLE,
            );
            publish_packed_payload(merged, prev_packed, payload, key);
            packed_ms = packed_started.elapsed().as_secs_f32() * 1000.0;
        }
        if draws_changed {
            refresh_concat_draws(
                merged, fpv, bodies, scripts, missiles, items, fx_models, dynents, scene, sky,
            );
        }
        merged.last_input = Some(stamp);
        return XModelMergeStamps {
            concatenated: true,
            packed_ms,
            append_ms: 0.0,
        };
    }
    let prev_packed = merged.packed_share.take();
    let prev_index = merged.index_share.take();
    let prev_range = merged.range_share.take();
    let mut packed = merged.packed_banks.take_write();
    let packed_write = merged.packed_banks.write_index();
    merged.packed_owner_vertices[packed_write] = [0; 8];
    merged.packed_segments.forget();
    packed.clear();
    let indices = merged.index_banks.take_write();
    let ranges = merged.range_banks.take_write();
    merged.clear();
    merged.indices = indices;
    merged.surface_ranges = ranges;
    merged.indices.clear();
    merged.surface_ranges.clear();
    merged.concat_layout = true;

    packed.reserve(
        packed_row_count(bodies.packed_vertices())
            + packed_row_count(fpv.packed_vertices())
            + packed_row_count(scripts.packed_vertices())
            + packed_row_count(missiles.packed_vertices())
            + packed_row_count(items.packed_vertices())
            + packed_row_count(fx_models.packed_vertices())
            + packed_row_count(dynents.packed_vertices()),
    );
    let mut packed_ok = true;
    let append_started = Instant::now();
    let mut next_body_object = XMODEL_OBJECT_ID_BODY_BASE;
    let (fx_draws, fx_object_id_exhausted) = dense_fx_object_ids(
        fx_models.draws(),
        merged
            .draws
            .iter()
            .chain(dynents.draws().iter())
            .map(|draw| draw.object_id)
            .max(),
    );
    merged.fx_object_id_exhausted = fx_object_id_exhausted;
    append_admitted_source(
        merged,
        &mut packed,
        &mut packed_ok,
        bodies.decoded_n(),
        bodies.indices(),
        bodies.surface_ranges(),
        bodies.materials(),
        bodies.packed_vertices(),
        bodies.draws().iter().filter_map(|d| {
            scene_ent_admitted(scene, d.scene_entnum).then(|| {
                let object_id = next_body_object;
                next_body_object = next_body_object.saturating_add(1);
                XModelSurfaceDraw {
                    surface: d.surface,
                    material: d.material,
                    world_from_local: d.world_from_local,
                    lighting_handle: d.lighting_handle,

                    pending_lighting: None,
                    colour_refusal: None,
                    object_id,
                    scene_light_index: d.scene_light_index,
                    reflection_probe_index: d.reflection_probe_index,
                    packed_lighting: None,
                    is_scope: false,
                    scene_entnum: d.scene_entnum,
                }
            })
        }),
    );

    if fpv_scene_colour(fpv, scene) {
        append_admitted_source(
            merged,
            &mut packed,
            &mut packed_ok,
            fpv.vertices().len(),
            fpv.indices(),
            fpv.surface_ranges(),
            fpv.materials(),
            fpv.packed_vertices(),
            fpv.draws().iter().map(|d| XModelSurfaceDraw {
                surface: d.surface,
                material: d.material,
                world_from_local: fpv.world_from_local,
                lighting_handle: fpv.lighting_handle,
                pending_lighting: None,
                colour_refusal: None,
                object_id: XMODEL_OBJECT_ID_VIEWMODEL,
                scene_light_index: fpv.scene_light_index,
                reflection_probe_index: fpv.reflection_probe_index,
                packed_lighting: None,
                is_scope: d.is_scope,
                scene_entnum: Some(crate::SCENE_VIEWMODEL_ENTNUM),
            }),
        );
    }

    append_admitted_source(
        merged,
        &mut packed,
        &mut packed_ok,
        scripts.vertices().len(),
        scripts.indices(),
        scripts.surface_ranges(),
        scripts.materials(),
        scripts.packed_vertices(),
        scripts
            .draws()
            .iter()
            .copied()
            .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
    );

    append_admitted_source(
        merged,
        &mut packed,
        &mut packed_ok,
        missiles.vertices().len(),
        missiles.indices(),
        missiles.surface_ranges(),
        missiles.materials(),
        missiles.packed_vertices(),
        missiles
            .draws()
            .iter()
            .copied()
            .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
    );

    append_admitted_source(
        merged,
        &mut packed,
        &mut packed_ok,
        items.vertices().len(),
        items.indices(),
        items.surface_ranges(),
        items.materials(),
        items.packed_vertices(),
        items
            .draws()
            .iter()
            .copied()
            .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
    );
    append_admitted_source(
        merged,
        &mut packed,
        &mut packed_ok,
        fx_models.vertices().len(),
        fx_models.indices(),
        fx_models.surface_ranges(),
        fx_models.materials(),
        fx_models.packed_vertices(),
        fx_draws,
    );
    append_admitted_source(
        merged,
        &mut packed,
        &mut packed_ok,
        dynents.vertices().len(),
        dynents.indices(),
        dynents.surface_ranges(),
        dynents.materials(),
        dynents.packed_vertices(),
        dynents.draws().iter().copied(),
    );
    if let Some((sky, eye)) = sky {
        append_admitted_source(
            merged,
            &mut packed,
            &mut packed_ok,
            sky.geometry.vertices.len(),
            &sky.geometry.indices,
            &sky.geometry.surface_ranges,
            &sky.geometry.materials,
            &sky.geometry.packed_vertices,
            sky.draws.iter().map(|draw| XModelSurfaceDraw {
                world_from_local: Mat4::from_translation(eye),
                ..*draw
            }),
        );
    }
    let append_ms = append_started.elapsed().as_secs_f32() * 1000.0;
    let packed_started = Instant::now();
    if packed_ok {
        merged.packed_owner_vertices[packed_write] = stamp.vertices;
    }
    merged.last_admitted = admitted_mask(
        fpv, bodies, scripts, missiles, items, fx_models, dynents, scene, sky,
    );
    let key = packed_ok.then(|| PackedContentKey::new(&stamp, merged.last_admitted, packed.len()));
    let payload = install_retained_packed(
        packed_ok,
        packed,
        merged.decoded_n,
        XMODEL_PACKED_EMPTY_PLAN,
        XMODEL_PACKED_UNAVAILABLE,
    );
    publish_packed_payload(merged, prev_packed, payload, key);
    publish_topology_shares(
        prev_index,
        prev_range,
        &mut merged.indices,
        &mut merged.surface_ranges,
        &mut merged.index_share,
        &mut merged.range_share,
        &mut merged.topology_revision,
        &mut merged.index_banks,
        &mut merged.range_banks,
    );
    merged.last_input = Some(stamp);
    merged.last_topology = Some(stamp.topology);
    let packed_ms = packed_started.elapsed().as_secs_f32() * 1000.0;
    XModelMergeStamps {
        append_ms,
        packed_ms,
        concatenated: true,
    }
}

fn dense_fx_object_ids(
    draws: &[XModelSurfaceDraw],
    max_non_fx: Option<u16>,
) -> (Vec<XModelSurfaceDraw>, u32) {
    let mut next = match max_non_fx {
        Some(max_id) => max_id
            .checked_add(1)
            .map(|id| id.max(XMODEL_OBJECT_ID_FX_BASE)),
        None => Some(XMODEL_OBJECT_ID_FX_BASE),
    };
    let mut previous_source = None;
    let mut current = None;
    let mut exhausted = 0u32;
    let mut dense = Vec::with_capacity(draws.len());
    for draw in draws {
        if previous_source != Some(draw.object_id) {
            previous_source = Some(draw.object_id);
            current = next;
            next = next.and_then(|id| id.checked_add(1));
        }
        let Some(object_id) = current else {
            exhausted = exhausted.saturating_add(1);
            continue;
        };
        dense.push(XModelSurfaceDraw { object_id, ..*draw });
    }
    (dense, exhausted)
}

fn concat_draws_changed(prev: XModelMergeStamp, stamp: &XModelMergeStamp) -> bool {
    // `vertices` is in here because a producer that changed its material list
    // bumps that revision, and the concat refresh rebuilds the merged material
    // list along with the draws.
    prev.draws != stamp.draws
        || prev.vertices != stamp.vertices
        || prev.fpv_world != stamp.fpv_world
        || prev.fpv_lighting != stamp.fpv_lighting
        || prev.fpv_scene_light != stamp.fpv_scene_light
        || prev.fpv_probe != stamp.fpv_probe
        || prev.fpv_visible != stamp.fpv_visible
        || prev.fpv_drawgun != stamp.fpv_drawgun
        || prev.fpv_colour != stamp.fpv_colour
        || prev.sky_eye != stamp.sky_eye
}

struct ConcatPackedOwner<'a> {
    admit: u8,
    rev_i: usize,
    payload: &'a assets::RetailPackedVertexPayload,
    decoded_n: usize,
}

fn concat_packed_owners<'a>(
    fpv: &'a FpvDrawPlan,
    bodies: &'a RemoteBodyDrawPlan,
    scripts: &'a ScriptModelDrawPlan,
    missiles: &'a MissileDrawPlan,
    items: &'a ItemDrawPlan,
    fx_models: &'a FxModelDrawPlan,
    dynents: &'a DynEntDrawPlan,
    sky: Option<(&'a super::sky::SkyModelDrawPlan, Vec3)>,
) -> [ConcatPackedOwner<'a>; 8] {
    let sky_geom = sky.map(|(sky, _)| &sky.geometry);
    [
        ConcatPackedOwner {
            admit: ADMIT_BODY,
            rev_i: 1,
            payload: bodies.packed_vertices(),
            decoded_n: bodies.decoded_n(),
        },
        ConcatPackedOwner {
            admit: ADMIT_FPV,
            rev_i: 0,
            payload: fpv.packed_vertices(),
            decoded_n: fpv.vertices().len(),
        },
        ConcatPackedOwner {
            admit: ADMIT_SCRIPT,
            rev_i: 2,
            payload: scripts.packed_vertices(),
            decoded_n: scripts.vertices().len(),
        },
        ConcatPackedOwner {
            admit: ADMIT_MISSILE,
            rev_i: 3,
            payload: missiles.packed_vertices(),
            decoded_n: missiles.vertices().len(),
        },
        ConcatPackedOwner {
            admit: ADMIT_ITEM,
            rev_i: 4,
            payload: items.packed_vertices(),
            decoded_n: items.vertices().len(),
        },
        ConcatPackedOwner {
            admit: ADMIT_FX,
            rev_i: 5,
            payload: fx_models.packed_vertices(),
            decoded_n: fx_models.vertices().len(),
        },
        ConcatPackedOwner {
            admit: ADMIT_DYNENT,
            rev_i: 6,
            payload: dynents.packed_vertices(),
            decoded_n: dynents.vertices().len(),
        },
        ConcatPackedOwner {
            admit: ADMIT_SKY,
            rev_i: 7,
            payload: sky_geom
                .map(|g| &g.packed_vertices)
                .unwrap_or(bodies.packed_vertices()),
            decoded_n: sky_geom.map(|g| g.vertices.len()).unwrap_or(0),
        },
    ]
}

fn concat_packed_layout(admitted: u8, owners: &[ConcatPackedOwner<'_>; 8]) -> ([usize; 8], usize) {
    let mut counts = [0usize; 8];
    let mut expected = 0usize;
    for (i, owner) in owners.iter().enumerate() {
        if admitted & owner.admit == 0 || owner.decoded_n == 0 {
            continue;
        }
        counts[i] = packed_row_count(owner.payload);
        expected = expected.saturating_add(counts[i]);
    }
    (counts, expected)
}

fn concat_packed_segments(
    counts: [usize; 8],
    stamp_vertices: [u64; 8],
    owners: &[ConcatPackedOwner<'_>; 8],
) -> [render_frame::PackedSegment; render_frame::PACKED_SEGMENT_OWNERS] {
    let mut start = 0u32;
    std::array::from_fn(|i| {
        let rows = u32::try_from(counts[i]).unwrap_or(u32::MAX);
        let segment = render_frame::PackedSegment {
            start,
            rows,
            revision: stamp_vertices[owners[i].rev_i],
        };
        start = start.saturating_add(rows);
        segment
    })
}

fn recopy_concat_packed_bank(
    packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    packed_ok: &mut bool,
    fresh: bool,
    bank_vertices: &mut [u64; 8],
    stamp_vertices: [u64; 8],
    admitted: u8,
    owners: [ConcatPackedOwner<'_>; 8],
    published: &mut render_frame::PackedSegments,
) {
    let (counts, expected) = concat_packed_layout(admitted, &owners);
    let can_patch = !fresh && packed.len() == expected && *packed_ok;
    if !can_patch {
        packed.clear();
        packed.reserve(expected);
        for owner in &owners {
            if admitted & owner.admit != 0 {
                append_packed_source(packed, packed_ok, owner.payload, owner.decoded_n);
            }
        }
        *bank_vertices = if *packed_ok { stamp_vertices } else { [0; 8] };
        published.forget();
        return;
    }
    let mut off = 0usize;
    for (i, owner) in owners.iter().enumerate() {
        if admitted & owner.admit == 0 || owner.decoded_n == 0 {
            continue;
        }
        let n = counts[i];
        if n != owner.decoded_n {
            *packed_ok = false;
            packed.clear();
            *bank_vertices = [0; 8];
            published.forget();
            return;
        }
        if bank_vertices[owner.rev_i] != stamp_vertices[owner.rev_i] {
            match owner.payload {
                assets::RetailPackedVertexPayload::Iw4(rows) if rows.len() == n => {
                    packed[off..off + n].copy_from_slice(rows);
                }
                _ => {
                    *packed_ok = false;
                    packed.clear();
                    *bank_vertices = [0; 8];
                    published.forget();
                    return;
                }
            }
        }
        off = off.saturating_add(n);
    }
    *bank_vertices = stamp_vertices;
    published.publish(concat_packed_segments(counts, stamp_vertices, &owners));
}

fn append_concat_owner_draws(
    merged: &mut XModelDrawPlan,
    range_base: &mut u32,
    surface_n: usize,
    materials: &[SmodelPassMaterial],
    draws: impl IntoIterator<Item = XModelSurfaceDraw>,
) {
    let draws: Vec<XModelSurfaceDraw> = draws.into_iter().collect();
    if draws.is_empty() {
        return;
    }
    let mat_base = merged.materials.len() as u32;
    merged.materials.extend_from_slice(materials);
    for mut d in draws {
        d.surface = d.surface.saturating_add(*range_base);
        d.material = d.material.saturating_add(mat_base);
        merged.draws.push(d);
    }
    *range_base = range_base.saturating_add(surface_n as u32);
}

fn refresh_concat_draws(
    merged: &mut XModelDrawPlan,
    fpv: &FpvDrawPlan,
    bodies: &RemoteBodyDrawPlan,
    scripts: &ScriptModelDrawPlan,
    missiles: &MissileDrawPlan,
    items: &ItemDrawPlan,
    fx_models: &FxModelDrawPlan,
    dynents: &DynEntDrawPlan,
    scene: Option<&crate::GfxScene>,
    sky: Option<(&super::sky::SkyModelDrawPlan, Vec3)>,
) {
    merged.draws.clear();
    merged.materials.clear();
    let mut range_base = 0u32;
    let mut next_body_object = XMODEL_OBJECT_ID_BODY_BASE;
    let (fx_draws, fx_object_id_exhausted) = dense_fx_object_ids(
        fx_models.draws(),
        dynents.draws().iter().map(|draw| draw.object_id).max(),
    );
    merged.fx_object_id_exhausted = fx_object_id_exhausted;
    append_concat_owner_draws(
        merged,
        &mut range_base,
        bodies.surface_ranges().len(),
        bodies.materials(),
        bodies.draws().iter().filter_map(|d| {
            scene_ent_admitted(scene, d.scene_entnum).then(|| {
                let object_id = next_body_object;
                next_body_object = next_body_object.saturating_add(1);
                XModelSurfaceDraw {
                    surface: d.surface,
                    material: d.material,
                    world_from_local: d.world_from_local,
                    lighting_handle: d.lighting_handle,
                    pending_lighting: None,
                    colour_refusal: None,
                    object_id,
                    scene_light_index: d.scene_light_index,
                    reflection_probe_index: d.reflection_probe_index,
                    packed_lighting: None,
                    is_scope: false,
                    scene_entnum: d.scene_entnum,
                }
            })
        }),
    );
    if fpv_scene_colour(fpv, scene) {
        append_concat_owner_draws(
            merged,
            &mut range_base,
            fpv.surface_ranges().len(),
            fpv.materials(),
            fpv.draws().iter().map(|d| XModelSurfaceDraw {
                surface: d.surface,
                material: d.material,
                world_from_local: fpv.world_from_local,
                lighting_handle: fpv.lighting_handle,
                pending_lighting: None,
                colour_refusal: None,
                object_id: XMODEL_OBJECT_ID_VIEWMODEL,
                scene_light_index: fpv.scene_light_index,
                reflection_probe_index: fpv.reflection_probe_index,
                packed_lighting: None,
                is_scope: d.is_scope,
                scene_entnum: Some(crate::SCENE_VIEWMODEL_ENTNUM),
            }),
        );
    }
    append_concat_owner_draws(
        merged,
        &mut range_base,
        scripts.surface_ranges().len(),
        scripts.materials(),
        scripts
            .draws()
            .iter()
            .copied()
            .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
    );
    append_concat_owner_draws(
        merged,
        &mut range_base,
        missiles.surface_ranges().len(),
        missiles.materials(),
        missiles
            .draws()
            .iter()
            .copied()
            .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
    );
    append_concat_owner_draws(
        merged,
        &mut range_base,
        items.surface_ranges().len(),
        items.materials(),
        items
            .draws()
            .iter()
            .copied()
            .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
    );
    append_concat_owner_draws(
        merged,
        &mut range_base,
        fx_models.surface_ranges().len(),
        fx_models.materials(),
        fx_draws,
    );
    append_concat_owner_draws(
        merged,
        &mut range_base,
        dynents.surface_ranges().len(),
        dynents.materials(),
        dynents.draws().iter().copied(),
    );
    if let Some((sky, eye)) = sky {
        append_concat_owner_draws(
            merged,
            &mut range_base,
            sky.geometry.surface_ranges.len(),
            &sky.geometry.materials,
            sky.draws.iter().map(|draw| XModelSurfaceDraw {
                world_from_local: Mat4::from_translation(eye),
                ..*draw
            }),
        );
    }
}

fn admitted_mask(
    fpv: &FpvDrawPlan,
    bodies: &RemoteBodyDrawPlan,
    scripts: &ScriptModelDrawPlan,
    missiles: &MissileDrawPlan,
    items: &ItemDrawPlan,
    fx_models: &FxModelDrawPlan,
    dynents: &DynEntDrawPlan,
    scene: Option<&crate::GfxScene>,
    sky: Option<(&super::sky::SkyModelDrawPlan, Vec3)>,
) -> u8 {
    let mut admitted = 0u8;
    if bodies
        .draws()
        .iter()
        .any(|d| scene_ent_admitted(scene, d.scene_entnum))
    {
        admitted |= ADMIT_BODY;
    }
    if fpv_scene_colour(fpv, scene) {
        admitted |= ADMIT_FPV;
    }
    if scripts
        .draws()
        .iter()
        .any(|d| scene_ent_admitted(scene, d.scene_entnum))
    {
        admitted |= ADMIT_SCRIPT;
    }
    if missiles
        .draws()
        .iter()
        .any(|d| scene_ent_admitted(scene, d.scene_entnum))
    {
        admitted |= ADMIT_MISSILE;
    }
    if items
        .draws()
        .iter()
        .any(|d| scene_ent_admitted(scene, d.scene_entnum))
    {
        admitted |= ADMIT_ITEM;
    }
    if !fx_models.draws().is_empty() {
        admitted |= ADMIT_FX;
    }
    if !dynents.draws().is_empty() {
        admitted |= ADMIT_DYNENT;
    }
    if sky.is_some() {
        admitted |= ADMIT_SKY;
    }
    admitted
}

fn publish_packed_payload(
    merged: &mut XModelDrawPlan,
    prev: Option<Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>>,
    payload: assets::RetailPackedVertexPayload,
    key: Option<PackedContentKey>,
) {
    match payload {
        assets::RetailPackedVertexPayload::Iw4(rows) => {
            let resident = key.is_some() && merged.published_packed == key;
            merged.packed_banks.put_write(rows);
            match prev.as_ref().filter(|_| resident) {
                Some(prev) => merged.packed_share = Some(Arc::clone(prev)),
                None => {
                    merged.revision = merged.revision.wrapping_add(1);
                    merged.packed_share = Some(merged.packed_banks.published());
                    merged.published_packed = key;
                }
            }
            merged.packed_vertices = assets::RetailPackedVertexPayload::Unavailable {
                source_layout: XMODEL_PACKED_UNAVAILABLE,
            };
        }
        other => {
            if prev.is_some() {
                merged.revision = merged.revision.wrapping_add(1);
            }
            merged.packed_share = None;
            merged.published_packed = None;
            merged.packed_vertices = other;
        }
    }
}

fn publish_topology_shares(
    prev_index: Option<Arc<Vec<u32>>>,
    prev_range: Option<Arc<Vec<(u32, u32)>>>,
    indices: &mut Vec<u32>,
    ranges: &mut Vec<(u32, u32)>,
    index_share: &mut Option<Arc<Vec<u32>>>,
    range_share: &mut Option<Arc<Vec<(u32, u32)>>>,
    topology_revision: &mut u64,
    index_banks: &mut super::ShareBanks<u32>,
    range_banks: &mut super::ShareBanks<(u32, u32)>,
) {
    let indices_same = prev_index
        .as_ref()
        .is_some_and(|prev| prev.as_slice() == indices.as_slice());
    let ranges_same = prev_range
        .as_ref()
        .is_some_and(|prev| prev.as_slice() == ranges.as_slice());
    if indices_same && ranges_same {
        *index_share = prev_index;
        *range_share = prev_range;
        index_banks.put_write(std::mem::take(indices));
        range_banks.put_write(std::mem::take(ranges));
        return;
    }
    *topology_revision = topology_revision.wrapping_add(1);
    index_banks.put_write(std::mem::take(indices));
    range_banks.put_write(std::mem::take(ranges));
    *index_share = Some(index_banks.published());
    *range_share = Some(range_banks.published());
}

fn packed_row_count(payload: &assets::RetailPackedVertexPayload) -> usize {
    match payload {
        assets::RetailPackedVertexPayload::Iw4(rows) => rows.len(),
        assets::RetailPackedVertexPayload::Unavailable { .. } => 0,
    }
}

fn append_packed_source(
    packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    packed_ok: &mut bool,
    payload: &assets::RetailPackedVertexPayload,
    decoded_count: usize,
) {
    if !*packed_ok || decoded_count == 0 {
        return;
    }
    match payload {
        assets::RetailPackedVertexPayload::Iw4(vertices) if vertices.len() == decoded_count => {
            packed.extend_from_slice(vertices);
        }
        _ => {
            *packed_ok = false;
            packed.clear();
        }
    }
}

fn install_retained_packed(
    packed_ok: bool,
    packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    decoded_count: usize,
    empty: &'static str,
    missing: &'static str,
) -> assets::RetailPackedVertexPayload {
    if packed_ok && packed.len() == decoded_count && !packed.is_empty() {
        assets::RetailPackedVertexPayload::Iw4(packed)
    } else if decoded_count == 0 {
        assets::RetailPackedVertexPayload::Unavailable {
            source_layout: empty,
        }
    } else {
        assets::RetailPackedVertexPayload::Unavailable {
            source_layout: missing,
        }
    }
}

fn append_admitted_source(
    merged: &mut XModelDrawPlan,
    packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    packed_ok: &mut bool,
    decoded_n: usize,
    indices: &[u32],
    surface_ranges: &[(u32, u32)],
    materials: &[SmodelPassMaterial],
    packed_payload: &assets::RetailPackedVertexPayload,
    draws: impl IntoIterator<Item = XModelSurfaceDraw>,
) {
    let draws: Vec<XModelSurfaceDraw> = draws.into_iter().collect();
    if draws.is_empty() {
        return;
    }
    let mut used = vec![false; surface_ranges.len()];
    for d in &draws {
        if let Some(slot) = used.get_mut(d.surface as usize) {
            *slot = true;
        }
    }
    let all_surfaces_live = !used.is_empty() && used.iter().all(|&live| live);
    if all_surfaces_live {
        append_source_plan(
            merged,
            decoded_n,
            indices,
            surface_ranges,
            materials,
            draws.into_iter(),
        );
        append_packed_source(packed, packed_ok, packed_payload, decoded_n);
        return;
    }
    compact_admitted_source(
        merged,
        packed,
        packed_ok,
        decoded_n,
        indices,
        surface_ranges,
        materials,
        packed_payload,
        &used,
        draws,
    );
}

fn compact_admitted_source(
    merged: &mut XModelDrawPlan,
    packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    packed_ok: &mut bool,
    decoded_n: usize,
    indices: &[u32],
    surface_ranges: &[(u32, u32)],
    materials: &[SmodelPassMaterial],
    packed_payload: &assets::RetailPackedVertexPayload,
    used_surfaces: &[bool],
    draws: Vec<XModelSurfaceDraw>,
) {
    let source_rows = match packed_payload {
        assets::RetailPackedVertexPayload::Iw4(rows) if rows.len() == decoded_n => {
            Some(rows.as_slice())
        }
        _ => {
            if decoded_n != 0 {
                *packed_ok = false;
                packed.clear();
            }
            None
        }
    };
    const UNMAPPED: u32 = u32::MAX;
    let mut vert_remap = vec![UNMAPPED; decoded_n];
    let mut surf_remap = vec![UNMAPPED; surface_ranges.len()];
    let vert_base = merged.decoded_n as u32;
    let mut new_verts = 0u32;
    for (si, &(start, count)) in surface_ranges.iter().enumerate() {
        if !used_surfaces.get(si).copied().unwrap_or(false) {
            continue;
        }
        let start = start as usize;
        let end = start.saturating_add(count as usize);
        let Some(span) = indices.get(start..end) else {
            continue;
        };
        let new_start = merged.indices.len() as u32;
        for &old in span {
            let old = old as usize;
            let Some(slot) = vert_remap.get_mut(old) else {
                continue;
            };
            if *slot == UNMAPPED {
                *slot = vert_base.saturating_add(new_verts);
                if *packed_ok {
                    if let Some(rows) = source_rows {
                        packed.push(rows[old]);
                    }
                }
                new_verts = new_verts.saturating_add(1);
            }
            merged.indices.push(*slot);
        }
        surf_remap[si] = merged.surface_ranges.len() as u32;
        merged.surface_ranges.push((new_start, count));
    }
    let mut mat_remap = vec![UNMAPPED; materials.len()];
    let mut used_mat = vec![false; materials.len()];
    for d in &draws {
        if let Some(slot) = used_mat.get_mut(d.material as usize) {
            *slot = true;
        }
    }
    for (mi, live) in used_mat.iter().enumerate() {
        if !*live {
            continue;
        }
        mat_remap[mi] = merged.materials.len() as u32;
        merged.materials.push(materials[mi].clone());
    }
    merged.decoded_n = merged.decoded_n.saturating_add(new_verts as usize);
    merged.concat_layout = false;
    for mut d in draws {
        if let Some(&mapped) = surf_remap.get(d.surface as usize)
            && mapped != UNMAPPED
        {
            d.surface = mapped;
        }
        if let Some(&mapped) = mat_remap.get(d.material as usize)
            && mapped != UNMAPPED
        {
            d.material = mapped;
        }
        merged.draws.push(d);
    }
}

fn append_source_plan(
    merged: &mut XModelDrawPlan,
    decoded_n: usize,
    indices: &[u32],
    surface_ranges: &[(u32, u32)],
    materials: &[SmodelPassMaterial],
    draws: impl Iterator<Item = XModelSurfaceDraw>,
) {
    let vert_base = merged.decoded_n as u32;
    let index_base = merged.indices.len() as u32;
    let range_base = merged.surface_ranges.len() as u32;
    let mat_base = merged.materials.len() as u32;

    merged.decoded_n = merged.decoded_n.saturating_add(decoded_n);
    merged.indices.reserve(indices.len());
    for &i in indices {
        merged.indices.push(i.saturating_add(vert_base));
    }
    for &(start, count) in surface_ranges {
        merged
            .surface_ranges
            .push((start.saturating_add(index_base), count));
    }
    merged.materials.extend_from_slice(materials);
    for mut d in draws {
        d.surface = d.surface.saturating_add(range_base);
        d.material = d.material.saturating_add(mat_base);
        merged.draws.push(d);
    }
}

pub(crate) fn apply_resolved_xmodel_lighting(
    resolved: Res<crate::prepare::scene::model_lighting_cache::ResolvedModelLightingTable>,
    mut script: ResMut<ScriptModelDrawPlan>,
    mut item: ResMut<ItemDrawPlan>,
    mut missile: ResMut<MissileDrawPlan>,
    mut dynent: ResMut<DynEntDrawPlan>,
    mut fpv: ResMut<FpvDrawPlan>,
    mut focus: ResMut<crate::assemble::drawsurf::RenderFocus>,
) {
    let script = script.as_mut();
    let script_failed = script.finalize_lighting(&resolved);
    let item = item.as_mut();
    item.finalize_lighting(&resolved);
    let missile = missile.as_mut();
    missile.finalize_lighting(&resolved);
    let dynent = dynent.as_mut();
    dynent.finalize_lighting(&resolved);
    fpv.finalize_lighting(&resolved);
    if let Some(frame) = focus.frame.as_mut()
        && let Some(object_id) = frame.object_id
    {
        if script_failed.contains(&object_id) {
            frame.outcome = "lighting_failed";
            frame.lighting_handle = None;
        } else if let Some(handle) = script
            .draws()
            .iter()
            .find(|draw| draw.object_id == object_id)
            .map(|draw| draw.lighting_handle)
        {
            frame.lighting_handle = Some(handle);
        }
    }
}

pub(crate) fn apply_resolved_fx_model_lighting(
    resolved: Res<crate::prepare::scene::model_lighting_cache::ResolvedModelLightingTable>,
    mut plan: ResMut<FxModelDrawPlan>,
) {
    plan.finalize_lighting(&resolved);
}

#[cfg(test)]
mod producer_revision_tests {
    use super::*;
    use render_scene::model_lighting::{
        ModelLightingOwner, ModelLightingRequest, ResolvedModelLighting, ResolvedModelLightingTable,
    };

    /// Slot order of `producer_keys`, and therefore of every array in the
    /// stamp. A producer that moved must move its own slot and no other.
    const FPV: usize = 0;
    const BODIES: usize = 1;
    const SCRIPTS: usize = 2;
    const MISSILES: usize = 3;
    const ITEMS: usize = 4;
    const FX_MODELS: usize = 5;
    const DYNENTS: usize = 6;

    fn draw(object_id: u16, at: Vec3) -> XModelSurfaceDraw {
        XModelSurfaceDraw {
            surface: 0,
            material: 0,
            world_from_local: Mat4::from_translation(at),
            lighting_handle: 0,
            pending_lighting: Some(ModelLightingRequest {
                owner: ModelLightingOwner::Item(u32::from(object_id)),
                origin: [0.0; 3],
                lookup_fallback: 0,
            }),
            colour_refusal: None,
            object_id,
            scene_light_index: 0,
            reflection_probe_index: 0,
            packed_lighting: None,
            is_scope: false,
            scene_entnum: None,
        }
    }

    /// The seven plans the merge reads, driven only through the publishing API
    /// their owners use — never by writing their rows.
    #[derive(Default)]
    struct Producers {
        fpv: FpvDrawPlan,
        bodies: RemoteBodyDrawPlan,
        scripts: ScriptModelDrawPlan,
        missiles: MissileDrawPlan,
        items: ItemDrawPlan,
        fx_models: FxModelDrawPlan,
        dynents: DynEntDrawPlan,
    }

    impl Producers {
        /// What the merge itself asks of them each frame.
        fn stamp(&self) -> XModelMergeStamp {
            xmodel_merge_stamp(
                &self.fpv,
                &self.bodies,
                &self.scripts,
                &self.missiles,
                &self.items,
                &self.fx_models,
                &self.dynents,
                None,
                None,
            )
        }

        /// One frame's rebuild of every row-publishing producer, at `at`.
        fn rebuild_all_at(&mut self, at: Vec3) {
            let (mut draws, mut owners) = (
                vec![draw(1, at)],
                vec![ItemOwnerDraw {
                    object_id: 1,
                    model: "rpg".into(),
                }],
            );
            self.items.publish_frame_rows(&mut draws, &mut owners);

            let (mut draws, mut owners) = (
                vec![draw(2, at)],
                vec![DynEntOwnerDraw {
                    object_id: 2,
                    model: "door".into(),
                }],
            );
            self.dynents.publish_frame_rows(&mut draws, &mut owners);

            let (mut draws, mut owners) = (vec![draw(3, at)], Vec::new());
            self.scripts.publish_frame_rows(&mut draws, &mut owners);

            let mut staged = MissileDrawPlan::default();
            let (mut draws, mut owners) = (
                vec![draw(4, at)],
                vec![MissileOwnerDraw {
                    object_id: 4,
                    model: "rocket".into(),
                }],
            );
            self.missiles
                .publish_rebuild(&mut staged, &mut draws, &mut owners);

            let mut staged = FxModelDrawPlan::default();
            staged.push_draw(draw(5, at));
            self.fx_models.publish_rebuild(&mut staged);
        }
    }

    fn moved_slots(before: &XModelMergeStamp, after: &XModelMergeStamp) -> Vec<usize> {
        (0..8)
            .filter(|&i| {
                before.draws[i] != after.draws[i]
                    || before.vertices[i] != after.vertices[i]
                    || before.topology.admission[i] != after.topology.admission[i]
                    || before.producer_revision[i] != after.producer_revision[i]
            })
            .collect()
    }

    fn seated(handle: u32) -> ResolvedModelLighting {
        ResolvedModelLighting::Seated {
            handle,
            scene_light_index: 2,
            reflection_probe_index: 3,
            packed_lighting: Some([1, 2, 3, 4]),
        }
    }

    /// The merge used to re-hash every draw, material and vertex count of all
    /// seven producers each frame, because what they published was not the
    /// truth: some restarted their revisions from zero, some bumped on a
    /// rebuild that changed nothing, some dropped every row without telling
    /// anyone. This drives the producers through their own publishing API and
    /// asks the consumer that replaced those hashes — the merge stamp — whether
    /// it can still tell a frame that moved from one that did not, and which
    /// producer it was.
    #[test]
    fn the_merge_stamp_moves_for_exactly_the_producers_whose_rows_moved() {
        let mut plans = Producers::default();
        let empty = plans.stamp();

        plans.rebuild_all_at(Vec3::ZERO);
        let published = plans.stamp();
        assert_ne!(published, empty);
        assert_eq!(
            moved_slots(&empty, &published),
            vec![SCRIPTS, MISSILES, ITEMS, FX_MODELS, DYNENTS]
        );
        assert_eq!(plans.items.draws().len(), 1);

        // An identical rebuild is not a change, and must not look like one:
        // this is the claim that let the merge stop re-hashing the rows.
        plans.rebuild_all_at(Vec3::ZERO);
        assert_eq!(plans.stamp(), published);
        assert!(!concat_draws_changed(published, &plans.stamp()));

        // Moving one row is — and it moves that producer's slot alone.
        plans.rebuild_all_at(Vec3::ZERO);
        let mut draws = vec![draw(1, Vec3::X)];
        let mut owners = vec![ItemOwnerDraw {
            object_id: 1,
            model: "rpg".into(),
        }];
        plans.items.publish_frame_rows(&mut draws, &mut owners);
        let moved = plans.stamp();
        assert_eq!(moved_slots(&published, &moved), vec![ITEMS]);
        assert!(concat_draws_changed(published, &moved));
        assert_eq!(
            plans.items.draws()[0].world_from_local,
            Mat4::from_translation(Vec3::X)
        );

        // Dropping rows is a change too: a producer with nothing to draw may
        // not leave last frame's rows standing under last frame's revision.
        plans.items.publish_no_rows();
        let dropped = plans.stamp();
        assert!(plans.items.draws().is_empty());
        assert_eq!(moved_slots(&moved, &dropped), vec![ITEMS]);
        assert!(concat_draws_changed(moved, &dropped));
        // And having dropped them, saying so again is not.
        plans.items.publish_no_rows();
        assert_eq!(plans.stamp(), dropped);

        // Same contract for the geometry-owning producers.
        plans.bodies.clear_geometry();
        assert_eq!(
            plans.stamp(),
            dropped,
            "a body plan with no rows had none to drop"
        );
        plans
            .missiles
            .finalize_lighting(&ResolvedModelLightingTable::default());
        let failed = plans.stamp();
        assert!(
            plans.missiles.draws().is_empty(),
            "unresolved lighting removes the owner"
        );
        assert_eq!(moved_slots(&dropped, &failed), vec![MISSILES]);
        // Once, not once per frame it stays failed.
        plans
            .missiles
            .finalize_lighting(&ResolvedModelLightingTable::default());
        assert_eq!(plans.stamp(), failed);

        // A resolved payload rewrites the rows, so the draws revision moves —
        // but it touches neither the vertices nor the index layout the merge
        // concatenates, and it is not an admission change.
        let mut resolved = ResolvedModelLightingTable::default();
        resolved.insert_if_absent(ModelLightingOwner::Item(3), seated(7));
        plans.scripts.finalize_lighting(&resolved);
        let lit = plans.stamp();
        assert_eq!(plans.scripts.draws()[0].lighting_handle, 7);
        assert_eq!(moved_slots(&failed, &lit), vec![SCRIPTS]);
        assert_ne!(lit.draws[SCRIPTS], failed.draws[SCRIPTS]);
        assert_eq!(lit.vertices[SCRIPTS], failed.vertices[SCRIPTS]);
        assert_eq!(
            lit.topology.topology[SCRIPTS],
            failed.topology.topology[SCRIPTS]
        );
        assert_eq!(
            lit.topology.admission[SCRIPTS],
            failed.topology.admission[SCRIPTS]
        );
        // The same resolved table again is a no-op: the request was consumed.
        plans.scripts.finalize_lighting(&resolved);
        assert_eq!(plans.stamp(), lit);

        // The viewmodel is admission, not vertices: hiding it must reach the
        // merge, and must not invalidate a vertex buffer.
        plans.fpv.visible = true;
        plans.fpv.lighting_handle = 7;
        plans
            .fpv
            .finalize_lighting(&ResolvedModelLightingTable::default());
        let hidden = plans.stamp();
        assert!(!plans.fpv.visible);
        assert_eq!(moved_slots(&lit, &hidden), vec![FPV]);
        assert_eq!(hidden.vertices[FPV], lit.vertices[FPV]);
        assert_ne!(hidden.topology.admission[FPV], lit.topology.admission[FPV]);
        assert!(concat_draws_changed(lit, &hidden));
        plans
            .fpv
            .finalize_lighting(&ResolvedModelLightingTable::default());
        assert_eq!(plans.stamp(), hidden);

        // Nothing above ever touched the body plan, and the merge agrees.
        assert_eq!(hidden.producer_revision[BODIES], 0);
        assert_eq!(hidden.draws[BODIES], 0);
    }
}
