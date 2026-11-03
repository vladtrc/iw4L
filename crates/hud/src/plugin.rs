use frame::ScopeApp;
use std::time::Instant;

use bevy::prelude::*;
use frame::{AppScreen, LaunchIdentity, LaunchReport, LifeFrontPublished, LifeStarted, UiDraw};
use net::{ClientSet, UpdatePhaseCensus};

use crate::blood::{BloodGpuJob, BloodOverlayLatch, HudRootVisible, update_blood_overlay};
use crate::compass::{CompassRaster, spawn_compass, update_compass};
use crate::flash::{FlashGpuJob, FlashWhiteoutLatch, update_flash_whiteout};
use crate::gpu_list::{self, HudTessPass};
use crate::hud_elems::{
    HudElemsBackRaster, HudElemsRaster, spawn_hud_elems, spawn_hud_elems_back, update_hud_elems,
};
use crate::images::HudImages;
use crate::iris::{IrisLetterboxFill, spawn_iris, update_iris};
use crate::killcam_skip::{KillcamSkipRaster, spawn_killcam_skip, update_killcam_skip};
use crate::killfeed::{KillfeedRaster, KillfeedWindow, spawn_killfeed, update_killfeed};
use crate::mantle_hint::{MantleHintRaster, spawn_mantle_hint, update_mantle_hint};
use crate::playercard::{
    PlayerCardCache, PlayerCardRaster, UiLocalVars, spawn_playercard, update_playercard,
};
use crate::reticle::{ReticleAdsLatch, spawn_reticle, update_reticle};
use crate::scorebar::{ScorebarRaster, spawn_scorebar, update_scorebar};
use crate::scoreboard::{ScoreboardRaster, spawn_scoreboard, update_scoreboard};
use crate::splash::{PendingSplash, SplashRaster, SplashSlots, spawn_splash, update_splash};
use crate::targetmap::{TargetmapRaster, spawn_targetmap, update_targetmap};
use crate::weaponbar::{WeaponbarRaster, spawn_weaponbar, update_weaponbar};

