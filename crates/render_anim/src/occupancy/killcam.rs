use std::sync::Arc;

use asset_world::{ClipCollision, ClipSweepHit};
use assets::PreparedWeapons;
use bevy::prelude::*;
use entity_iw4::{EntityState, Trajectory, evaluate_trajectory};
use net::PresentedSnapshot;
use playerstate_iw4::{ENTITYNUM_NONE, KillCamMode};
use render_scene::WorldCameraPose;
use sim::ClientId;

#[derive(Resource, Default)]
pub struct KillcamCamera {
    entity: Option<i32>,
    generation: Option<u32>,
    tracked_projectile: Option<(sim::ProjectileId, sim::LifeSequence)>,
    mode: Option<KillCamMode>,
    origin: Vec3,
    angles: [f32; 3],
    target: Vec3,
    rest_ground: bool,
    previous_origin: Option<(i32, Vec3)>,
    stop: Option<(i32, f32, Vec3, Vec3)>,
    entered_at: i32,
    last_time: i32,
    last_pose: Option<WorldCameraPose>,
    blend_until: i32,
    portal_focus: Option<(i32, usize, Option<Vec3>)>,
    pose_log_at: i32,
    embedded_eye: Option<[f32; 3]>,
}

fn entity_origin(es: &EntityState, now: i32) -> Vec3 {
    Vec3::from_array(evaluate_trajectory(
        &Trajectory {
            tr_time: es.tr_time,
            tr_type: es.tr_type,
            tr_duration: es.tr_duration,
            tr_base: es.tr_base,
            tr_delta: es.tr_delta,
        },
        now,
    ))
}

fn look_angles(direction: Vec3) -> [f32; 3] {
    let forward = direction.normalize_or_zero();
    let left = Vec3::new(-forward.y, forward.x, 0.0)
        .try_normalize()
        .unwrap_or(Vec3::X);
    math_iw4::axis_to_angles([
        forward.to_array(),
        left.to_array(),
        forward.cross(left).to_array(),
    ])
}

/// The projectile_flight atlas the match runs on (a merged map): the clip of each
/// world but the match's own (world 0, `None`), by world, and the doors
/// between them. The killcam camera follows its entity through the doors.
#[derive(Resource, Default)]
pub struct AtlasClips {
    pub clips: Vec<Option<Arc<ClipCollision>>>,
    pub doors: Vec<render_frame::PortalPair>,
    pub flight: Option<(
        Arc<sim::world_graph::Atlas>,
        Arc<sim::world_graph::AtlasWorld>,
        sim::projectile_flight::ProjectileFlightRules,
    )>,
}

pub struct KillcamWorld<'a> {
    own_clip: Option<&'a ClipCollision>,
    atlas: Option<&'a AtlasClips>,
}

impl<'a> KillcamWorld<'a> {
    pub fn new(own_clip: Option<&'a ClipCollision>, atlas: Option<&'a AtlasClips>) -> Self {
        Self { own_clip, atlas }
    }

    fn portals(&self, origin: Vec3) -> Option<render_frame::PortalView> {
        let atlas = self.atlas;
        atlas.filter(|atlas| !atlas.doors.is_empty()).map(|atlas| {
            render_frame::PortalView::new(
                render_frame::PortalAtlas {
                    pairs: &atlas.doors,
                    world_count: atlas.clips.len(),
                    surfaces: Default::default(),
                    space: None,
                    camera_world: None,
                },
                origin.to_array(),
            )
        })
    }

