use glam::Vec3;

use crate::frame::FrameWorld;
use crate::{ClientId, ClientLifecycle};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeaponLock {
    pub weapon: u32,
    pub life: u32,
    pub flags: u8,
    pub target: [f32; 3],
    pub point_sum: [f32; 3],
    pub point_count: u8,
    pub misses: u8,
    pub sampled_at: i32,
    pub out_of_ads_at: i32,
    pub acquire_started_at: i32,
}

impl WeaponLock {
    pub fn locking(self) -> bool {
        self.flags & 3 == 1
    }

    pub fn locked(self) -> bool {
        self.flags & 2 != 0
    }

    pub fn too_close(self) -> bool {
        self.flags & 16 != 0
    }
}

pub(crate) fn update(world: &mut FrameWorld, id: ClientId, now: i32) {
    let Some(ps) = world.player(id).copied() else {
        return;
    };
    let Some(meta) = world.client_meta(id) else {
        return;
    };
    let life = meta.life_sequence.0;
    let mut lock = meta.weapon_lock;
    let reset = WeaponLock {
        weapon: ps.weapon,
        life,
        sampled_at: now,
        out_of_ads_at: now,
        ..Default::default()
    };
    if lock.weapon != ps.weapon || lock.life != life || now < lock.sampled_at {
        lock = reset;
    }
    if !world.weapon_script_name(ps.weapon).contains("javelin")
        || meta.lifecycle != ClientLifecycle::Alive
        || ps.other_flags & (1 << 10) != 0
        || ps.f_weapon_pos_frac < 0.95
    {
        world.client_meta_mut(id).weapon_lock = reset;
        return;
    }
    if now.saturating_sub(lock.sampled_at) < 50 {
        return;
    }
    lock.sampled_at = now;
    let origin = Vec3::from_array(ps.origin);
    let eye = origin + Vec3::Z * ps.view_height_current;
    let (forward, right, up) = math_iw4::angle_vectors(ps.viewangles);
    let forward = Vec3::from_array(forward);
    if lock.flags & 3 != 0 {
        let delta = Vec3::from_array(lock.target) - eye;
        let depth = delta.dot(forward);
        let scale = 320.0 / (65.0_f32.to_radians() * 0.5).tan();
        let x = delta.dot(Vec3::from_array(right)) * scale / depth;
        let y = delta.dot(Vec3::from_array(up)) * scale / depth;
        if depth <= 0.0 || x * x + y * y >= 45.0 * 45.0 {
            lock = WeaponLock {
                out_of_ads_at: lock.out_of_ads_at,
                ..reset
            };
        } else {
            lock.flags = (lock.flags & !16)
                | if Vec3::from_array(lock.target).distance(origin) < 1100.0 {
                    16
                } else {
                    0
                };
            if now.saturating_sub(lock.acquire_started_at) >= 1150 {
                lock.flags |= 2 | 4;
            }
        }
    } else if lock.misses >= 4 {
        lock = WeaponLock {
            out_of_ads_at: lock.out_of_ads_at,
            ..reset
        };
    } else if now.saturating_sub(lock.out_of_ads_at) >= 100 {
        let end = eye + forward * 15000.0;
        let hit = world.trace_world(
            eye.to_array(),
            end.to_array(),
            [0.0; 3],
            [0.0; 3],
            crate::bullet_collision::MASK_BULLET_WORLD,
        );
        if hit.fraction >= 1.0
            || hit.startsolid != 0
            || trace_iw4::surface_type_from_flags(hit.surface_flags) == 0
        {
            lock.misses += 1;
        } else if Vec3::from_array(hit.endpos).distance(origin) < 1100.0 {
            lock.flags |= 16;
        } else {
            lock.flags &= !16;
            let point = Vec3::from_array(hit.endpos);
            let sum = Vec3::from_array(lock.point_sum);
            if lock.point_count != 0 && (sum / f32::from(lock.point_count)).distance(point) > 400.0
            {
                lock.misses += 1;
            } else {
                lock.point_sum = (sum + point).to_array();
                lock.point_count += 1;
                lock.misses = 0;
                if lock.point_count >= 12 {
                    lock.target = ((sum + point) / f32::from(lock.point_count)).to_array();
                    lock.flags = 1 | 64;
                    lock.acquire_started_at = now;
                }
            }
        }
    }
    world.client_meta_mut(id).weapon_lock = lock;
}