#[derive(Component)]
struct HudRoot;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnExit(frame::MatchScope::Live),
            release_hud_publication.in_set(frame::ScopeSet::Release),
        );
        crate::gpu_list::register(app);
        let _ = crate::scorebar::milliseconds();
        app.init_resource::<frame::HudDraw>()
            .init_resource::<HudImages>()
            .init_resource::<ReticleAdsLatch>()
            .init_resource::<IrisLetterboxFill>()
            .init_resource::<BloodOverlayLatch>()
            .scoped::<crate::damage_direction::DamageDirections>(frame::Timeline::Live)
            .init_resource::<BloodGpuJob>()
            .scoped::<FlashWhiteoutLatch>(frame::Timeline::Live)
            .scoped::<FlashGpuJob>(frame::Timeline::Live)
            .scoped::<PendingSplash>(frame::Timeline::Live)
            .scoped::<SplashSlots>(frame::Timeline::Live)
            .scoped::<KillfeedWindow>(frame::Timeline::Live)
            .scoped::<PlayerCardCache>(frame::MatchScope::Live)
            .scoped::<UiLocalVars>(frame::MatchScope::Live)
            .init_resource::<crate::playercard::FrontendLocalVars>()
            .init_resource::<HudRootVisible>()
            .scoped::<crate::compass::CompassPingLatch>(frame::Timeline::Live)
            .init_resource::<crate::surface::Hud2dSurface>()
            .init_resource::<HudPresentStamp>()
            .init_resource::<HudStageStamp>()
            .init_resource::<crate::expr_cache::MenuExprCache>()
            .scoped::<crate::hudelem::HudElemSoundLatch>(frame::Timeline::Live)
            .scoped::<crate::menus::ScriptMenus>(frame::MatchScope::Live)
            .init_resource::<crate::menus::FrontendMenus>()
            .add_message::<net::SvcCardSlotCmd>()
            .add_message::<net::SvcOpenMenuCmd>();

        frame::register_ui_contracts(app);
        crate::overhead_names::register(app);
        app.add_message::<LifeStarted>()
            .add_observer(crate::killfeed::obituary)
            .add_systems(
                Update,
                (
                    hud_stamp_open,
                    sync_games_root,
                    sync_map_zone_tree,
                    sync_zone_atlases,
                    warm_hud_images,
                    ensure_hud_root,
                    sync_frontend_camera,
                    hud_stamp_setup,
                    ApplyDeferred,
                    hud_stamp_deferred,
                    sync_hud_visibility,
                    hud_stamp_visibility,
                )
                    .chain()
                    .in_set(ClientSet::Ui),
            )
            .add_systems(
                Update,
                (
                    (
                        (
                            hud_surfaces_open,
                            crate::surface::update_hud_surface,
                            update_reticle.in_set(frame::InMatch),
                            hud_stage_close::<0>,
                            update_iris.in_set(frame::InMatch),
                            hud_stage_close::<1>,
                            update_blood_overlay.in_set(frame::InMatch),
                            hud_stage_close::<2>,
                            update_flash_whiteout
                                .in_set(frame::InMatch)
                                .after(frame::ScreenEffectsPublished),
                            hud_stage_close::<3>,
                            update_compass.in_set(frame::InMatch),
                            hud_stage_close::<4>,
                            update_playercard.in_set(frame::InMatch),
                            update_weaponbar.in_set(frame::InMatch),
                            hud_stage_close::<5>,
                        )
                            .chain(),
                        (
                            update_scorebar.in_set(frame::InMatch),
                            update_splash.in_set(frame::InMatch),
                            update_killfeed.in_set(frame::InMatch),
                            update_scoreboard,
                            update_killcam_skip,
                            crate::emp_static::update.in_set(frame::InMatch),
                            update_mantle_hint,
                            crate::breath_hint::update.in_set(frame::InMatch),
                            crate::use_hint::update.in_set(frame::InMatch),
                            update_hud_elems.in_set(frame::InMatch),
                            update_targetmap,
                            crate::damage_direction::update.in_set(frame::InMatch),
                            crate::menus::update_script_menus,
                            crate::connection_notice::update.in_set(frame::InMatch),
                            hud_stage_close::<7>,
                        )
                            .chain(),
                    )
                        .chain(),
                    (hud_stage_close::<8>, hud_surfaces_close).chain(),
                )
                    .chain()
                    .in_set(LifeFrontPublished),
            )
            .add_systems(
                Update,
                (
                    begin_hud_tess_frame,
                    flush_flash_tess.in_set(frame::InMatch),
                    flush_overhead_names_tess,
                    flush_hud_tess,
                    flush_killcam_skip_tess,
                    flush_playercard_tess,
                    flush_scoreboard_tess,
                    flush_emp_static_tess,
                    flush_mantle_hint_tess,
                    flush_breath_hint_tess,
                    flush_use_hint_tess,
                    flush_hud_elems_tess,
                    flush_targetmap_tess,
                    flush_blood_tess,
                    flush_damage_direction_tess,
                    flush_script_menus_tess,
                    flush_connection_notice_tess,
                )
                    .chain()
                    .after(hud_stage_close::<7>)
                    .before(hud_stage_close::<8>)
                    .in_set(LifeFrontPublished),
            )
            .add_systems(
                Update,
                hide_tess_when_hud_hidden
                    .after(hud_surfaces_close)
                    .in_set(LifeFrontPublished),
            );
    }
}

fn hud_root_should_show(screen: AppScreen, ui_draw: bool) -> bool {
    matches!(screen, AppScreen::InGame | AppScreen::ClassSelect) && ui_draw
}

fn hide_tess_when_hud_hidden(
    root: Res<HudRootVisible>,
    screen: Res<AppScreen>,
    mut frame: ResMut<crate::gpu_list::HudTessGpuFrame>,
    mut latches: Query<&mut crate::gpu_list::GpuListLatch>,
) {
    if root.0 == Some(1) || *screen == AppScreen::MainMenu {
        return;
    }
    frame.clear_geometry();
    frame.visible = false;

    for mut latch in &mut latches {
        latch.mark_hidden();
    }
}

fn begin_hud_tess_frame(
    surface: Res<crate::surface::Hud2dSurface>,
    mut frame: ResMut<crate::gpu_list::HudTessGpuFrame>,
) {
    let _body = gpu_list::TessBody::open();
    *frame = crate::gpu_list::HudTessGpuFrame::default();
    if !surface.is_ready() {
        return;
    }
    frame.surface_w = surface.width();
    frame.surface_h = surface.height();
    frame.visible = true;
}

