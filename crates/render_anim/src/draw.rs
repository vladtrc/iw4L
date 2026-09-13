use bevy::prelude::*;
use render_frame::SmodelVertex;
use render_scene::{SmodelPassMaterial, XModelSurfaceDraw};

pub use render_frame::SourceRevisions;

#[derive(Clone, Copy, Debug)]
pub struct FpvSurfaceDraw {
    pub surface: u32,
    pub material: u32,

    pub is_scope: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct RemoteBodySurfaceDraw {
    pub surface: u32,
    pub material: u32,
    pub world_from_local: Mat4,

    pub lighting_handle: u32,

    pub scene_light_index: u8,

    pub reflection_probe_index: u8,

    pub scene_entnum: Option<u32>,
}

pub const FPV_PACKED_UNAVAILABLE: &str =
    "fpv GfxPackedVertex missing or count-mismatched; decoded float is not packed VB";
pub const FPV_PACKED_EMPTY_PLAN: &str = "fpv plan has no vertices";

pub const BODY_PACKED_UNAVAILABLE: &str = "remote body GfxPackedVertex not retained";

pub const XMODEL_OBJECT_ID_BODY_BASE: u16 = 2;

pub const XMODEL_OBJECT_ID_MISSILE_BASE: u16 = 0x400;

pub const XMODEL_OBJECT_ID_ITEM_BASE: u16 = 0x500;

pub const XMODEL_OBJECT_ID_DYNENT_BASE: u16 = 0x600;

#[derive(Resource, Clone, Debug, Default)]
pub struct RemoteBodyDrawPlan {
    pub vertices: Vec<SmodelVertex>,

    pub decoded_n: usize,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub draws: Vec<RemoteBodySurfaceDraw>,

    pub revision: u64,

    pub generation: u64,

    pub revisions: SourceRevisions,

    pub packed_vertices: assets::RetailPackedVertexPayload,

    pub last_packed_id: Option<u64>,
    pub last_draw_id: Option<u64>,
}

impl RemoteBodyDrawPlan {
    pub fn clear_geometry(&mut self) {
        self.vertices.clear();
        self.decoded_n = 0;
        self.indices.clear();
        self.surface_ranges.clear();
        self.materials.clear();
        self.draws.clear();
        self.packed_vertices = assets::RetailPackedVertexPayload::Unavailable {
            source_layout: BODY_PACKED_UNAVAILABLE,
        };
        self.last_packed_id = None;
        self.last_draw_id = None;
    }
}

#[derive(Clone, Debug)]
pub struct ScriptModelAssetDraw {
    pub key: assets::MapXModelAssetKey,
    pub dobj_state: assets::dobj::DObjSemanticState,

    pub camera_lods: Vec<Option<u8>>,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Clone, Debug)]
