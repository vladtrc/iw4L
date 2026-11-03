use render_anim::geometry::install_retained_packed;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use bevy::prelude::*;

use super::smodel::{PackedVertexRefusal, SmodelPassMaterial, SmodelVertex};

pub use render_anim::{
    BODY_PACKED_UNAVAILABLE, DynEntAssetDraw, DynEntDrawPlan, DynEntOwnerDraw,
    FPV_PACKED_EMPTY_PLAN, FPV_PACKED_UNAVAILABLE, FpvDrawPlan, FpvSurfaceDraw, ItemAssetDraw,
    ItemDrawPlan, ItemOwnerDraw, MissileDrawPlan, MissileOwnerDraw, RemoteBodyDrawPlan,
    RemoteBodySurfaceDraw, ScriptModelAssetDraw, ScriptModelDrawPlan, ScriptModelOwnerDraw,
    XMODEL_OBJECT_ID_BODY_BASE, XMODEL_OBJECT_ID_DYNENT_BASE, XMODEL_OBJECT_ID_ITEM_BASE,
    XMODEL_OBJECT_ID_MISSILE_BASE,
};
pub use render_anim::{
    BodyPackedSession, append_dynent_asset, append_dynent_surfaces, append_item_surfaces,
    append_missile_surfaces, append_remote_body_cpu_blob, append_script_model_asset,
    authored_lit_xmodel_pass_material, body_lit_pass_material, bound_lit_xmodel_pass_material,
    finish_remote_body_draw_plan, install_body_packed_session, overwrite_script_model_asset,
    push_remote_body_cpu_draw, retain_script_model_assets, take_body_packed_session,
};
pub use render_fx::append_fx_model_asset;

pub use render_frame::XMODEL_OBJECT_ID_VIEWMODEL;
pub use render_fx::{FxModelAssetDraw, FxModelDrawPlan, XMODEL_OBJECT_ID_FX_BASE};
pub use render_scene::{XModelColourRefusal, XModelSurfaceDraw};

#[derive(Resource, Clone, Debug, Default)]
pub struct XModelDrawPlan {
    pub skin_jobs: Vec<render_frame::skinning::PackedSkinJob>,
    pub vertices: Vec<SmodelVertex>,

    pub decoded_n: usize,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub draws: Vec<XModelSurfaceDraw>,

    pub fx_object_id_exhausted: u32,
    pub revision: u64,

    pub topology_revision: u64,

    pub packed_vertices: asset_world::PackedVertexPayload,

    pub packed_share: Option<Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>>,

    published_packed: Option<PackedContentKey>,

    pub index_share: Option<Arc<Vec<u32>>>,
    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
    pub decoded_share: Option<Arc<Vec<SmodelVertex>>>,
    packed_banks: super::ShareBanks<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    index_banks: super::ShareBanks<u32>,
    range_banks: super::ShareBanks<(u32, u32)>,

