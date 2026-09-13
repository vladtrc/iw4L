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
    BodyMeshCatalog, FpvMeshCatalog, WeaponRegistry, WorldWeaponCatalog, XAnimCatalog, ZoneGame,
    ZoneImage,
    lane_capability::{LaneStatus, PreparedCapability},
    progress::LoadProgress,
    session_load::{PreparedWorld, WorldDrawPolicy},
};
use asset_anim::AnimLoadCapture;
use asset_audio::AudioLoadCapture;
use asset_game::GameLoadCapture;
use asset_model::ModelLoadCapture;
use asset_transport::MapTransportCapture;
use asset_world::WorldLoadCapture;

#[derive(Clone, Debug)]
pub struct LaneGap {
    pub capability: PreparedCapability,
    pub reason: String,
    pub addr: Option<&'static str>,
}

#[derive(Default)]
pub struct LoadedWorld {
    pub world: WorldLoadCapture,
    pub models: ModelLoadCapture,
    pub anim: AnimLoadCapture,
    pub audio: AudioLoadCapture,
    pub game: GameLoadCapture,
    pub transport: MapTransportCapture,
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
            world: WorldLoadCapture {
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_prepared_parts(
        prepared: PreparedWorld,
        collision: Option<crate::ClipCollision>,
        spawns: Vec<crate::SpawnPoint>,
        bodies: BodyMeshCatalog,
        fpv_meshes: FpvMeshCatalog,
        xanims: XAnimCatalog,
        report: Vec<String>,
        gaps: Vec<LaneGap>,
        s1_map_arenas: Option<crate::ZoneMemory>,
    ) -> Self {
        let PreparedWorld {
            draw,
            static_model_meshes,
            static_model_instances,
            map_xmodel_scene_assets,
            script_model_instances,
            script_brush_models,
            map_use_triggers,
            flag_descriptors,
            dyn_ents,
            smodel_lighting_samples,
            light_grid,
            fx,
            fx_models,
            fx_glass,
            impact_fx,
            reflection_probe_images,
            intermission_view,
            minimap_corners,
            north_yaw,
            compass,
            script_sound,
            t5_teamset,
            team_icons,
            exp_fog,
            film_vision,
            createart_name,
            min,
            max,
            world_bounds,
            policy,
        } = prepared;
        Self {
            world: WorldLoadCapture {
                draw,
                collision,
                spawns,
                static_model_meshes,
                static_model_instances,
                map_xmodel_scene_assets,
                script_model_instances,
                script_brush_models,
                map_use_triggers,
                flag_descriptors,
                dyn_ents,
                smodel_lighting_samples,
                light_grid,
                fx_glass,
                reflection_probe_images,
                intermission_view,
                minimap_corners,
                north_yaw,
                compass,
                exp_fog,
                film_vision,
                createart_name,
                min,
                max,
                world_bounds,
                policy,
            },
            models: ModelLoadCapture { bodies, fpv_meshes },
            anim: AnimLoadCapture { xanims },
            audio: AudioLoadCapture { script_sound },
            game: GameLoadCapture {
                fx,
                fx_models,
                impact_fx,
                t5_teamset,
                team_icons,
            },
            transport: MapTransportCapture { s1_map_arenas },
            report,
            gaps,
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
    pub weapons: WeaponRegistry,

    pub cac_tables: Vec<crate::CapturedStringTable>,
    pub fpv: FpvMeshCatalog,
    pub world_weapons: WorldWeaponCatalog,

    pub projectile_meshes: crate::ProjectileMeshCatalog,
    pub xanims: XAnimCatalog,
    pub player_anim_sources: crate::PlayerAnimSources,
    pub fx: crate::FxCatalog,
    pub fx_models: crate::FxModelCatalog,

    pub tracers: crate::TracerCatalog,
    pub impact_fx: Option<crate::OwnedFxImpactTable>,

    pub material_population: crate::MaterialCatalog,

    pub technique_sets: Vec<crate::TechniqueSetFacts>,

    pub light_defs: Vec<crate::CapturedLightDef>,
    pub report: Vec<String>,

    pub pen_table: weapon_iw4::PenetrationDepthTable,
    pub pen_table_loaded: bool,

    pub xmodel_walk: crate::PreparedXModelWalkCensus,

    pub s1_common_arenas: Option<crate::ZoneMemory>,

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
        common_techsets: &[crate::TechniqueSetFacts],

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
