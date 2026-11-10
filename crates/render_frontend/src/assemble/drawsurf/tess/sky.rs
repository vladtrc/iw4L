use super::smodel::{SmodelGpuPlan, SmodelPassMaterial, pack_smodel_meshes, runtime_maps};
use super::xmodel::XModelSurfaceDraw;
use crate::prepare::scene::{spawn::WorldSpawnJob, world::WorldScene};
use bevy::prelude::*;
use render_frame::SourceRevisions;

#[derive(Resource, Default)]
pub struct SkyModelDrawPlan {
    pub geometry: SmodelGpuPlan,
    pub draws: Vec<XModelSurfaceDraw>,
    /// The atlas world of each draw.
    pub worlds: Vec<usize>,
    pub generation: u64,
    pub revisions: SourceRevisions,
}

pub fn build_sky_model_draw_plan(
    mut commands: Commands,
    scene: Option<Res<WorldScene>>,
    job: Option<Res<WorldSpawnJob>>,
    existing: Option<Res<SkyModelDrawPlan>>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(scene), Some(job)) = (scene, job) else {
        return;
    };
    if scene.sky_models.is_empty() {
        return;
    }
    let (exact, _, _) = job.tess_image_handles();
    if exact.is_empty() {
        return;
    }
    let (mut geometry, _, _, _) = pack_smodel_meshes(&scene.sky_models, &[]);
    let mut draws = Vec::new();
    let mut worlds = Vec::new();
    let sky_surfaces: Vec<_> = geometry
        .meshes
        .iter()
        .zip(&scene.sky_worlds)
        .flat_map(|(mesh, &world)| mesh.surfaces_by_lod[0].iter().map(move |&s| (world, s)))
        .collect();
    for (world, (surface, authored)) in sky_surfaces {
        let Some(authored) = authored else {
            diag::warn_once!(World, "sky surface {surface}: material missing");
            continue;
        };
        let Some(derived) = scene.runtime_material_catalog.derived(authored) else {
            diag::warn_once!(World, "sky material {authored:?}: unavailable");
            continue;
        };
        let Some(ordinal) = scene
            .runtime_material_catalog
            .parts()
            .sorted_materials
            .ordinal_for_asset_id(authored.order())
        else {
            diag::warn_once!(World, "sky material {authored:?}: no sorted ordinal");
            continue;
        };
        let maps = runtime_maps(Some(authored), &scene.runtime_material_catalog, exact);
        let material = geometry.materials.len() as u32;
        geometry.materials.push(SmodelPassMaterial {
            model_lighting_required: false,
            color: maps.color,
            specular: None,
            probe: None,
            atlas: None,
            alpha_mode: maps.alpha_mode,
            draw_mode: maps.draw_mode,
            cull_mode: maps.cull_mode,
            env_map_parms: maps.env_map_parms,
            lighting_lookup_scale: [0.; 4],
            atlas_lookup: [0.; 4],
            sort_key: derived.sort_key,
            material_sorted_index: Some(ordinal.get()),
        });
        worlds.push(world);
        draws.push(XModelSurfaceDraw {
            surface,
            material,
            world_from_local: Mat4::IDENTITY,
            lighting_handle: 0,
            pending_lighting: None,
            colour_refusal: None,
            object_id: u16::MAX,
            scene_light_index: 0,
            reflection_probe_index: 0,
            packed_lighting: None,
            is_scope: false,
            scene_entnum: None,
            body_client: None,
            caster_bound: None,
        });
    }
    diag::info!(
        World,
        "skybox tess: models={:?} surfaces={} skipped={}",
        scene
            .sky_models
            .iter()
            .map(|model| &model.name)
            .collect::<Vec<_>>(),
        draws.len(),
        geometry.packed_skip_surfaces
    );
    let mut revisions = SourceRevisions::default();
    revisions.set_topology_from(
        geometry.indices(),
        geometry.surface_ranges(),
        geometry.decoded_vertices().len(),
    );
    revisions.bump_surfaces();
    revisions.bump_draws();
    commands.queue(move |world: &mut World| {
        frame::scope::insert(
            world,
            SkyModelDrawPlan {
                geometry,
                draws,
                worlds,
                generation: 1,
                revisions,
            },
            frame::MatchScope::Live,
        );
    });
}

impl SkyModelDrawPlan {
    /// The draws, each sky around the eye as its own world sees it through
    /// the doors; the sky of a world no door leads to is left out.
    pub fn draws_at<'a>(
        &'a self,
        eye: Vec3,
        view: Option<&'a render_frame::PortalView>,
    ) -> impl Iterator<Item = XModelSurfaceDraw> + 'a {
        self.draws
            .iter()
            .zip(&self.worlds)
            .filter_map(move |(draw, &world)| {
                let at = match &view {
                    Some(view) => Vec3::from_array(view.eye_in(world, eye.to_array())?),
                    None => eye,
                };
                Some(XModelSurfaceDraw {
                    world_from_local: Mat4::from_translation(at),
                    ..*draw
                })
            })
    }
}
