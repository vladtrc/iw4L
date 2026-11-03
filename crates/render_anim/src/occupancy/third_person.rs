use asset_world::ClipCollision;
use net::{AUTHORITY_MS, PresentedSnapshot};
use playerstate_iw4::{
    CG_CAMERA_PULLBACK_BOX_HALF, CG_CAMERA_PULLBACK_CLIPMASK, CG_THIRD_PERSON_RANGE_DEFAULT,
    KillCamMode, OffsetThirdPersonViewInputs, ThirdPersonViewInputs, is_third_person_view,
    offset_third_person_view,
};
use sim::{ClientId, LifeSequence, ProjectileId, RemoteMissile};

use render_scene::WorldCameraPose;

pub const CG_THIRD_PERSON_ANGLE_MP: f32 = 356.0;

pub fn presented_is_third_person(
    presented: &PresentedSnapshot,
    local: ClientId,
    in_killcam: bool,
    third_person: bool,
) -> bool {
    if presented.external_camera() {
        return true;
    }
    if presented.first_person_camera() {
        return false;
    }
    let Some(ps) = presented.player(local) else {
        return false;
    };
    if in_killcam && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE {
        // Older demos name the eventual missile throughout the preroll.
        // The camera remains first person until it can resolve that entity.
        return super::killcam::classify_view(presented, local, true, None).is_some();
    }
    if remote_missile_link(presented, local).is_some() {
        return true;
    }
    is_third_person_view(ThirdPersonViewInputs {
        pm_type: ps.pm_type,
        other_flags: ps.other_flags,
        link_flags: ps.link_flags,
        cg_third_person: third_person && !in_killcam,
        in_killcam,
        killcam_mode: KillCamMode::Mode0,
    })
}

fn remote_missile_link(
    presented: &PresentedSnapshot,
    local: ClientId,
) -> Option<(RemoteMissile, LifeSequence)> {
    let meta = presented.snapshot()?.meta.for_client(local)?;
    Some((meta.remote_missile?, meta.life_sequence))
}

// The missile leaves the snapshot on impact, before the link goes and the
// static starts. The last pose is held until then so that neither the
// player's own view nor a black frame shows in between.
#[derive(bevy::prelude::Resource, Default)]
pub struct RemoteMissileView {
    held: Option<HeldMissileView>,
    turn: MissileTurn,
}

#[derive(Clone, Copy, Debug)]
struct HeldMissileView {
    life: LifeSequence,
    projectile: ProjectileId,
    pose: WorldCameraPose,
}

const TURN_SAMPLES: usize = 4;

#[derive(Default)]
struct MissileTurn {
    missile: Option<(LifeSequence, ProjectileId)>,
    samples: Vec<(i32, [f32; 3])>,
}

impl MissileTurn {
    fn record(
        &mut self,
        missile: (LifeSequence, ProjectileId),
        snapshot_ms: i32,
        angles: [f32; 3],
    ) {
        if self.missile != Some(missile) {
            self.missile = Some(missile);
            self.samples.clear();
        }
        match self.samples.last_mut() {
            Some(last) if last.0 == snapshot_ms => last.1 = angles,
            Some(last) if last.0 > snapshot_ms => {
                self.samples.clear();
                self.samples.push((snapshot_ms, angles));
            }
            _ => {
                if self.samples.len() == TURN_SAMPLES {
                    self.samples.remove(0);
                }
                self.samples.push((snapshot_ms, angles));
            }
        }
    }

    fn angles_at(&self, at_time: i32) -> Option<[f32; 3]> {
        let shown = at_time.saturating_sub(AUTHORITY_MS);
        let &(first_ms, first) = self.samples.first()?;
        if shown <= first_ms {
            return Some(first);
        }
        let Some(pair) = self.samples.windows(2).find(|pair| shown < pair[1].0) else {
            return self.samples.last().map(|&(_, newest)| newest);
        };
        let ((from_ms, from), (to_ms, to)) = (pair[0], pair[1]);
        let fraction = (shown - from_ms) as f32 / (to_ms - from_ms) as f32;
        Some(std::array::from_fn(|i| {
            from[i] + math_iw4::angle_subtract(to[i], from[i]) * fraction
        }))
    }
}

