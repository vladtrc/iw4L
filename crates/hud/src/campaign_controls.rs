use bevy::prelude::*;
use frame::ScopeApp;
use playerstate_iw4::{eflags, pm_flags as movement_flags};

use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate};
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::{HUD_CHROME_NAMESPACE, HudImages};
use crate::surface::Hud2dSurface;

#[derive(Component)]
pub(crate) struct CampaignControlsRaster;

#[derive(Resource, Default)]
struct StanceDisplay {
    client: Option<u32>,
    stance: Option<u32>,
    time: i32,
    shown: i32,
    changed: i32,
}

impl StanceDisplay {
    fn advance(&mut self, client: u32, now: i32, pm_flags: u32, e_flags: u32) {
        if self.client != Some(client) || now < self.time {
            *self = Self {
                client: Some(client),
                ..default()
            };
        }
        let stance = pm_flags & (movement_flags::PRONE | movement_flags::CROUCH);
        if self.stance.map_or(stance != 0, |old| old != stance) {
            self.shown = now;
            self.changed = now;
        }
        if (e_flags & eflags::PRONE != 0 && e_flags & eflags::TURRET_ACTIVE_PRONE != 0)
            || (e_flags & eflags::DUCK != 0 && e_flags & eflags::TURRET_ACTIVE_DUCK != 0)
        {
            self.shown = now;
        }
        self.stance = Some(stance);
        self.time = now;
    }

    fn alpha(&self, duration: f32) -> f32 {
        if duration == 0. {
            1.
        } else {
            hud_iw4::fade_color(
                self.time,
                self.shown,
                (duration * 1000.).round() as i32,
                700,
            )
            .unwrap_or(0.)
        }
    }

    fn material(&self) -> &'static str {
        match self.stance.unwrap_or(0) {
            1 | 3 => "stance_prone",
            2 => "stance_crouch",
            _ => "stance_stand",
        }
    }
}

pub(crate) fn register(app: &mut App) {
    app.scoped::<StanceDisplay>(frame::Timeline::Live)
        .add_systems(
            Update,
            update
                .in_set(frame::InMatch)
                .in_set(frame::LifeFrontPublished)
                .after(crate::surface::update_hud_surface)
                .before(crate::plugin::flush_campaign_controls_tess),
        );
}

fn picture(
    list: &mut Draw2dList,
    surface: &Hud2dSurface,
    item: &asset_game::MenuItem,
    material: &str,
    alpha: f32,
    override_color: Option<[f32; 4]>,
) {
    let rect = &item.rect;
    let placed = surface.apply_rect(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        rect.horz_align.into(),
        rect.vert_align.into(),
    );
    let mut color = override_color.unwrap_or(item.fore_color);
    color[3] *= alpha;
    list.cmds.push(Draw2dCmd {
        material_namespace: HUD_CHROME_NAMESPACE,
        material: asset_core::AssetRef::bare_name(material).to_owned(),
        x: placed.x,
        y: placed.y,
        w: placed.w,
        h: placed.h,
        s0: 0.,
        t0: 0.,
        s1: 1.,
        t1: 1.,
        color,
        op: Draw2dOp::StretchPic,
        provenance: if item.owner_draw == 20 {
            Draw2dProvenance::OwnerDraw(20)
        } else {
            Draw2dProvenance::MenuItem {
                menu: "facemaskoverlay".into(),
                index: 0,
            }
        },
        layer: 1,
    });
}

#[derive(Clone, Copy)]
struct Settings {
    stance: bool,
    mask: bool,
    fade: f32,
    flash_color: [f32; 4],
}