    fn clip<'b>(
        &'b self,
        portals: Option<&'b render_frame::PortalView>,
    ) -> Option<KillcamClip<'b>> {
        let atlas = self.atlas;
        let own_clip = self.own_clip;
        match (atlas, portals) {
            (Some(atlas), Some(view)) => KillcamClip::of_world(atlas, own_clip, view.camera_world)
                .map(|clip| KillcamClip {
                    doors: Some((view, atlas, own_clip)),
                    ..clip
                }),
            _ => own_clip.map(KillcamClip::own),
        }
    }

    fn next_portal(
        &self,
        snapshot: &sim::Snapshot,
        projectile: Option<&sim::ProjectileState>,
        world: usize,
    ) -> Option<Vec3> {
        let atlas = self.atlas;
        atlas
            .and_then(|a| a.flight.as_ref())
            .and_then(|(atlas, base, rules)| {
                let p = projectile?;
                let clip = sim::world_graph::AtlasClip {
                    brushes: &base.brushes,
                    bsp: &base.bsp,
                    mesh: &base.mesh,
                    cmodels: &base.cmodels,
                };
                let mut body = sim::projectile_flight::ProjectileFlightBody {
                    origin: p.origin,
                    pos: p.pos,
                    travel: 0.0,
                };
                let mut prev = sim::level_time_ms(snapshot.tick);
                for _ in 0..800 {
                    let time = prev + 50;
                    for event in sim::projectile_flight::projectile_step(
                        sim::projectile_flight::ProjectileFlightScene {
                            atlas,
                            base: clip,
                            players: &[],
                            blocked: &snapshot.meta.world_objects.portal_blocked_exits,
                        },
                        None,
                        rules,
                        &mut body,
                        prev,
                        time,
                    ) {
                        if let sim::projectile_flight::ProjectileFlightEvent::Portal {
                            at, ..
                        } = event
                        {
                            return Some(Vec3::from_array(at));
                        }
                    }
                    if sim::world_graph::world_at(body.origin) != world {
                        break;
                    }
                    prev = time;
                }
                None
            })
    }
}

/// The clip the killcam camera traces in: the one of the world it is in,
/// at that world's slab of the shared coordinates. A sweep that passes a
/// door of that world goes on in the clip of the world beyond it.
#[derive(Clone, Copy)]
pub struct KillcamClip<'a> {
    clip: &'a ClipCollision,
    offset: [f32; 3],
    doors: Option<(
        &'a render_frame::PortalView,
        &'a AtlasClips,
        Option<&'a ClipCollision>,
    )>,
}

impl<'a> KillcamClip<'a> {
    pub fn own(clip: &'a ClipCollision) -> Self {
        Self {
            clip,
            offset: [0.0; 3],
            doors: None,
        }
    }

    fn of_world(
        atlas: &'a AtlasClips,
        own: Option<&'a ClipCollision>,
        world: usize,
    ) -> Option<Self> {
        match world {
            0 => own.map(Self::own),
            world => atlas.clips.get(world)?.as_deref().map(|clip| Self {
                clip,
                offset: sim::world_graph::world_offset(world),
                doors: None,
            }),
        }
    }

