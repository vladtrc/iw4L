use bevy::prelude::*;

/// What the ADS autofocus sweep is allowed to focus on: the same solid-world
/// contents the sight trace uses. Every `sweep_box` caller names its mask.
const AUTOFOCUS_CLIPMASK: u32 = 0x0080_6c31;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DepthOfField {
    pub view_model_start: f32,
    pub view_model_end: f32,
    pub near_start: f32,
    pub near_end: f32,
    pub far_start: f32,
    pub far_end: f32,
    pub near_blur: f32,
    pub far_blur: f32,
}
impl DepthOfField {
    pub fn active(self) -> bool {
        self.view_model_end > self.view_model_start + 1.0
            || self.near_end > self.near_start + 1.0
            || (self.far_end > self.far_start + 1.0 && self.far_blur > 0.0)
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct DofDvars {
    pub enable: bool,
    pub tweak: bool,
    pub values: DepthOfField,
    pub bias: f32,
}
impl Default for DofDvars {
    fn default() -> Self {
        Self {
            enable: true,
            tweak: false,
            bias: 0.5,
            values: DepthOfField {
                view_model_start: 2.0,
                view_model_end: 8.0,
                near_start: 10.0,
                near_end: 60.0,
                far_start: 1000.0,
                far_end: 7000.0,
                near_blur: 6.0,
                far_blur: 1.8,
            },
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct GlowDvars {
    pub enable: bool,
    pub use_tweaks: bool,
    pub tweak_enable: bool,
    pub tweak_radius: f32,
    pub tweak_intensity: f32,
    pub tweak_cutoff: f32,
    pub tweak_desaturation: f32,

    pub allowed: bool,

    pub allowed_script_forced: bool,
}
impl Default for GlowDvars {
    fn default() -> Self {
        let tweaks = lighting_iw4::GlowViewInfo::tweak_register_defaults();
        Self {
            enable: lighting_iw4::R_GLOW_DEFAULT,
            use_tweaks: lighting_iw4::R_GLOW_USE_TWEAKS_DEFAULT,
            tweak_enable: tweaks.enable,
            tweak_radius: tweaks.radius,
            tweak_intensity: tweaks.intensity,
            tweak_cutoff: tweaks.cutoff,
            tweak_desaturation: tweaks.desaturation,
            allowed: hud_iw4::R_GLOW_ALLOWED_DEFAULT,
            allowed_script_forced: hud_iw4::R_GLOW_ALLOWED_SCRIPT_FORCED_DEFAULT,
        }
    }
}

impl GlowDvars {
    #[must_use]
    pub fn tweak_view_info(self) -> lighting_iw4::GlowViewInfo {
        lighting_iw4::GlowViewInfo {
            enable: self.tweak_enable,
            cutoff: self.tweak_cutoff,
            desaturation: self.tweak_desaturation,
            intensity: self.tweak_intensity,
            radius: self.tweak_radius,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct DofFrame {
    pub dof: DepthOfField,
    pub bias: f32,
    pub scene_near: f32,
    pub view_model_near: f32,
}

pub fn register(app: &mut App) {
    app.init_resource::<DofDvars>()
        .init_resource::<GlowDvars>()
        .init_resource::<DofFrame>()
        .add_systems(
            Update,
            update_dof
                .after(crate::prepare::scene::view_parms::stamp_prepared_scene_view)
                .in_set(net::ClientSet::Present),
        );
}

fn approach(current: f32, target: f32, max_change: f32, dt: f32) -> f32 {
    let half = (current - target).abs() * 0.5;
    let scaled = max_change / 0.05 * dt;
    let step = if half > scaled { scaled } else { half.max(1.0) };
    if current > target {
        (current - step).max(target)
    } else {
        (current + step).min(target)
    }
}

fn update_dof(
    view: Res<crate::prepare::scene::view_parms::PreparedSceneView>,
    znear: Res<crate::prepare::scene::view_parms::RZnearDvar>,
    dvars: Res<DofDvars>,
    presented: Res<net::PresentedSnapshot>,
    local: Res<net::LocalPresentClient>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    clock: Res<net::CgFrameClock>,
    mut frame: ResMut<DofFrame>,
    mut scene: Local<DepthOfField>,
    clip: Res<crate::adapters::anim::dyn_ent::DynEntPhysClip>,
    world: Res<frame::WorldGeneration>,
    subject: Res<frame::ViewSubject>,
    camera: Option<Res<render_anim::occupancy::view_kick::SessionViewKick>>,
    mut generation: Local<Option<u64>>,
) {
    *frame = DofFrame::default();
    if *generation != world.0 {
        *generation = world.0;
        *scene = DepthOfField::default();
    }
    let Some(ps) = presented.player(local.0).filter(|_| view.ready) else {
        *scene = DepthOfField::default();
        return;
    };
    let ads = ps.f_weapon_pos_frac;

    let mut target = DepthOfField {
        near_start: 1.0,
        near_end: 256.0,
        far_start: 2500.0,
        far_end: 10000.0,
        near_blur: 6.0,
        ..Default::default()
    };
    if ads != 0.0 || ps.pm_type >= 8 {
        if let Some(clip) = clip.0.as_ref() {
            let hit = clip
                .sweep_box(
                    view.eye.to_array(),
                    (view.eye + view.forward * 8192.0).to_array(),
                    [0.0; 3],
                    [0.0; 3],
                    AUTOFOCUS_CLIPMASK,
                )
                .fraction
                * 8192.0;
            if hit < target.near_end {
                target.near_end = (hit - 30.0).max(1.0);
            }
            target.far_start = target.far_start.max(hit);
            target.far_end = target.far_start * 4.0;
        }
    }
    if ads < 1.0 && ps.pm_type < 8 {
        scene.near_start = ads;
        scene.near_end = ads * target.near_end;
        scene.far_start = (1.0 - ads) * 5000.0 + ads * target.far_start;
        scene.far_end = (1.0 - ads) * 5000.0 + ads * target.far_end;
        scene.near_blur = 6.0;
        scene.far_blur = 0.0;
    } else {
        let dt = clock.frametime_secs();
        scene.near_start = approach(scene.near_start, target.near_start, 50.0, dt);
        scene.near_end = approach(scene.near_end, target.near_end, 50.0, dt);
        scene.far_start = approach(scene.far_start, target.far_start, 400.0, dt);
        scene.far_end = approach(scene.far_end, target.far_end, 400.0, dt);
        scene.near_blur = approach(scene.near_blur, 6.0, 0.1, dt);
        scene.far_blur = approach(scene.far_blur, 0.0, 0.1, dt);
    }

    scene.view_model_start = 0.0;
    scene.view_model_end = 0.0;
    if !crate::adapters::anim::third_person::presented_is_third_person(
        &presented,
        local.0,
        subject.in_killcam(),
    ) {
        if let Some(range) = weapons
            .as_ref()
            .and_then(|w| w.0.facts_of(weapon_iw4::bg_get_viewmodel_weapon_index(ps)))
            .and_then(|w| w.ads_dof)
        {
            scene.view_model_start = range[0] * ads;
            scene.view_model_end = range[1] * ads;
        }
    }
    if let Some(distance) = camera.as_ref().and_then(|c| c.killcam_focus_distance) {
        *scene = DepthOfField {
            near_start: 0.0,
            near_end: 100.0,
            far_start: distance + 100.0,
            far_end: distance + 400.0,
            near_blur: 4.0,
            far_blur: 2.0,
            ..Default::default()
        };
    }
    *frame = DofFrame {
        dof: if dvars.tweak {
            dvars.values
        } else if dvars.enable {
            *scene
        } else {
            DepthOfField::default()
        },
        bias: dvars.bias,
        scene_near: znear.value,
        view_model_near: view.depth_hack_near,
    };
}