impl Settings {
    fn read(dvars: sim::ScriptDvars<'_>) -> Self {
        let flash_color = dvars
            .string("cg_hudstanceflash")
            .and_then(|v| {
                let values: Option<Vec<f32>> =
                    v.split_whitespace().map(|v| v.parse().ok()).collect();
                <[f32; 4]>::try_from(values?).ok()
            })
            .filter(|v| v.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
            .unwrap_or([1.; 4]);
        Self {
            stance: dvars.int("hud_showstance").unwrap_or(1) != 0,
            mask: dvars.int("hud_gasmaskoverlay").is_some_and(|v| v != 0),
            fade: dvars
                .float("hud_fade_stance")
                .filter(|v| v.is_finite())
                .unwrap_or(1.7)
                .clamp(0., 30.),
            flash_color,
        }
    }
}

fn paint(
    catalog: &asset_game::MenuCatalog,
    surface: &Hud2dSurface,
    stance: &StanceDisplay,
    settings: Settings,
    stance_allowed: bool,
) -> (Draw2dList, usize) {
    let mut list = Draw2dList::default();
    let mut layout_gaps = 0;
    let alpha = stance.alpha(settings.fade);
    if settings.stance && stance_allowed && alpha > 0. {
        if let Some(item) = catalog
            .get("stance")
            .and_then(|m| m.items.iter().find(|i| i.owner_draw == 20))
        {
            picture(&mut list, surface, item, stance.material(), alpha, None);
            if stance.changed != 0 && stance.time < stance.changed.saturating_add(1000) {
                let flash =
                    (stance.changed.saturating_add(1000) - stance.time) as f32 / 1000. * 0.8;
                picture(
                    &mut list,
                    surface,
                    item,
                    "stance_flash",
                    flash,
                    Some(settings.flash_color),
                );
            }
        } else {
            layout_gaps += 1;
            diag::warn_once!(Ui, "campaign stance layout unavailable");
        }
    }
    if settings.mask {
        if let Some(item) = catalog
            .get("facemaskoverlay")
            .and_then(|m| m.items.iter().find(|i| !i.background.is_empty()))
        {
            picture(&mut list, surface, item, &item.background, 1., None);
        } else {
            layout_gaps += 1;
            diag::warn_once!(Ui, "campaign face mask layout unavailable");
        }
    }
    (list, layout_gaps)
}

#[derive(bevy::ecs::system::SystemParam)]
struct Display<'w> {
    catalog: Option<Res<'w, asset_game::CampaignHudCatalog>>,
    surface: Res<'w, Hud2dSurface>,
    clock: Res<'w, net::FrameClock>,
    stance: ResMut<'w, StanceDisplay>,
    pass: ResMut<'w, HudTessPass>,
    hud_images: ResMut<'w, HudImages>,
    images: ResMut<'w, Assets<Image>>,
    input: Option<Res<'w, frame::HudInputView>>,
    weapons: Option<Res<'w, assets::PreparedWeapons>>,
}

fn update(mut display: Display, presentation: net::LocalPresentation) {
    display.pass.campaign_controls = TessJob::Hide;
    let Some(snapshot) = presentation
        .snapshot()
        .filter(|s| s.meta.objectives.campaign_mode)
    else {
        return;
    };
    let Some(ps) = presentation.player() else {
        return;
    };
    let now = display.clock.time();
    display
        .stance
        .advance(presentation.client_id().0, now, ps.pm_flags, ps.e_flags);
    let dvars = snapshot.meta.script_dvars(presentation.client_id());
    if !display.surface.is_ready() || dvars.int("cg_draw2d") == Some(0) {
        return;
    }
    let Some(catalog) = display.catalog.as_ref().map(|c| c.0.as_ref()) else {
        return;
    };
    let settings = Settings::read(dvars);
    let alpha = display.stance.alpha(settings.fade);
    let scoped = ps.f_weapon_pos_frac > 0.
        && display.weapons.as_ref().is_some_and(|w| {
            w.registry()
                .overlay_is_hud_iris(weapon_iw4::get_viewmodel_weapon_index(ps))
        });
    let shock = presentation
        .shellshock()
        .map_or(hud_iw4::SCREEN_BLEND_BLURRED, |s| s.screen_type);
    let paused =
        dvars.int("cg_paused").is_some_and(|v| v != 0) && dvars.int("cg_drawpaused") != Some(1);
    let stance_allowed = crate::gameplay_hud_visible(&presentation)
        && ps.health > 0
        && !paused
        && !scoped
        && !display.input.as_ref().is_some_and(|i| i.script_menu_open)
        && hud_iw4::is_flashbanged(now, ps.shellshock_time, ps.shellshock_duration, shock) == 0
        && snapshot
            .meta
            .for_client(presentation.client_id())
            .is_none_or(|m| m.remote_missile.is_none())
        && dvars.int("hud_missionfailed") != Some(1)
        && dvars.int("ammocounterhide") != Some(1)
        && dvars.int("actionslotshide") != Some(1)
        && (ps.e_flags & (0x100000 | eflags::SP_LINKED_STANCE)) != 0x100000;
    let (mut list, layout_gaps) = paint(
        catalog,
        &display.surface,
        &display.stance,
        settings,
        stance_allowed,
    );
    let requested = list.cmds.len();
    let mut missing = 0;
    let Display {
        hud_images, images, ..
    } = &mut display;
    list.cmds.retain(|cmd| {
        let available = hud_images
            .get(cmd.material_namespace, &cmd.material, images)
            .is_some();
        if !available {
            missing += 1;
            diag::warn_once!(
                Ui,
                "campaign HUD image unavailable: {material}",
                material = cmd.material.clone()
            );
        }
        available
    });
    perf::instant!("campaign_hud_controls", "iw4l.render", |ctx| {
        ctx.arg("show_stance", i64::from(settings.stance && stance_allowed));
        ctx.arg("mask", i64::from(settings.mask));
        ctx.arg("requested", requested as i64);
        ctx.arg("missing_images", missing as i64);
        ctx.arg("layout_gaps", layout_gaps as i64);
        ctx.arg("stance_alpha", f64::from(alpha));
    });
    let (quads, _) = tessellate(&list);
    display.pass.campaign_controls = TessJob::Quads(quads);
}