    last_input: Option<XModelMergeStamp>,
    last_topology: Option<XModelTopologyKey>,
    concat_layout: bool,
    last_admitted: u8,
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
        self.skin_jobs.clear();
        self.vertices.clear();
        self.decoded_n = 0;
        self.indices.clear();
        self.surface_ranges.clear();
        self.materials.clear();
        self.draws.clear();
        self.fx_object_id_exhausted = 0;
        self.packed_share = None;
        self.index_share = None;
        self.range_share = None;
        self.decoded_share = None;
        self.last_input = None;
        self.last_topology = None;
        self.concat_layout = false;
        self.last_admitted = 0;
        self.packed_vertices = asset_world::PackedVertexPayload::Unavailable {
            source_layout: XMODEL_PACKED_UNAVAILABLE,
        };
    }

    pub fn packed_rows(&self) -> Option<&[[u8; asset_iw4::size::GFX_PACKED_VERTEX]]> {
        if let Some(share) = self.packed_share.as_ref() {
            return Some(share.as_slice());
        }
        match &self.packed_vertices {
            asset_world::PackedVertexPayload::Iw4(rows) => Some(rows.as_slice()),
            asset_world::PackedVertexPayload::Unavailable { .. } => None,
        }
    }

    pub fn index_rows(&self) -> &[u32] {
        super::published_rows(&self.index_share)
    }

    pub fn range_rows(&self) -> &[(u32, u32)] {
        super::published_rows(&self.range_share)
    }

    pub fn exact_packed_vertices(
        &self,
    ) -> Result<&[[u8; asset_iw4::size::GFX_PACKED_VERTEX]], PackedVertexRefusal> {
        let table_stride =
            asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::PACKED_VERTEX_TYPE, 0);
        if table_stride != Some(asset_iw4::size::GFX_PACKED_VERTEX as u16) {
            return Err(PackedVertexRefusal::StrideMismatch { table_stride });
        }
        let vertices = match self.packed_rows() {
            Some(vertices) => vertices,
            None => {
                let source_layout = match &self.packed_vertices {
                    asset_world::PackedVertexPayload::Unavailable { source_layout } => {
                        *source_layout
                    }
                    asset_world::PackedVertexPayload::Iw4(_) => XMODEL_PACKED_UNAVAILABLE,
                };
                return Err(PackedVertexRefusal::ForeignLayout { source_layout });
            }
        };
        let decoded = if self.vertices.is_empty() {
            self.decoded_n
        } else {
            self.vertices.len()
        };
        if vertices.len() != decoded {
            return Err(PackedVertexRefusal::VertexCountMismatch {
                packed: vertices.len(),
                decoded,
            });
        }
        Ok(vertices)
    }
    fn publish_topology_shares(
        &mut self,
        prev_index: Option<Arc<Vec<u32>>>,
        prev_range: Option<Arc<Vec<(u32, u32)>>>,
    ) {
        let Self {
            indices,
            surface_ranges: ranges,
            index_share,
            range_share,
            topology_revision,
            index_banks,
            range_banks,
            ..
        } = self;

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

#[must_use]
fn script_draw_admitted(scene: Option<&crate::GfxScene>, draw: &XModelSurfaceDraw) -> bool {
    scene_ent_admitted(scene, draw.scene_entnum)
        || (draw.colour_refusal.is_some()
            && draw
                .scene_entnum
                .is_some_and(|n| scene.is_some_and(|s| s.scene_ent_hidden(n))))
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
    materials: [u64; 8],
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

struct ProducerKeys {
    generation: [u64; 8],
    topology: [u64; 8],
    admission: [u64; 8],
    vertices: [u64; 8],
    materials: [u64; 8],
    draws: [u64; 8],
    producer_revision: [u64; 8],
}

#[derive(Clone, Copy)]
pub struct XModelProducers<'a> {
    pub fpv: &'a FpvDrawPlan,
    pub bodies: &'a RemoteBodyDrawPlan,
    pub scripts: &'a ScriptModelDrawPlan,
    pub missiles: &'a MissileDrawPlan,
    pub items: &'a ItemDrawPlan,
    pub fx_models: &'a FxModelDrawPlan,
    pub dynents: &'a DynEntDrawPlan,
}

impl<'a> XModelProducers<'a> {
    fn keys(self, sky: Option<&super::sky::SkyModelDrawPlan>) -> ProducerKeys {
        let Self {
            fpv,
            bodies,
            scripts,
            missiles,
            items,
            fx_models,
            dynents,
        } = self;
        let sky_rev = sky
            .map(|sky| sky.revisions)
            .unwrap_or_else(render_frame::SourceRevisions::default);
        let sky_gen = sky.map(|sky| sky.generation).unwrap_or(0);
        ProducerKeys {
            generation: [
                fpv.generation,
                bodies.generation,
                scripts.generation,
                missiles.generation,
                items.generation,
                fx_models.generation,
                dynents.generation,
                sky_gen,
            ],
            topology: [
                fpv.revisions.topology,
                bodies.revisions.topology,
                scripts.revisions.topology,
                missiles.revisions.topology,
                items.revisions.topology,
                fx_models.revisions.topology,
                dynents.revisions.topology,
                sky_rev.topology,
            ],
            admission: [
                fpv.revisions.admission,
                bodies.revisions.admission,
                scripts.revisions.admission,
                missiles.revisions.admission,
                items.revisions.admission,
                fx_models.revisions.admission,
                dynents.revisions.admission,
                sky_rev.admission,
            ],
            vertices: [
                fpv.revisions.vertices,
                bodies.revisions.vertices,
                scripts.revisions.vertices,
                missiles.revisions.vertices,
                items.revisions.vertices,
                fx_models.revisions.vertices,
                dynents.revisions.vertices,
                sky_rev.vertices,
            ],
            materials: [
                fpv.revisions.materials,
                bodies.revisions.materials,
                scripts.revisions.materials,
                missiles.revisions.materials,
                items.revisions.materials,
                fx_models.revisions.materials,
                dynents.revisions.materials,
                sky_rev.materials,
            ],
            draws: [
                fpv.revisions.draws,
                bodies.revisions.draws,
                scripts.revisions.draws,
                missiles.revisions.draws,
                items.revisions.draws,
                fx_models.revisions.draws,
                dynents.revisions.draws,
                sky_rev.draws,
            ],
            producer_revision: [
                fpv.revision,
                bodies.revision,
                scripts.revision,
                missiles.revision,
                items.revision,
                fx_models.revision,
                dynents.revision,
                0,
            ],
        }
    }

    fn merge_stamp(
        self,
        scene: Option<&crate::GfxScene>,
        sky: Option<(
            &super::sky::SkyModelDrawPlan,
            Vec3,
            Option<&render_frame::PortalView>,
        )>,
    ) -> XModelMergeStamp {
        let Self {
            fpv,
            bodies,
            scripts,
            missiles,
            items,
            fx_models,
            dynents,
        } = self;
        let mut occupancy = DefaultHasher::new();
        fpv_scene_colour(fpv, scene).hash(&mut occupancy);
        for draw in bodies.draws() {
            scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
        }
        for draw in scripts.draws() {
            script_draw_admitted(scene, draw).hash(&mut occupancy);
        }
        for draw in missiles.draws() {
            scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
        }
        for draw in items.draws() {
            scene_ent_admitted(scene, draw.scene_entnum).hash(&mut occupancy);
        }
        (!fx_models.draws().is_empty()).hash(&mut occupancy);
        (!dynents.draws().is_empty()).hash(&mut occupancy);
        let sky_eye = sky.map(|(_, eye, _)| [eye.x.to_bits(), eye.y.to_bits(), eye.z.to_bits()]);
        let keys = self.keys(sky.map(|(sky, _, _)| sky));
        XModelMergeStamp {
            topology: XModelTopologyKey {
                generation: keys.generation,
                topology: keys.topology,
                admission: keys.admission,
                occupancy: occupancy.finish(),
            },
            vertices: keys.vertices,
            materials: keys.materials,
            draws: keys.draws,
            producer_revision: keys.producer_revision,
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

    pub fn merge_into(
        self,
        merged: &mut XModelDrawPlan,
        scene: Option<&crate::GfxScene>,
        sky: Option<(
            &'a super::sky::SkyModelDrawPlan,
            Vec3,
            Option<&'a render_frame::PortalView>,
        )>,
    ) -> XModelMergeStamps {
        let Self {
            fpv,
            bodies,
            scripts,
            missiles,
            items,
            fx_models,
            dynents,
        } = self;
        let stamp = self.merge_stamp(scene, sky);
        if merged.last_input == Some(stamp) {
            publish_skin_jobs(merged, fpv, bodies);
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
                let mut packed = merged.packed_banks.take_write();
                let mut packed_ok = true;
                recopy_concat_packed_bank(
                    &mut packed,
                    &mut packed_ok,
                    merged.last_admitted,
                    self.packed_owners(sky),
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
                self.refresh_draws(merged, scene, sky);
            }
            merged.last_input = Some(stamp);
            publish_skin_jobs(merged, fpv, bodies);
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
        let mut admitted = 0u8;
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
        if (XModelGeometrySource {
            decoded_n: bodies.decoded_n(),
            indices: bodies.indices(),
            surface_ranges: bodies.surface_ranges(),
            materials: bodies.materials(),
            packed_payload: bodies.packed_vertices(),
        })
        .append_admitted(
            merged,
            &mut packed,
            &mut packed_ok,
            bodies
                .draws()
                .iter()
                .filter(|&d| scene_ent_admitted(scene, d.scene_entnum))
                .map(|d| {
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
                        body_client: d.scene_entnum,
                        // Remote bodies carry no radius here; unbounded keeps
                        // every player caster in both partitions.
                        caster_bound: None,
                    }
                }),
        ) {
            admitted |= ADMIT_BODY;
        }

        if fpv_scene_colour(fpv, scene)
            && (XModelGeometrySource {
                decoded_n: fpv.decoded_n(),
                indices: fpv.indices(),
                surface_ranges: fpv.surface_ranges(),
                materials: fpv.materials(),
                packed_payload: fpv.packed_vertices(),
            })
            .append_admitted(
                merged,
                &mut packed,
                &mut packed_ok,
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
                    body_client: None,
                    caster_bound: None,
                }),
            )
        {
            admitted |= ADMIT_FPV;
        }

        if (XModelGeometrySource {
            decoded_n: scripts.vertices().len(),
            indices: scripts.indices(),
            surface_ranges: scripts.surface_ranges(),
            materials: scripts.materials(),
            packed_payload: scripts.packed_vertices(),
        })
        .append_admitted(
            merged,
            &mut packed,
            &mut packed_ok,
            scripts
                .draws()
                .iter()
                .copied()
                .filter(|d| script_draw_admitted(scene, d)),
        ) {
            admitted |= ADMIT_SCRIPT;
        }

        if (XModelGeometrySource {
            decoded_n: missiles.vertices().len(),
            indices: missiles.indices(),
            surface_ranges: missiles.surface_ranges(),
            materials: missiles.materials(),
            packed_payload: missiles.packed_vertices(),
        })
        .append_admitted(
            merged,
            &mut packed,
            &mut packed_ok,
            missiles
                .draws()
                .iter()
                .copied()
                .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
        ) {
            admitted |= ADMIT_MISSILE;
        }

        if (XModelGeometrySource {
            decoded_n: items.vertices().len(),
            indices: items.indices(),
            surface_ranges: items.surface_ranges(),
            materials: items.materials(),
            packed_payload: items.packed_vertices(),
        })
        .append_admitted(
            merged,
            &mut packed,
            &mut packed_ok,
            items
                .draws()
                .iter()
                .copied()
                .filter(|d| scene_ent_admitted(scene, d.scene_entnum)),
        ) {
            admitted |= ADMIT_ITEM;
        }
        if !fx_draws.is_empty() {
            admitted |= ADMIT_FX;
            append_source_plan(
                merged,
                fx_models.vertices().len(),
                fx_models.indices(),
                fx_models.surface_ranges(),
                fx_models.materials(),
                fx_draws.into_iter(),
            );
            append_packed_source(
                &mut packed,
                &mut packed_ok,
                fx_models.packed_vertices(),
                fx_models.vertices().len(),
            );
        }
        if (XModelGeometrySource {
            decoded_n: dynents.vertices().len(),
            indices: dynents.indices(),
            surface_ranges: dynents.surface_ranges(),
            materials: dynents.materials(),
            packed_payload: dynents.packed_vertices(),
        })
        .append_admitted(
            merged,
            &mut packed,
            &mut packed_ok,
            dynents.draws().iter().copied(),
        ) {
            admitted |= ADMIT_DYNENT;
        }
        if let Some((sky, eye, view)) = sky
            && (XModelGeometrySource {
                decoded_n: sky.geometry.decoded_vertices().len(),
                indices: sky.geometry.indices(),
                surface_ranges: sky.geometry.surface_ranges(),
                materials: &sky.geometry.materials,
                packed_payload: &sky.geometry.packed_vertices,
            })
            .append_admitted(
                merged,
                &mut packed,
                &mut packed_ok,
                sky.draws_at(eye, view),
            )
        {
            admitted |= ADMIT_SKY;
        }
        let append_ms = append_started.elapsed().as_secs_f32() * 1000.0;
        let packed_started = Instant::now();
        merged.last_admitted = admitted;
        let key =
            packed_ok.then(|| PackedContentKey::new(&stamp, merged.last_admitted, packed.len()));
        let payload = install_retained_packed(
            packed_ok,
            packed,
            merged.decoded_n,
            XMODEL_PACKED_EMPTY_PLAN,
            XMODEL_PACKED_UNAVAILABLE,
        );
        publish_packed_payload(merged, prev_packed, payload, key);
        merged.publish_topology_shares(prev_index, prev_range);
        publish_decoded_share(merged);
        merged.last_input = Some(stamp);
        merged.last_topology = Some(stamp.topology);
        publish_skin_jobs(merged, fpv, bodies);
        let packed_ms = packed_started.elapsed().as_secs_f32() * 1000.0;
        XModelMergeStamps {
            append_ms,
            packed_ms,
            concatenated: true,
        }
    }

    fn packed_owners(
        self,
        sky: Option<(
            &'a super::sky::SkyModelDrawPlan,
            Vec3,
            Option<&'a render_frame::PortalView>,
        )>,
    ) -> [ConcatPackedOwner<'a>; 8] {
        let Self {
            fpv,
            bodies,
            scripts,
            missiles,
            items,
            fx_models,
            dynents,
        } = self;
        let sky_geom = sky.map(|(sky, _, _)| &sky.geometry);
        [
            ConcatPackedOwner {
                admit: ADMIT_BODY,
                payload: bodies.packed_vertices(),
                decoded_n: bodies.decoded_n(),
            },
            ConcatPackedOwner {
                admit: ADMIT_FPV,
                payload: fpv.packed_vertices(),
                decoded_n: fpv.decoded_n(),
            },
            ConcatPackedOwner {
                admit: ADMIT_SCRIPT,
                payload: scripts.packed_vertices(),
                decoded_n: scripts.vertices().len(),
            },
            ConcatPackedOwner {
                admit: ADMIT_MISSILE,
                payload: missiles.packed_vertices(),
                decoded_n: missiles.vertices().len(),
            },
            ConcatPackedOwner {
                admit: ADMIT_ITEM,
                payload: items.packed_vertices(),
                decoded_n: items.vertices().len(),
            },
            ConcatPackedOwner {
                admit: ADMIT_FX,
                payload: fx_models.packed_vertices(),
                decoded_n: fx_models.vertices().len(),
            },
            ConcatPackedOwner {
                admit: ADMIT_DYNENT,
                payload: dynents.packed_vertices(),
                decoded_n: dynents.vertices().len(),
            },
            ConcatPackedOwner {
                admit: ADMIT_SKY,
                payload: sky_geom
                    .map(|g| &g.packed_vertices)
                    .unwrap_or(bodies.packed_vertices()),
                decoded_n: sky_geom.map(|g| g.decoded_vertices().len()).unwrap_or(0),
            },
        ]
    }

    fn refresh_draws(
        self,
        merged: &mut XModelDrawPlan,
        scene: Option<&crate::GfxScene>,
        sky: Option<(
            &super::sky::SkyModelDrawPlan,
            Vec3,
            Option<&render_frame::PortalView>,
        )>,
    ) {
        let Self {
            fpv,
            bodies,
            scripts,
            missiles,
            items,
            fx_models,
            dynents,
        } = self;
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
            bodies
                .draws()
                .iter()
                .filter(|&d| scene_ent_admitted(scene, d.scene_entnum))
                .map(|d| {
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
                        body_client: d.scene_entnum,
                        // Remote bodies carry no radius here; unbounded keeps
                        // every player caster in both partitions.
                        caster_bound: None,
                    }
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
                    body_client: None,
                    caster_bound: None,
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
                .filter(|d| script_draw_admitted(scene, d)),
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
        if let Some((sky, eye, view)) = sky {
            append_concat_owner_draws(
                merged,
                &mut range_base,
                sky.geometry.surface_ranges().len(),
                &sky.geometry.materials,
                sky.draws_at(eye, view),
            );
        }
    }
}

fn mat4_bits(m: Mat4) -> [u32; 16] {
    let cols = m.to_cols_array();
    std::array::from_fn(|i| cols[i].to_bits())
}

fn publish_skin_jobs(merged: &mut XModelDrawPlan, fpv: &FpvDrawPlan, bodies: &RemoteBodyDrawPlan) {
    merged.skin_jobs.clear();
    if merged.last_admitted & ADMIT_BODY != 0 {
        merged.skin_jobs.extend(bodies.skin_jobs.iter().cloned());
    }
    if merged.last_admitted & ADMIT_FPV == 0 {
        return;
    }
    let base = if merged.last_admitted & ADMIT_BODY != 0 {
        bodies.decoded_n()
    } else {
        0
    };
    for job in &fpv.skin_jobs {
        let mut job = job.clone();
        job.destination = u32::try_from(base)
            .expect("xmodel vertex base fits u32")
            .checked_add(job.destination)
            .expect("xmodel skin destination fits u32");
        merged.skin_jobs.push(job);
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
    prev.draws != stamp.draws
        || prev.materials != stamp.materials
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
    payload: &'a asset_world::PackedVertexPayload,
    decoded_n: usize,
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

fn recopy_concat_packed_bank(
    packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    packed_ok: &mut bool,
    admitted: u8,
    owners: [ConcatPackedOwner<'_>; 8],
) {
    let (_, expected) = concat_packed_layout(admitted, &owners);
    packed.clear();
    packed.reserve(expected);
    for owner in &owners {
        if admitted & owner.admit != 0 {
            append_packed_source(packed, packed_ok, owner.payload, owner.decoded_n);
        }
    }
}

fn append_concat_owner_draws(
    merged: &mut XModelDrawPlan,
    range_base: &mut u32,
    surface_n: usize,
    materials: &[SmodelPassMaterial],
    draws: impl IntoIterator<Item = XModelSurfaceDraw>,
) {
    let mat_base = merged.materials.len() as u32;
    let mut any = false;
    for mut d in draws {
        if !any {
            any = true;
            merged.materials.extend_from_slice(materials);
        }
        d.surface = d.surface.saturating_add(*range_base);
        d.material = d.material.saturating_add(mat_base);
        merged.draws.push(d);
    }
    if !any {
        return;
    }
    *range_base = range_base.saturating_add(surface_n as u32);
}

fn publish_packed_payload(
    merged: &mut XModelDrawPlan,
    prev: Option<Arc<Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>>>,
    payload: asset_world::PackedVertexPayload,
    key: Option<PackedContentKey>,
) {
    match payload {
        asset_world::PackedVertexPayload::Iw4(rows) => {
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
            merged.packed_vertices = asset_world::PackedVertexPayload::Unavailable {
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

fn publish_decoded_share(merged: &mut XModelDrawPlan) {
    if merged.vertices.is_empty() {
        merged.decoded_share = None;
        return;
    }
    merged.decoded_share = Some(Arc::new(std::mem::take(&mut merged.vertices)));
}

fn packed_row_count(payload: &asset_world::PackedVertexPayload) -> usize {
    match payload {
        asset_world::PackedVertexPayload::Iw4(rows) => rows.len(),
        asset_world::PackedVertexPayload::Unavailable { .. } => 0,
    }
}

fn append_packed_source(
    packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    packed_ok: &mut bool,
    payload: &asset_world::PackedVertexPayload,
    decoded_count: usize,
) {
    if !*packed_ok || decoded_count == 0 {
        return;
    }
    match payload {
        asset_world::PackedVertexPayload::Iw4(vertices) if vertices.len() == decoded_count => {
            packed.extend_from_slice(vertices);
        }
        _ => {
            *packed_ok = false;
            packed.clear();
        }
    }
}

#[derive(Clone, Copy)]
struct XModelGeometrySource<'a> {
    decoded_n: usize,
    indices: &'a [u32],
    surface_ranges: &'a [(u32, u32)],
    materials: &'a [SmodelPassMaterial],
    packed_payload: &'a asset_world::PackedVertexPayload,
}

impl XModelGeometrySource<'_> {
    fn append_admitted(
        self,
        merged: &mut XModelDrawPlan,
        packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
        packed_ok: &mut bool,
        draws: impl IntoIterator<Item = XModelSurfaceDraw>,
    ) -> bool {
        let Self {
            decoded_n,
            indices,
            surface_ranges,
            materials,
            packed_payload,
        } = self;
        let draws: Vec<XModelSurfaceDraw> = draws.into_iter().collect();
        if draws.is_empty() {
            return false;
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
            return true;
        }
        self.compact_admitted(merged, packed, packed_ok, &used, draws);
        true
    }

    fn compact_admitted(
        self,
        merged: &mut XModelDrawPlan,
        packed: &mut Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
        packed_ok: &mut bool,
        used_surfaces: &[bool],
        draws: Vec<XModelSurfaceDraw>,
    ) {
        let Self {
            decoded_n,
            indices,
            surface_ranges,
            materials,
            packed_payload,
        } = self;
        let source_rows = match packed_payload {
            asset_world::PackedVertexPayload::Iw4(rows) if rows.len() == decoded_n => {
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
                    if *packed_ok && let Some(rows) = source_rows {
                        packed.push(rows[old]);
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