fn flush_blood_tess(
    latch: Res<BloodOverlayLatch>,
    job: Res<BloodGpuJob>,
    mut frame: ResMut<crate::gpu_list::HudTessGpuFrame>,
) {
    let _body = gpu_list::TessBody::open();
    match *job {
        BloodGpuJob::Show | BloodGpuJob::Write => {
            if latch.packed.is_empty() {
                return;
            }
            frame.append_packed(&latch.packed);
        }
        BloodGpuJob::Hide | BloodGpuJob::Idle => {}
    }
}

fn flush_flash_tess(
    latch: Res<FlashWhiteoutLatch>,
    job: Res<FlashGpuJob>,
    mut frame: ResMut<crate::gpu_list::HudTessGpuFrame>,
) {
    let _body = gpu_list::TessBody::open();
    frame.saved_screen_sequence = latch.save_sequence;
    match *job {
        FlashGpuJob::Write => {
            if latch.packed.is_empty() {
                return;
            }
            frame.append_packed(&latch.saved);
            frame.append_packed(&latch.packed);
        }
        FlashGpuJob::Hide | FlashGpuJob::Idle => {}
    }
}

fn sync_games_root(
    mut hud_images: ResMut<HudImages>,
    identity: Option<Res<LaunchIdentity>>,
    publication: Option<Res<asset_material::UiImagePublication>>,
) {
    if let Some(publication) = publication {
        hud_images.adopt_ui_images(&publication);
    }
    let Some(identity) = identity else {
        return;
    };
    if identity.games_root.as_os_str().is_empty() {
        return;
    }
    hud_images.set_games_root(&identity.games_root);
}

fn sync_map_zone_tree(mut hud_images: ResMut<HudImages>, report: Option<Res<LaunchReport>>) {
    let Some(report) = report else {
        return;
    };
    let Ok(zone_ff) = report.zone_ff.as_ref() else {
        return;
    };
    hud_images.adopt_map_zone(zone_ff);
}

fn sync_zone_atlases(
    catalog: Option<Res<asset_game::MenuCatalog>>,
    mut hud_images: ResMut<HudImages>,
) {
    match catalog {
        Some(catalog) if !hud_images.zone_installed() || catalog.is_changed() => {
            hud_images.install_zone_catalog(&catalog)
        }
        None if hud_images.zone_installed() => hud_images.clear_zone_catalog(),
        _ => {}
    }
}

fn warm_hud_images(
    catalog: Option<Res<asset_game::MenuCatalog>>,
    compass: Option<Res<assets::SessionCompass>>,
    mut hud_images: ResMut<HudImages>,
    mut images: ResMut<Assets<Image>>,
) {
    hud_images.warm_present_stems(&mut images, catalog.as_deref(), compass.as_deref());
}

#[derive(Component)]
struct FrontendCamera;

fn sync_frontend_camera(
    mut commands: Commands,
    screen: Res<AppScreen>,
    scope: Res<State<frame::MatchScope>>,
    cameras: Query<Entity, With<FrontendCamera>>,
) {
    if *screen == AppScreen::MainMenu && *scope.get() == frame::MatchScope::Absent {
        if cameras.is_empty() {
            commands.spawn((
                FrontendCamera,
                Camera3d::default(),
                frame::DisplayEncodedCamera,
                bevy::core_pipeline::tonemapping::Tonemapping::None,
                bevy::render::view::DebandDither::Disabled,
                Camera {
                    order: -1,
                    ..default()
                },
                bevy::render::view::Msaa::Off,
                frame::UiCamera,
            ));
        }
    } else {
        for camera in &cameras {
            commands.entity(camera).despawn();
        }
    }
}

fn ensure_hud_root(mut commands: Commands, existing: Query<Entity, With<HudRoot>>) {
    if !existing.is_empty() {
        return;
    }
    commands
        .spawn((
            HudRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            crate::font_overlay::spawn_overlay(root, crate::overhead_names::OverheadNamesRaster);
            spawn_reticle(root);
            crate::damage_direction::spawn(root);
            spawn_iris(root);
            spawn_hud_elems_back(root);
            spawn_compass(root);
            spawn_scorebar(root);
            spawn_weaponbar(root);
            spawn_splash(root);
            spawn_killfeed(root);
            spawn_killcam_skip(root);
            spawn_playercard(root);
            spawn_scoreboard(root);
            crate::font_overlay::spawn_overlay(root, crate::emp_static::EmpStaticRaster);
            spawn_mantle_hint(root);
            crate::font_overlay::spawn_overlay(root, crate::breath_hint::BreathHintRaster);
            crate::font_overlay::spawn_overlay(root, crate::use_hint::UseHintRaster);
            spawn_hud_elems(root);
            spawn_targetmap(root);
            crate::menus::spawn_script_menus(root);
            crate::font_overlay::spawn_overlay(
                root,
                crate::connection_notice::ConnectionNoticeRaster,
            );
        });
}

