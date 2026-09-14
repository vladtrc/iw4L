mod helpers;
mod iw4;
mod iw5;
mod sink;
mod t5;

use std::path::Path;

pub(crate) use helpers::{
    build_iw5_static_model_draw, build_static_model_draw, build_t5_static_model_draw,
};
pub(crate) use sink::{CommonWalkSink, MaterialPopulationSink, ZoneWalkSink};

use crate::{
    BodyMeshBuild, FpvMeshBuild, WeaponBuild, WorldWeaponBuild, XAnimBuild, ZoneGame, ZoneImage,
    lane_capability::{LaneStatus, PreparedCapability},
    progress::LoadProgress,
    session_load::{PreparedWorld, WorldDrawPolicy},
};

#[derive(Clone, Debug)]
pub struct LaneGap {
    pub capability: PreparedCapability,
    pub reason: String,
    pub addr: Option<&'static str>,
}

#[derive(Default)]
pub struct LoadedWorld {
    pub world: PreparedWorld,
    /// What the map zone itself captured. The local material indices in
    /// `world` are indices into this pool until the match finalizes one.
    pub materials: crate::MaterialCatalog,
    pub collision: Option<crate::ClipCollision>,
    pub spawns: Vec<crate::SpawnPoint>,
    pub bodies: BodyMeshBuild,
    pub fpv_meshes: FpvMeshBuild,
    pub xanims: XAnimBuild,
    pub facts: crate::MapFacts,
    /// Bytes the zone arenas held while the walk read them. The arenas
    /// themselves die with the walk; only their size travels.
    pub arena_bytes: usize,
    pub report: Vec<String>,
    pub gaps: Vec<LaneGap>,
}

impl LoadedWorld {
    pub fn with_gap(
        policy: WorldDrawPolicy,
        capability: PreparedCapability,
        reason: impl Into<String>,
        addr: Option<&'static str>,
    ) -> Self {
        let reason = reason.into();
        Self {
            world: PreparedWorld {
                policy,
                ..Default::default()
            },
            report: vec![reason.clone()],
            gaps: vec![LaneGap {
                capability,
                reason,
                addr,
            }],
            ..Default::default()
        }
    }

    pub fn push_gap(
        &mut self,
        capability: PreparedCapability,
        reason: impl Into<String>,
        addr: Option<&'static str>,
    ) {
        self.gaps.push(LaneGap {
            capability,
            reason: reason.into(),
            addr,
        });
    }
}

#[derive(Default)]
pub struct CommonCensus {
    pub scene_models: crate::MapXModelSceneCatalog,
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    pub weapons: WeaponBuild,

    pub cac_tables: Vec<crate::CapturedStringTable>,
    pub fpv: FpvMeshBuild,
    pub world_weapons: WorldWeaponBuild,

    pub projectile_meshes: crate::ProjectileMeshBuild,
    pub xanims: XAnimBuild,
    pub player_anim_sources: crate::PlayerAnimSources,
    pub fx: crate::FxCatalog,
    pub fx_models: crate::FxModelCatalog,

    pub tracers: crate::TracerCatalog,
    pub impact_fx: Option<crate::OwnedFxImpactTable>,

    pub material_population: crate::MaterialCatalog,

    pub light_defs: Vec<crate::CapturedLightDef>,
    pub report: Vec<String>,

    pub pen_table: weapon_iw4::PenetrationDepthTable,
    pub pen_table_loaded: bool,
    pub lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,

    pub xmodel_walk: crate::PreparedXModelWalkCensus,

    /// Bytes the common_mp arenas held while the walk read them; the arenas
    /// themselves do not outlive it.
    pub s1_common_bytes: usize,

    pub teamset_icons: std::collections::HashMap<String, crate::TeamIcons>,
    pub film_visions:
        std::collections::BTreeMap<String, Result<crate::FilmVision, crate::FilmVisionParseError>>,

    pub pending_images: Option<crate::material_images::ImageDemandPlan>,
}

pub struct MaterialPopulation {
    pub materials: crate::MaterialCatalog,
    pub walked: usize,
    pub report: Vec<String>,
}

impl Default for MaterialPopulation {
    fn default() -> Self {
        Self {
            materials: crate::MaterialCatalog::default(),
            walked: 0,
            report: Vec::new(),
        }
    }
}

pub trait ZoneLane: Send + Sync {
    fn game(&self) -> ZoneGame;
    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)];

    fn load_world(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,

        shared_surfaces: asset_model::SharedXModelSurfaces,

        material_seed: crate::MaterialCatalog,
        common_film_visions: &mut std::collections::BTreeMap<
            String,
            Result<crate::FilmVision, crate::FilmVisionParseError>,
        >,
    ) -> LoadedWorld;

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        decode_color_maps: bool,
        material_seed: crate::MaterialCatalog,
    ) -> CommonCensus;

    fn load_material_population(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        material_seed: crate::MaterialCatalog,
    ) -> MaterialPopulation;
}

pub fn lane(game: ZoneGame) -> &'static dyn ZoneLane {
    match game {
        ZoneGame::Iw4 => &iw4::Iw4Lane,
        ZoneGame::T5 => &t5::T5Lane,
        ZoneGame::Iw5 => &iw5::Iw5Lane,
    }
}

pub const LANE_GAPS: &[&str] = &[
    "assets::lane::iw4::load_world/zone_header",
    "assets::lane::iw4::load_world/zone_arenas",
    "assets::lane::iw4::load_world/no_gfx_world",
    "assets::lane::iw4::load_world/world_mesh",
    "assets::lane::t5::load_world/zone_header",
    "assets::lane::t5::load_world/zone_arenas",
    "assets::lane::t5::load_world/no_gfx_world",
    "assets::lane::t5::load_world/world_mesh",
    "assets::lane::iw5::load_world/zone_header",
    "assets::lane::iw5::load_world/zone_arenas",
    "assets::lane::iw5::load_world/no_gfx_world",
    "assets::lane::iw5::load_world/world_mesh",
    "assets::session_load::load_prepared_match/zone_open",
];