pub struct ScriptModelOwnerDraw {
    pub entity: Entity,
    pub current_model: assets::MapXModelAssetKey,
    pub object_id: u16,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ScriptModelDrawPlan {
    pub vertices: Vec<SmodelVertex>,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub assets: Vec<ScriptModelAssetDraw>,
    pub owners: Vec<ScriptModelOwnerDraw>,
    pub draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub packed_vertices: assets::RetailPackedVertexPayload,
}

impl ScriptModelDrawPlan {
    pub fn asset_index(
        &self,
        key: &assets::MapXModelAssetKey,
        dobj_state: &assets::dobj::DObjSemanticState,
        camera_lods: &[Option<u8>],
    ) -> Option<usize> {
        self.assets.iter().position(|asset| {
            &asset.key == key && &asset.dobj_state == dobj_state && asset.camera_lods == camera_lods
        })
    }
}

#[derive(Clone, Debug)]
pub struct MissileOwnerDraw {
    pub object_id: u16,
    pub model: String,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct MissileDrawPlan {
    pub vertices: Vec<SmodelVertex>,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub owners: Vec<MissileOwnerDraw>,
    pub draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub packed_vertices: assets::RetailPackedVertexPayload,
}

#[derive(Clone, Debug)]
pub struct ItemOwnerDraw {
    pub object_id: u16,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct ItemAssetDraw {
    pub model: String,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ItemDrawPlan {
    pub vertices: Vec<SmodelVertex>,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub assets: Vec<ItemAssetDraw>,
    pub owners: Vec<ItemOwnerDraw>,
    pub draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub packed_vertices: assets::RetailPackedVertexPayload,
}

#[derive(Clone, Debug)]
pub struct DynEntOwnerDraw {
    pub object_id: u16,
    pub model: String,
}

#[derive(Clone, Debug)]
pub struct DynEntAssetDraw {
    pub key: assets::MapXModelAssetKey,

    pub camera_lod: Option<u8>,
    pub surfaces: Vec<(u32, u32)>,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct DynEntDrawPlan {
    pub vertices: Vec<SmodelVertex>,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub assets: Vec<DynEntAssetDraw>,
    pub owners: Vec<DynEntOwnerDraw>,
    pub draws: Vec<XModelSurfaceDraw>,
    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,
    pub packed_vertices: assets::RetailPackedVertexPayload,
}

impl DynEntDrawPlan {
    pub fn asset(
        &self,
        key: &assets::MapXModelAssetKey,
        camera_lod: Option<u8>,
    ) -> Option<&DynEntAssetDraw> {
        self.assets
            .iter()
            .find(|asset| &asset.key == key && asset.camera_lod == camera_lod)
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct FpvDrawPlan {
    pub vertices: Vec<SmodelVertex>,
    pub indices: Vec<u32>,
    pub surface_ranges: Vec<(u32, u32)>,
    pub materials: Vec<SmodelPassMaterial>,
    pub draws: Vec<FpvSurfaceDraw>,

    pub world_from_local: Mat4,
    pub lighting_handle: u32,
    pub visible: bool,

    pub hands_plan_n: Option<u32>,
    pub gun_plan_n: Option<u32>,

    pub gun_colormap_skip_n: Option<u32>,

    pub gun_ordinal_skip_n: Option<u32>,

    pub gun_colormap_skip_names: Option<String>,

    pub scope_plan_n: Option<u32>,

    pub scope_house_plan_n: Option<u32>,

    pub scope_lens_plan_n: Option<u32>,

    pub plan_draw_n: Option<u32>,

    pub plan_mat_hints: Option<String>,

    pub plan_skip_n: Option<u32>,

    pub drawgun: Option<i32>,

    pub scene_light_index: u8,

    pub reflection_probe_index: u8,

    pub revision: u64,
    pub generation: u64,
    pub revisions: SourceRevisions,

    pub packed_vertices: assets::RetailPackedVertexPayload,
}

impl FpvDrawPlan {
    #[must_use]
    pub fn admits_colour(&self) -> bool {
        self.visible && self.lighting_handle != 0 && self.drawgun != Some(0)
    }
}

pub struct FpvPlanSurface {
    pub mesh: Mesh,
    pub authored: Option<usize>,
    pub material: SmodelPassMaterial,
    pub packed_vertices: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,

    pub is_scope: bool,

    pub is_lens: bool,
}

pub fn topology_fingerprint(indices: &[u32], ranges: &[(u32, u32)], decoded_n: usize) -> u64 {
    let mut revisions = SourceRevisions::default();
    revisions.set_topology_from(indices, ranges, decoded_n);
    revisions.topology
}

pub fn stamp_plan_geometry(revisions: &mut SourceRevisions, revision: u64, topology: u64) -> u64 {
    if revisions.topology == topology {
        return revision;
    }
    revisions.topology = topology;
    revisions.bump_draws();
    revision.wrapping_add(1)
}

fn apply_plan<O: HasObjectId>(
    resolved: &render_scene::model_lighting::ResolvedModelLightingTable,
    draws: &mut Vec<XModelSurfaceDraw>,
    owners: &mut Vec<O>,
    revisions: &mut SourceRevisions,
) -> Vec<u16> {
    use render_scene::model_lighting::ResolvedModelLighting;
    let mut failed = Vec::new();
    let mut changed = false;
    for draw in draws.iter_mut() {
        let Some(request) = draw.pending_lighting.take() else {
            continue;
        };
        match resolved.get(request.owner) {
            Some(ResolvedModelLighting::Seated {
                handle,
                scene_light_index,
                reflection_probe_index,
                packed_lighting,
            }) => {
                changed |= (
                    draw.lighting_handle,
                    draw.scene_light_index,
                    draw.reflection_probe_index,
                    draw.packed_lighting,
                ) != (
                    handle,
                    scene_light_index,
                    reflection_probe_index,
                    packed_lighting,
                );
                draw.lighting_handle = handle;
                draw.scene_light_index = scene_light_index;
                draw.reflection_probe_index = reflection_probe_index;
                draw.packed_lighting = packed_lighting;
            }
            Some(ResolvedModelLighting::Failed) | None => failed.push(draw.object_id),
        }
    }
    if !failed.is_empty() {
        revisions.bump_admission();
        changed = true;
        draws.retain(|draw| !failed.contains(&draw.object_id));
        owners.retain(|owner| !failed.contains(&owner.object_id()));
    }
    if changed {
        revisions.bump_draws();
    }
    failed
}

fn apply_fpv_plan(
    resolved: &render_scene::model_lighting::ResolvedModelLightingTable,
    plan: &mut FpvDrawPlan,
) {
    use render_scene::model_lighting::{ModelLightingOwner, ResolvedModelLighting};
    let previous = (
        plan.lighting_handle,
        plan.scene_light_index,
        plan.reflection_probe_index,
    );
    let was_visible = plan.visible;
    let hide = |plan: &mut FpvDrawPlan| {
        plan.lighting_handle = 0;
        plan.visible = false;
        plan.scene_light_index = 0;
        plan.reflection_probe_index = 0;
    };
    match resolved.get(ModelLightingOwner::Eye) {
        Some(ResolvedModelLighting::Seated {
            handle,
            scene_light_index,
            reflection_probe_index,
            ..
        }) => {
            plan.lighting_handle = handle;
            plan.scene_light_index = scene_light_index;
            plan.reflection_probe_index = reflection_probe_index;

            if !plan.draws.is_empty() {
                plan.visible = plan.drawgun != Some(0);
            }
        }
        Some(ResolvedModelLighting::Failed) | None => hide(plan),
    }
    if was_visible != plan.visible {
        plan.revisions.bump_admission();
    }
    if was_visible != plan.visible
        || previous
            != (
                plan.lighting_handle,
                plan.scene_light_index,
                plan.reflection_probe_index,
            )
    {
        plan.revisions.bump_draws();
    }
}

impl ScriptModelDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl ItemDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl MissileDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl DynEntDrawPlan {
    pub fn finalize_lighting(
        &mut self,
        resolved: &render_scene::ResolvedModelLightingTable,
    ) -> Vec<u16> {
        apply_plan(
            resolved,
            &mut self.draws,
            &mut self.owners,
            &mut self.revisions,
        )
    }
}

impl FpvDrawPlan {
    pub fn finalize_lighting(&mut self, resolved: &render_scene::ResolvedModelLightingTable) {
        apply_fpv_plan(resolved, self);
    }
}
trait HasObjectId {
    fn object_id(&self) -> u16;
}

impl HasObjectId for ScriptModelOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

impl HasObjectId for ItemOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

impl HasObjectId for MissileOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

impl HasObjectId for DynEntOwnerDraw {
    fn object_id(&self) -> u16 {
        self.object_id
    }
}

#[cfg(test)]
mod lighting_finalization_tests {
    use super::*;
    use render_scene::{
        ModelLightingOwner, ModelLightingRequest, ResolvedModelLighting, ResolvedModelLightingTable,
    };

    fn draw(object_id: u16) -> XModelSurfaceDraw {
        XModelSurfaceDraw {
            surface: 0,
            material: 0,
            world_from_local: Mat4::IDENTITY,
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

    #[test]
    fn resolved_payload_and_noop_preserve_geometry_revisions() {
        let mut plan = MissileDrawPlan::default();
        plan.draws.push(draw(1));
        let mut table = ResolvedModelLightingTable::default();
        table.insert_if_absent(
            ModelLightingOwner::Item(1),
            ResolvedModelLighting::Seated {
                handle: 7,
                scene_light_index: 2,
                reflection_probe_index: 3,
                packed_lighting: Some([1, 2, 3, 4]),
            },
        );
        assert!(plan.finalize_lighting(&table).is_empty());
        assert_eq!(plan.draws[0].lighting_handle, 7);
        assert_eq!(
            plan.revisions,
            SourceRevisions {
                draws: 1,
                ..Default::default()
            }
        );
        let revisions = plan.revisions;
        plan.finalize_lighting(&table);
        assert_eq!(plan.revisions, revisions);
        // A newly requested but identical lighting result is also a payload no-op.
        plan.draws[0].pending_lighting = draw(1).pending_lighting;
        plan.finalize_lighting(&table);
        assert_eq!(plan.revisions, revisions);
    }

    #[test]
    fn failed_owner_removes_all_its_surfaces_and_invalidates_admission_once() {
        let mut plan = MissileDrawPlan::default();
        plan.draws = vec![draw(1), draw(1), draw(2)];
        plan.draws[2].pending_lighting = None;
        plan.owners = vec![
            MissileOwnerDraw {
                object_id: 1,
                model: String::new(),
            },
            MissileOwnerDraw {
                object_id: 2,
                model: String::new(),
            },
        ];
        plan.finalize_lighting(&ResolvedModelLightingTable::default());
        assert_eq!(plan.draws.len(), 1);
        assert_eq!(plan.owners.len(), 1);
        assert_eq!(plan.owners[0].object_id, 2);
        assert_eq!(
            plan.revisions,
            SourceRevisions {
                draws: 1,
                admission: 1,
                ..Default::default()
            }
        );
        let revisions = plan.revisions;
        plan.finalize_lighting(&ResolvedModelLightingTable::default());
        assert_eq!(plan.revisions, revisions);
    }

    #[test]
    fn fpv_visibility_changes_admission_without_invalidating_vertices() {
        let mut plan = FpvDrawPlan::default();
        plan.visible = true;
        plan.lighting_handle = 7;
        plan.finalize_lighting(&ResolvedModelLightingTable::default());
        assert!(!plan.visible);
        assert_eq!(
            plan.revisions,
            SourceRevisions {
                draws: 1,
                admission: 1,
                ..Default::default()
            }
        );
        let revisions = plan.revisions;
        plan.finalize_lighting(&ResolvedModelLightingTable::default());
        assert_eq!(plan.revisions, revisions);
    }
}