fn sync_hud_visibility(
    screen: Res<AppScreen>,
    ui_draw: Option<Res<UiDraw>>,
    hud_draw: Res<frame::HudDraw>,
    mut roots: Query<&mut Visibility, With<HudRoot>>,
    mut visible: ResMut<HudRootVisible>,
) {
    let ui_on = ui_draw.is_some_and(|d| d.0);
    let show = hud_draw.0 && hud_root_should_show(*screen, ui_on);
    visible.0 = Some(i32::from(show));
    for mut vis in &mut roots {
        let want = if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }
}

#[derive(Resource, Default)]
struct HudPresentStamp(Option<Instant>);

fn hud_stamp_open(mut stamp: ResMut<HudPresentStamp>) {
    stamp.0 = Some(Instant::now());
}

fn hud_stamp_setup(mut stamp: ResMut<HudPresentStamp>, mut census: ResMut<UpdatePhaseCensus>) {
    let now = Instant::now();
    census.hud_root_setup_ms = stamp.0.map(|t| (now - t).as_secs_f32() * 1000.0);
    stamp.0 = Some(now);
}

fn hud_stamp_deferred(mut stamp: ResMut<HudPresentStamp>, mut census: ResMut<UpdatePhaseCensus>) {
    let now = Instant::now();
    census.present_apply_deferred_ms = stamp.0.map(|t| (now - t).as_secs_f32() * 1000.0);
    stamp.0 = Some(now);
}

fn hud_stamp_visibility(stamp: Res<HudPresentStamp>, mut census: ResMut<UpdatePhaseCensus>) {
    let now = Instant::now();
    census.hud_visibility_ms = stamp.0.map(|t| (now - t).as_secs_f32() * 1000.0);
}

fn hud_surfaces_open(
    mut stamp: ResMut<HudPresentStamp>,
    mut split: ResMut<HudStageStamp>,
    mut pass: ResMut<HudTessPass>,
) {
    let now = Instant::now();
    stamp.0 = Some(now);
    split.0 = Some(now);
    *pass = HudTessPass::default();
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn flush_hud_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut compass: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<CompassRaster>,
    >,
    mut scorebar: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        (With<ScorebarRaster>, Without<CompassRaster>),
    >,
    mut weaponbar: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        (
            With<WeaponbarRaster>,
            Without<CompassRaster>,
            Without<ScorebarRaster>,
        ),
    >,
    mut splash: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        (
            With<SplashRaster>,
            Without<CompassRaster>,
            Without<ScorebarRaster>,
            Without<WeaponbarRaster>,
        ),
    >,
    mut killfeed: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        (
            With<KillfeedRaster>,
            Without<CompassRaster>,
            Without<ScorebarRaster>,
            Without<WeaponbarRaster>,
            Without<SplashRaster>,
        ),
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let compass_job = std::mem::take(&mut pass.compass);
    let scorebar_job = std::mem::take(&mut pass.scorebar);
    let weaponbar_job = std::mem::take(&mut pass.weaponbar);
    let splash_job = std::mem::take(&mut pass.splash);
    let killfeed_job = std::mem::take(&mut pass.killfeed);
    if let Ok((_, mut host, mut latch)) = compass.single_mut() {
        writer.apply(compass_job, &mut host, &mut latch);
    }
    if let Ok((_, mut host, mut latch)) = scorebar.single_mut() {
        writer.apply(scorebar_job, &mut host, &mut latch);
    }
    if let Ok((_, mut host, mut latch)) = weaponbar.single_mut() {
        writer.apply(weaponbar_job, &mut host, &mut latch);
    }
    if let Ok((_, mut host, mut latch)) = splash.single_mut() {
        writer.apply(splash_job, &mut host, &mut latch);
    }
    if let Ok((_, mut host, mut latch)) = killfeed.single_mut() {
        writer.apply(killfeed_job, &mut host, &mut latch);
    }
}