    fn sweep_here(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> ClipSweepHit {
        let o = self.offset;
        let local = |p: [f32; 3]| [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
        let mut hit = self
            .clip
            .sweep_box(local(start), local(end), mins, maxs, mask);
        hit.endpos = [
            hit.endpos[0] + o[0],
            hit.endpos[1] + o[1],
            hit.endpos[2] + o[2],
        ];
        hit
    }

    fn sweep_box(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> ClipSweepHit {
        let here = self.sweep_here(start, end, mins, maxs, mask);
        let Some((view, atlas, own)) = self.doors else {
            return here;
        };
        let Some((t, world)) = view.first_opening(start, end) else {
            return here;
        };
        if here.startsolid || here.fraction < t {
            return here;
        }
        let door = Vec3::from_array(start)
            .lerp(Vec3::from_array(end), t)
            .to_array();
        let (Some(beyond), Some(from), Some(to)) = (
            Self::of_world(atlas, own, view.worlds.get(world).copied().unwrap_or(world)),
            view.eye_in(world, door),
            view.eye_in(world, end),
        ) else {
            return here;
        };
        let far = beyond.sweep_here(from, to, mins, maxs, mask);
        if far.startsolid {
            return ClipSweepHit {
                fraction: t,
                endpos: door,
                ..here
            };
        }
        match (
            view.seen_in(world, far.endpos),
            view.seen_direction(world, far.normal),
        ) {
            (Some(endpos), Some(normal)) => ClipSweepHit {
                fraction: t + (1.0 - t) * far.fraction,
                endpos,
                normal,
                ..far
            },
            _ => here,
        }
    }
}

fn pull_back(clip: Option<KillcamClip<'_>>, mut start: Vec3, mut end: Vec3) -> Vec3 {
    let Some(clip) = clip else {
        return end;
    };
    let trace =
        |a: Vec3, b: Vec3| clip.sweep_box(a.to_array(), b.to_array(), [-5.0; 3], [5.0; 3], 0x811);
    let mut hit = trace(start, end);
    for _ in 0..3 {
        if !hit.startsolid {
            break;
        }
        start += if hit.fraction == 1.0 {
            Vec3::Z * 20.0
        } else {
            Vec3::from_array(hit.normal) * 20.0
        };
        hit = trace(start, end);
    }
    if hit.startsolid {
        return start;
    }
    for _ in 0..3 {
        if hit.fraction >= 1.0 {
            return end;
        }
        start = start.lerp(end, hit.fraction);
        let normal = Vec3::from_array(hit.normal);
        let remaining = end - start;
        end = start + remaining + normal * (0.1 - remaining.dot(normal));
        hit = trace(start, end);
    }
    start.lerp(end, hit.fraction)
}

fn look_at_both(target: Vec3, entity: Vec3, camera: Vec3) -> Vec3 {
    let first = (target - camera).normalize_or_zero();
    let second = (entity - camera).normalize_or_zero();
    let mut direction = (first + second).normalize_or_zero();
    let dot = direction.dot(first);
    if dot < 0.9848077893257141 {
        direction = if dot < 0.8660253882408142 {
            first
        } else {
            let axis = first.cross(second).normalize_or_zero();
            let rotated = Quat::from_axis_angle(axis, 10.0_f32.to_radians()) * first;
            rotated.lerp(
                first,
                (dot - 0.9848077893257141) / (0.8660253882408142 - 0.9848077893257141),
            )
        };
    }
    if direction.z < -0.949999988079071 {
        direction = if direction.z < -0.9800000190734863 {
            first
        } else {
            first.lerp(
                direction,
                (direction.z + 0.9800000190734863) / 0.030000030994415283,
            )
        };
    }
    direction.normalize_or_zero()
}

pub(super) fn classify_view(
    presented: &PresentedSnapshot,
    viewer: ClientId,
    in_killcam: bool,
    weapons: Option<&PreparedWeapons>,
) -> Option<KillCamMode> {
    if !in_killcam {
        return Some(KillCamMode::Mode0);
    }
    let ps = presented.player(viewer)?;
    if ps.kill_cam_entity == ENTITYNUM_NONE {
        return Some(KillCamMode::Mode0);
    }
    let snapshot = presented.snapshot()?;
    let number = ps.kill_cam_entity;
    let entity = snapshot.meta.entities.iter().find(|e| e.number == number);
    let projectile = snapshot.projectiles.iter().find(|p| p.entnum == number);
    let projectile_weapon = projectile.map(|p| p.weapon).or_else(|| {
        entity
            .filter(|e| e.e_type == entity_iw4::ET_MISSILE)
            .and_then(|e| u32::try_from(e.index).ok())
    });
    let mode = if let Some(projectile_weapon) = projectile_weapon {
        let facts = weapons.and_then(|w| {
            w.snapshot_weapon(projectile_weapon)
                .ok()
                .and_then(|weapon| weapon.event_facts())
        });
        match facts
            .map(|f| f.projectile_camera())
            .unwrap_or(asset_game::ProjectileCameraPolicy::Missile)
        {
            asset_game::ProjectileCameraPolicy::TopAttack => KillCamMode::Mode7Javelin,
            asset_game::ProjectileCameraPolicy::Remote => KillCamMode::Mode8Remote,
            asset_game::ProjectileCameraPolicy::Rocket => KillCamMode::Mode5Rocket,
            asset_game::ProjectileCameraPolicy::MissileAlternate => KillCamMode::Mode4MissileAlt,
            asset_game::ProjectileCameraPolicy::Missile => KillCamMode::Mode3Missile,
        }
    } else {
        if let Some(mode) = snapshot
            .meta
            .script_movers
            .iter()
            .find(|mover| mover.state.number == number)
            .and_then(|mover| mover.killcam_camera)
        {
            return Some(mode);
        }
        match entity?.e_type {
            12 => KillCamMode::Mode1Heli,
            11 => KillCamMode::Mode6Turret,
            _ => KillCamMode::Mode2Airstrike,
        }
    };
    Some(mode)
}

impl KillcamCamera {
    pub fn update(
        &mut self,
        presentation: &net::LocalPresentation,
        now: i32,
        in_killcam: bool,
        weapons: Option<&PreparedWeapons>,
        scene: KillcamWorld<'_>,
        generation: Option<u32>,
    ) -> Option<(WorldCameraPose, f32, Option<f32>)> {
        let presented = &*presentation.presented;
        let viewer = presentation.client_id();
        let ps = presented.player(viewer)?;
        if !in_killcam {
            *self = Self::default();
        }
        let previous_time = self.last_time;
        self.last_time = now;
        if !in_killcam {
            return None;
        }
        let snapshot = presented.snapshot()?;
        let trajectory_time = presented.trajectory_time_ms(now);
        let number = ps.kill_cam_entity;
        if number == ENTITYNUM_NONE {
            self.entity = None;
            self.embedded_eye = None;
            self.generation = None;
            self.mode = None;
            self.stop = None;
            self.previous_origin = None;
            self.last_pose = Some(WorldCameraPose {
                origin: [
                    ps.origin[0],
                    ps.origin[1],
                    ps.origin[2] + ps.view_height_current,
                ],
                angles: ps.viewangles,
            });
            return None;
        }
        let entity = snapshot.meta.entities.iter().find(|e| e.number == number);
        let projectile = snapshot.projectiles.iter().find(|p| p.entnum == number);
        if self.entity != Some(number) {
            let mode = classify_view(presented, viewer, in_killcam, weapons)?;
            self.rest_ground = false;
            self.entity = Some(number);
            self.generation = generation;
            self.tracked_projectile = projectile.map(|p| (p.id, p.owner_life));
            self.mode = Some(mode);
            self.stop = None;
            self.portal_focus = None;
            self.embedded_eye = None;
            self.previous_origin = None;
            self.entered_at = now;
            self.blend_until = playerstate_iw4::killcam_lerp_deadline_ms(
                self.last_pose.is_none(),
                true,
                mode,
                now,
            );
        }
        let same_projectile = projectile.is_some_and(|p| {
            let identity = (p.id, p.owner_life);
            let tracked = self.tracked_projectile.get_or_insert(identity);
            *tracked == identity
        });
        let same_generation = same_projectile
            || self
                .generation
                .zip(generation)
                .is_some_and(|(old, current)| old == current);
        let entity = entity.filter(|_| same_generation);
        let projectile = projectile.filter(|_| same_generation);
        if let Some(p) = projectile {
            self.origin = Vec3::from_array(presented.projectile_origin_at(p, now));
            self.angles = evaluate_trajectory(&p.apos, trajectory_time);
        } else if let Some(es) = entity {
            self.origin = entity_origin(es, trajectory_time);
            self.angles = evaluate_trajectory(
                &Trajectory {
                    tr_time: es.apos_tr_time,
                    tr_type: es.apos_tr_type,
                    tr_duration: es.apos_tr_duration,
                    tr_base: es.apos_tr_base,
                    tr_delta: es.apos_tr_delta,
                },
                trajectory_time,
            );
        }
        if let Some(p) = projectile {
            self.rest_ground = entity.is_some_and(|e| e.ground_entity_num != ENTITYNUM_NONE)
                && weapons
                    .and_then(|w| {
                        w.snapshot_weapon(p.weapon)
                            .ok()
                            .and_then(|weapon| weapon.event_facts())
                    })
                    .is_some_and(|f| f.rests_on_ground());
        }
        let look_at = if ps.kill_cam_look_at_entity == ENTITYNUM_NONE {
            viewer.0 as i32
        } else {
            ps.kill_cam_look_at_entity
        };
        if let Some(es) = snapshot.meta.entities.iter().find(|e| e.number == look_at) {
            self.target = entity_origin(es, trajectory_time) + Vec3::Z * 60.0;
        } else if look_at != viewer.0 as i32
            && let Some(target) = presented.player(ClientId(look_at as u32))
        {
            self.target = Vec3::from_array(target.origin) + Vec3::Z * target.view_height_current;
        }
        // In an atlas the entity may be in another world than the target:
        // the camera works in the entity's world, where the target is seen
        // through the doors, and traces in that world's clip.
        let portals = scene.portals(self.origin);
        let clip = scene.clip(portals.as_ref());
        if projectile
            .is_some_and(|p| matches!(p.attached_to, Some(sim::MissileTarget::PlayerBone { .. })))
            && let Some(previous) = self.last_pose
        {
            // The blade now belongs to the victim's animated bone. Keep the
            // last flight eye outside the body and watch the impact instead
            // of riding the embedded blade through the death animation.
            let eye = *self.embedded_eye.get_or_insert(previous.origin);
            let direction = self.origin - Vec3::from_array(eye);
            let pose = WorldCameraPose {
                origin: eye,
                angles: look_angles(direction),
            };
            return Some((pose, 50.0, Some(direction.length())));
        }
        let world = sim::world_graph::world_at(self.origin.to_array());
        if projectile.is_some()
            && self
                .portal_focus
                .as_ref()
                .is_none_or(|(id, w, _)| *id != number || *w != world)
        {
            let next = scene.next_portal(snapshot, projectile, world);
            diag::info!(
                World,
                "killcam focus: entity={number} world={world} look_at={look_at} victim={:?} next_portal={next:?}",
                self.target
            );
            self.portal_focus = Some((number, world, next));
        }
        let portal_target = self.portal_focus.as_ref().and_then(|(_, _, point)| *point);
        let target = portal_target
            .or_else(|| {
                portals
                    .as_ref()
                    .and_then(|view| view.seen(self.target.to_array()))
                    .map(Vec3::from_array)
            })
            .unwrap_or(self.target);
        if portal_target.is_some() {
            self.stop = None;
        }
        let mode = self.mode?;
        let near_target = (self.origin - target).length() < 350.0
            || (mode == KillCamMode::Mode8Remote && self.origin.z - target.z < 350.0);
        if projectile.is_some() && !near_target {
            self.stop = None;
        }
        let mut anchor = self.origin;
        if let Some((start, duration, origin, velocity)) = self.stop {
            let elapsed = (now - start).max(0) as f32;
            let t = elapsed.min(duration);
            anchor = origin
                + velocity
                    * if duration > 0.0 {
                        t * (1.0 - 0.5 * t / duration)
                    } else {
                        0.0
                    };
        }
        let (forward, right, up) = math_iw4::angle_vectors(self.angles);
        let forward = Vec3::from_array(forward);
        let up = Vec3::from_array(up);
        let mut toward = target - anchor;
        toward.z = toward.z.min(0.0);
        toward = toward.normalize_or_zero();
        let side = Vec3::new(-toward.y, toward.x, 0.0)
            .try_normalize()
            .unwrap_or(Vec3::X);
        let camera_up = toward.cross(side);
        let (desired, angles, fov) = match mode {
            KillCamMode::Mode0 => return None,
            KillCamMode::Mode1Heli => {
                let anchor = anchor + Vec3::Z * 50.0;
                let target = target - Vec3::Z * 100.0;
                let direction = (target - anchor).normalize_or_zero();
                let origin = pull_back(clip, anchor, anchor - direction * 1000.0);
                (origin, look_angles(target - origin), 15.0)
            }
            KillCamMode::Mode2Airstrike => {
                let origin = anchor.with_z(anchor.z.max(target.z + 24.0));
                (origin, look_angles(target - origin), 50.0)
            }
            KillCamMode::Mode6Turret => (anchor - forward * 10.0 + up * 10.0, self.angles, 50.0),
            KillCamMode::Mode7Javelin => {
                let age = projectile
                    .map(|p| (trajectory_time - p.launch_time).max(0))
                    .unwrap_or(now - self.entered_at) as f32
                    / 1000.0;
                let origin = if projectile.is_some_and(|p| p.pos.tr_type == entity_iw4::TR_GRAVITY)
                {
                    anchor
                } else if forward.z > 0.0 {
                    let pass = (age * 2.0 * std::f32::consts::PI / 5.0)
                        .min(4.71238899230957)
                        .sin();
                    anchor - forward * (pass * 200.0) + up * (50.0 * (1.0 - pass.abs()))
                } else {
                    anchor + Vec3::Z * (175.0 + 25.0 * forward.z)
                };
                let blend = (1.0 - (anchor.z - target.z) / 3000.0).clamp(0.0, 1.0);
                let gravity = projectile.is_some_and(|p| p.pos.tr_type == entity_iw4::TR_GRAVITY);
                let axis_forward = if gravity {
                    forward
                } else {
                    (anchor - origin).normalize_or_zero()
                };
                let left = -Vec3::from_array(right);
                let axis = Mat3::from_cols(axis_forward, left, axis_forward.cross(left));
                let mut rotation = Quat::from_mat3(&axis);
                if !gravity && forward.z <= 0.0 {
                    let target_forward = (target - origin).normalize_or_zero();
                    let target_left = Vec3::new(-target_forward.y, target_forward.x, 0.0)
                        .try_normalize()
                        .unwrap_or(Vec3::X);
                    let target_axis = Mat3::from_cols(
                        target_forward,
                        target_left,
                        target_forward.cross(target_left),
                    );
                    rotation = rotation.slerp(Quat::from_mat3(&target_axis), blend);
                }
                let angles = if projectile.is_some_and(|p| p.pos.tr_type == entity_iw4::TR_GRAVITY)
                {
                    self.angles
                } else {
                    math_iw4::axis_to_angles(Mat3::from_quat(rotation).to_cols_array_2d())
                };
                let origin = if !gravity && forward.z <= 0.0 {
                    origin.with_z(origin.z.max(target.z + 128.0))
                } else {
                    origin
                };
                (origin, angles, 60.0)
            }
            _ => {
                let (height, back) = if self.rest_ground {
                    if matches!(
                        mode,
                        KillCamMode::Mode3Missile | KillCamMode::Mode4MissileAlt
                    ) {
                        (15.0, 30.0)
                    } else {
                        (0.0, 0.0)
                    }
                } else {
                    match mode {
                        KillCamMode::Mode5Rocket => (10.0, 70.0),
                        KillCamMode::Mode8Remote => (60.0, 300.0),
                        _ => (5.0, 35.0),
                    }
                };
                let origin = pull_back(
                    clip,
                    self.origin,
                    anchor + camera_up * height - toward * back,
                );
                let direction = look_at_both(target, self.origin, origin);
                (origin, look_angles(direction), 50.0)
            }
        };
        if portal_target.is_none()
            && self.stop.is_none()
            && matches!(
                mode,
                KillCamMode::Mode4MissileAlt | KillCamMode::Mode5Rocket | KillCamMode::Mode8Remote
            )
            && near_target
            && let Some((time, previous)) = self.previous_origin.filter(|(time, _)| *time < now)
        {
            let velocity = (self.origin - previous) / (now - time) as f32;
            let speed = velocity.length();
            let fraction = clip
                .map(|c| {
                    c.sweep_box(
                        self.origin.to_array(),
                        (self.origin + velocity.normalize_or_zero() * 100.0).to_array(),
                        [0.0; 3],
                        [0.0; 3],
                        0x811,
                    )
                    .fraction
                })
                .unwrap_or(1.0);
            let duration = if speed > 0.0 {
                200.0 * fraction / speed
            } else {
                0.0
            };
            self.stop = Some((now, duration, self.origin, velocity));
        }
        let mut pose = WorldCameraPose {
            origin: desired.to_array(),
            angles,
        };
        if now < self.blend_until
            && let Some(from) = self.last_pose
        {
            let dt = (now - previous_time).max(0) as f32;
            let t = (dt / (self.blend_until - previous_time).max(1) as f32).clamp(0.0, 1.0);
            let velocity = self
                .previous_origin
                .filter(|(time, _)| *time < now && (projectile.is_some() || entity.is_some()))
                .map(|(time, origin)| (self.origin - origin) / (now - time) as f32)
                .unwrap_or(Vec3::ZERO);
            let mut projected = desired + velocity * (self.blend_until - now) as f32;
            if let Some(clip) = clip {
                let hit = clip.sweep_box(
                    desired.to_array(),
                    projected.to_array(),
                    [-5.0; 3],
                    [5.0; 3],
                    0x811,
                );
                projected = if hit.startsolid {
                    desired
                } else {
                    desired.lerp(projected, hit.fraction)
                };
            }
            pose.origin = Vec3::from_array(from.origin).lerp(projected, t).to_array();
            for (i, angle) in pose.angles.iter_mut().enumerate() {
                *angle = from.angles[i] + math_iw4::angle_subtract(*angle, from.angles[i]) * t;
            }
        }
        self.previous_origin = Some((now, self.origin));
        self.last_pose = Some(pose);
        // Behind the entity across a door, the camera is in the world on the
        // other side of it.
        if let Some(view) = &portals {
            let world = view.world_along(self.origin.to_array(), pose.origin);
            if let Some((origin, turn)) = view
                .pose_in(world, pose.origin)
                .filter(|_| world != view.camera_world)
            {
                pose.origin = origin;
                pose.angles[1] += turn;
            }
        }
        if now - self.pose_log_at >= 500 {
            self.pose_log_at = now;
            diag::info!(
                World,
                "killcam pose: now={now} trajectory={trajectory_time} entity={:?} eye={:?} angles={:?}",
                self.origin,
                pose.origin,
                pose.angles
            );
        }
        let focus_distance = matches!(
            mode,
            KillCamMode::Mode1Heli
                | KillCamMode::Mode2Airstrike
                | KillCamMode::Mode3Missile
                | KillCamMode::Mode4MissileAlt
                | KillCamMode::Mode5Rocket
                | KillCamMode::Mode8Remote
        )
        .then_some((target - desired).length());
        Some((pose, fov, focus_distance))
    }
}