impl RemoteMissileView {
    pub fn camera(
        &mut self,
        presented: &PresentedSnapshot,
        local: ClientId,
        render_time: i32,
        clip: Option<&ClipCollision>,
    ) -> Option<WorldCameraPose> {
        let at_time = presented.trajectory_time_ms(render_time);
        let Some((link, life)) = remote_missile_link(presented, local) else {
            self.held = None;
            self.turn = MissileTurn::default();
            return None;
        };
        self.held = self
            .held
            .filter(|held| held.life == life && held.projectile == link.projectile);
        let flying = link
            .unlink_at_ms
            .is_none()
            .then(|| {
                presented
                    .snapshot()?
                    .projectiles
                    .iter()
                    .find(|p| p.id == link.projectile)
            })
            .flatten();
        let snapshot_ms = presented.tick().map_or(at_time, sim::level_time_ms);
        if let Some(missile) = flying {
            self.turn
                .record((life, link.projectile), snapshot_ms, link.angles);
            let pose = WorldCameraPose {
                origin: missile_eye(missile, snapshot_ms, at_time, clip),
                angles: self.turn.angles_at(at_time).unwrap_or(link.angles),
            };
            self.held = Some(HeldMissileView {
                life,
                projectile: link.projectile,
                pose,
            });
        }
        self.held.map(|held| held.pose)
    }
}

fn missile_eye(
    missile: &sim::ProjectileState,
    snapshot_ms: i32,
    at_time: i32,
    clip: Option<&ClipCollision>,
) -> [f32; 3] {
    let known = missile.origin_at(at_time.min(snapshot_ms));
    if at_time <= snapshot_ms {
        return known;
    }
    let ahead = missile.origin_at(at_time.min(snapshot_ms.saturating_add(AUTHORITY_MS)));
    let fraction = clip.map_or(1.0, |clip| {
        let half = CG_CAMERA_PULLBACK_BOX_HALF;
        let hit = clip.sweep_box(
            known,
            ahead,
            [-half; 3],
            [half; 3],
            CG_CAMERA_PULLBACK_CLIPMASK,
        );
        if hit.allsolid { 0.0 } else { hit.fraction }
    });
    std::array::from_fn(|i| known[i] + (ahead[i] - known[i]) * fraction.clamp(0.0, 1.0))
}

pub fn linked_weapon_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
) -> Option<WorldCameraPose> {
    let current = presented.snapshot()?.meta.for_client(local)?;
    let view = current.linked_weapon_view?;
    let previous = presented
        .interpolation_pair()
        .and_then(|(before, _, fraction)| {
            let meta = before.meta.for_client(local)?;
            (meta.life_sequence == current.life_sequence)
                .then_some((meta.linked_weapon_view?, fraction.clamp(0.0, 1.0)))
        });
    Some(match previous {
        Some((previous, fraction)) => WorldCameraPose {
            origin: std::array::from_fn(|i| {
                previous.origin[i] + (view.origin[i] - previous.origin[i]) * fraction
            }),
            angles: std::array::from_fn(|i| {
                previous.angles[i]
                    + math_iw4::angle_subtract(view.angles[i], previous.angles[i]) * fraction
            }),
        },
        None => WorldCameraPose {
            origin: view.origin,
            angles: view.angles,
        },
    })
}

pub fn third_person_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    clip: Option<&ClipCollision>,
) -> Option<WorldCameraPose> {
    let ps = presented.player(local)?;
    let offset = presented.view_offset();
    let yaw = presented
        .snapshot()?
        .meta
        .for_client(local)
        .map(|meta| meta.look_at_killer_yaw as f32)
        .unwrap_or(ps.viewangles[1]);

    let half = CG_CAMERA_PULLBACK_BOX_HALF;
    let trace = |start: [f32; 3], end: [f32; 3]| {
        let Some(clip) = clip else { return 1.0 };
        clip.sweep_box(
            start,
            end,
            [-half, -half, -half],
            [half, half, half],
            CG_CAMERA_PULLBACK_CLIPMASK,
        )
        .fraction
    };
    let view = offset_third_person_view(
        OffsetThirdPersonViewInputs {
            origin: [
                ps.origin[0] + offset[0],
                ps.origin[1] + offset[1],
                ps.origin[2] + offset[2],
            ],
            view_height_current: ps.view_height_current,
            viewangles: ps.viewangles,
            pm_type: ps.pm_type,
            look_at_killer_yaw: yaw,

            corpse_j_mainroot: None,
            other_flags: ps.other_flags,
            delta_time: ps.delta_time,
            cg_third_person_angle: if ps.pm_type > 7 {
                CG_THIRD_PERSON_ANGLE_MP
            } else {
                0.0
            },
            cg_third_person_range: CG_THIRD_PERSON_RANGE_DEFAULT,
        },
        trace,
    );
    Some(WorldCameraPose {
        origin: view.origin,
        angles: view.angles,
    })
}