fn flush_killcam_skip_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut skip: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<KillcamSkipRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.killcam_skip);
    if let Ok((_, mut host, mut latch)) = skip.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_targetmap_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut raster: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<TargetmapRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.targetmap);
    if let Ok((_, mut host, mut latch)) = raster.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_script_menus_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut raster: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::menus::ScriptMenuRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.script_menus);
    if let Ok((_, mut host, mut latch)) = raster.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_playercard_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut card: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<PlayerCardRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.playercard);
    if let Ok((_, mut host, mut latch)) = card.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_mantle_hint_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut hint: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<MantleHintRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.mantle_hint);
    if let Ok((_, mut host, mut latch)) = hint.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_damage_direction_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut hint: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::damage_direction::DamageDirectionRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.damage_direction);
    if let Ok((_, mut host, mut latch)) = hint.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_scoreboard_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut scoreboard: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<ScoreboardRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.scoreboard);
    if let Ok((_, mut host, mut latch)) = scoreboard.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn flush_hud_elems_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut front: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        (With<HudElemsRaster>, Without<HudElemsBackRaster>),
    >,
    mut back: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<HudElemsBackRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let front_job = std::mem::take(&mut pass.hud_elems);
    let back_job = std::mem::take(&mut pass.hud_elems_back);
    for (job, raster) in [
        (front_job, front.single_mut()),
        (back_job, back.single_mut()),
    ] {
        if let Ok((_, mut host, mut latch)) = raster {
            writer.apply(job, &mut host, &mut latch);
        }
    }
}

#[derive(Resource, Default)]
struct HudStageStamp(Option<Instant>);

fn hud_stage_close<const N: usize>(
    mut split: ResMut<HudStageStamp>,
    mut census: ResMut<UpdatePhaseCensus>,
) {
    let now = Instant::now();
    if let Some(t) = split.0 {
        census.hud_stage_ms[N] = Some((now - t).as_secs_f32() * 1000.0);
    }
    split.0 = Some(now);
}

fn hud_surfaces_close(stamp: Res<HudPresentStamp>, mut census: ResMut<UpdatePhaseCensus>) {
    let now = Instant::now();
    census.hud_surfaces_ms = stamp.0.map(|t| (now - t).as_secs_f32() * 1000.0);
    // Two different quantities, published side by side on purpose: the line
    // above is the gap between two systems, and the tess bodies below ran
    // inside part of it. Whatever else the executor put in that gap is the
    // difference, and naming it is the point.
    let (body_ms, jobs) = gpu_list::take_tess_body_cost();
    census.hud_tess_body_ms = Some(body_ms);
    census.hud_tess_jobs = Some(jobs);
}

fn flush_use_hint_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut hint: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::use_hint::UseHintRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.use_hint);
    if let Ok((_, mut host, mut latch)) = hint.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

pub(crate) fn flush_overhead_names_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut hint: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::overhead_names::OverheadNamesRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.overhead_names);
    if let Ok((_, mut host, mut latch)) = hint.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_emp_static_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut raster: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::emp_static::EmpStaticRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.emp_static);
    if let Ok((_, mut host, mut latch)) = raster.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_breath_hint_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut hint: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::breath_hint::BreathHintRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.breath_hint);
    if let Ok((_, mut host, mut latch)) = hint.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn flush_connection_notice_tess(
    mut writer: gpu_list::HudTessWriter,
    mut pass: ResMut<HudTessPass>,
    mut hint: Query<
        (Entity, &mut Node, &mut crate::gpu_list::GpuListLatch),
        With<crate::connection_notice::ConnectionNoticeRaster>,
    >,
) {
    let _body = gpu_list::TessBody::open();
    if !writer.is_ready() {
        return;
    }
    let job = std::mem::take(&mut pass.connection_notice);
    if let Ok((_, mut host, mut latch)) = hint.single_mut() {
        writer.apply(job, &mut host, &mut latch);
    }
}

fn release_hud_publication(
    mut latches: Query<&mut crate::gpu_list::GpuListLatch>,
    mut frame: ResMut<crate::gpu_list::HudTessGpuFrame>,
    mut blood: ResMut<BloodGpuJob>,
    flash: Option<ResMut<FlashGpuJob>>,
) {
    for mut latch in &mut latches {
        latch.mark_hidden();
    }
    *frame = Default::default();
    *blood = BloodGpuJob::Hide;
    if let Some(mut flash) = flash {
        *flash = FlashGpuJob::Hide;
    }
}
