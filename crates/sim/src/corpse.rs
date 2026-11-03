use crate::frame::FrameWorld;
use crate::world::ClientId;
use entity_iw4::{
    TR_GRAVITY, TR_INTERPOLATE, Trajectory, evaluate_trajectory, evaluate_trajectory_delta,
};
use playerstate_iw4::{
    AnimPair, ENTITYNUM_NONE, LINK_FLAGS_WEAPON_VIEW_ONLY, MAX_CLIENT_CORPSES,
    PLAYER_CORPSE_ENTITY_BASE, PlayerState,
};

use crate::bullet_collision::{MASK_PLAYER_SOLID, PLAYER_MAXS, PLAYER_MINS};

pub const G_CLONE_PLAYER_MAX_VELOCITY: f32 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCorpseSlot {
    pub occupied: bool,

    pub weapon_model: u8,
    pub client_state_team: i32,
    pub ffa_team: Option<u8>,

    pub entnum: i32,
    pub victim: ClientId,
    pub origin: [f32; 3],
    pub viewangles: [f32; 3],

    pub anim: AnimPair,
    pub view_height_current: f32,
    pub weapon: u32,
    pub e_flags: u32,

    pub tr_type: i32,

    pub tr_time: i32,
    pub tr_duration: i32,
    pub tr_delta: [f32; 3],
    pub tr_base: [f32; 3],

    pub falling: bool,

    pub ground_entity_num: i32,

    pub spawn_time_ms: i32,

    pub shield: Option<crate::ShieldAttachment>,
}

impl Default for PlayerCorpseSlot {
    fn default() -> Self {
        Self {
            occupied: false,

            weapon_model: 0,
            client_state_team: 0,
            ffa_team: None,
            entnum: -1,
            victim: ClientId(0),
            origin: [0.0; 3],
            viewangles: [0.0; 3],
            anim: AnimPair::default(),
            view_height_current: 0.0,
            weapon: 0,
            e_flags: 0,
            tr_type: 0,
            tr_time: 0,
            tr_duration: 0,
            tr_delta: [0.0; 3],
            tr_base: [0.0; 3],
            falling: false,
            ground_entity_num: ENTITYNUM_NONE,
            spawn_time_ms: 0,
            shield: None,
        }
    }
}

impl PlayerCorpseSlot {
    pub fn trajectory(&self) -> Trajectory {
        Trajectory {
            tr_time: self.tr_time,
            tr_type: self.tr_type,
            tr_duration: self.tr_duration,
            tr_delta: self.tr_delta,
            tr_base: self.tr_base,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerCorpsePool {
    pub slots: [PlayerCorpseSlot; MAX_CLIENT_CORPSES as usize],

    pub spawn_ring: u8,
}

impl Default for PlayerCorpsePool {
    fn default() -> Self {
        Self {
            slots: [PlayerCorpseSlot::default(); MAX_CLIENT_CORPSES as usize],
            spawn_ring: 0,
        }
    }
}

impl PlayerCorpsePool {
    pub fn alloc_clone_entity(&mut self) -> i32 {
        let entnum = PLAYER_CORPSE_ENTITY_BASE + i32::from(self.spawn_ring);
        self.spawn_ring = (self.spawn_ring + 1) % MAX_CLIENT_CORPSES as u8;
        self.clear_entnum(entnum);
        entnum
    }

    pub fn clear_entnum(&mut self, entnum: i32) {
        for slot in &mut self.slots {
            if slot.entnum == entnum {
                *slot = PlayerCorpseSlot::default();
            }
        }
    }

    pub fn get_free(&mut self, ref_origin: [f32; 3]) -> u8 {
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.entnum == -1 {
                return i as u8;
            }
        }
        let mut best = 0u8;
        let mut best_dist = -1.0f32;
        for (i, slot) in self.slots.iter().enumerate() {
            let dx = ref_origin[0] - slot.origin[0];
            let dy = ref_origin[1] - slot.origin[1];
            let dz = ref_origin[2] - slot.origin[2];
            let dist = dx * dx + dy * dy + dz * dz;
            if dist > best_dist {
                best_dist = dist;
                best = i as u8;
            }
        }
        let entnum = self.slots[best as usize].entnum;
        self.clear_entnum(entnum);
        best
    }

    pub fn entnums(&self) -> [i32; MAX_CLIENT_CORPSES as usize] {
        let mut nums = [-1; MAX_CLIENT_CORPSES as usize];
        for (i, slot) in self.slots.iter().enumerate() {
            nums[i] = slot.entnum;
        }
        nums
    }

    pub fn occupy(&mut self, slot: u8, entnum: i32, mut body: PlayerCorpseSlot) -> u8 {
        let i = (slot as usize) % MAX_CLIENT_CORPSES as usize;
        body.occupied = true;
        body.entnum = entnum;
        self.slots[i] = body;
        i as u8
    }
}

pub fn level_time_ms(tick: crate::world::Tick) -> i32 {
    i32::try_from(tick.0)
        .unwrap_or(0)
        .saturating_mul(crate::MATCH_TICK_MS as i32)
}

pub(crate) fn occupy_player_clone(
    world: &mut FrameWorld,
    victim: ClientId,
    ps: &PlayerState,
    time_ms: i32,
) -> u8 {
    let shield = world
        .client_meta(victim)
        .and_then(|meta| meta.shield)
        .filter(|shield| shield.on_back);
    let (client_state_team, ffa_team) = world
        .client_meta(victim)
        .map_or((0, None), |meta| (meta.client_state_team, meta.ffa_team));
    let weapon_model = weapon_iw4::weapon_model_for_held(&ps.weapons, &ps.weapon_data, ps.weapon);
    let entnum = world.corpses_mut().alloc_clone_entity();
    world.entity_kernel_mut().occupy_reserved(entnum);
    let pool = world.corpses_mut();
    let slot = pool.get_free(ps.origin);
    let slot = pool.occupy(
        slot,
        entnum,
        PlayerCorpseSlot {
            victim,
            weapon_model,
            client_state_team,
            ffa_team,
            origin: ps.origin,

            viewangles: [0.0, client_think_entity_yaw(ps), 0.0],
            anim: ps.anim(),
            view_height_current: ps.view_height_current,
            weapon: ps.weapon,
            e_flags: ps.e_flags,
            tr_type: TR_GRAVITY,
            tr_time: time_ms,
            tr_duration: 0,
            tr_delta: clamp_clone_tr_delta(ps.velocity),
            tr_base: ps.origin,
            falling: true,
            spawn_time_ms: time_ms,
            shield,
            ..PlayerCorpseSlot::default()
        },
    );
    world.corpse_dobj_tree_install(entnum, ps.anim().legs_anim);
    slot
}

fn client_think_entity_yaw(ps: &PlayerState) -> f32 {
    if (ps.link_flags & LINK_FLAGS_WEAPON_VIEW_ONLY) != 0 {
        ps.link_weapon_angles[1]
    } else {
        ps.viewangles[1]
    }
}

fn corpse_land_angles(yaw: f32, normal: [f32; 3]) -> [f32; 3] {
    let (fwd, _, _) = math_iw4::angle_vectors([0.0, yaw, 0.0]);
    let axis0 = [fwd[0], fwd[1], fwd[2]];
    let axis2 = normal;
    let axis1 = [
        axis2[1] * axis0[2] - axis2[2] * axis0[1],
        axis2[2] * axis0[0] - axis2[0] * axis0[2],
        axis2[0] * axis0[1] - axis2[1] * axis0[0],
    ];
    let axis0 = [
        axis1[1] * axis2[2] - axis1[2] * axis2[1],
        axis1[2] * axis2[0] - axis1[0] * axis2[2],
        axis1[0] * axis2[1] - axis1[1] * axis2[0],
    ];
    math_iw4::axis_to_angles([axis0, axis1, axis2])
}

fn clamp_clone_tr_delta(mut delta: [f32; 3]) -> [f32; 3] {
    for speed in &mut delta[..2] {
        *speed = speed.clamp(-G_CLONE_PLAYER_MAX_VELOCITY, G_CLONE_PLAYER_MAX_VELOCITY);
    }
    delta
}

const CORPSE_ANIM_DELTA_LEN_SQ_MIN: f32 = 1.0;

const CORPSE_ANIM_AXIS_Y_SCALE: f32 = -1.0;

const CORPSE_GROUND_PROBE_DROP: f32 = 1.0;

const CORPSE_STEP_DOWN: f32 = 18.0;

const CORPSE_WALL_SLIDE_GAP: f32 = 0.125;

const CORPSE_UNSTICK_Z_LIFT: f32 = 32.0;

const CORPSE_UNSTICK_CLIPMASK_DROP: u32 = 0x0001_0000;

const TR_RAGDOLL_SETTLED: i32 = 12;

const TR_BOUNCE_GRAVITY: i32 = 0xb;

fn is_ragdoll_tr_type(tr_type: i32) -> bool {
    (10..=12).contains(&tr_type)
}

pub(crate) fn phase_run_corpse_move(world: &mut FrameWorld, time_ms: i32) {
    let n = world.corpses().slots.len();
    for i in 0..n {
        let slot = world.corpses().slots[i];
        if !slot.occupied {
            continue;
        }
        let anim_delta = world.corpse_dobj_tree_delta(slot.entnum, crate::MATCH_TICK_MS as i32);
        let frame = &*world;
        let stepped = step_corpse(slot, anim_delta, time_ms, |start, end, mask| {
            let content = frame.content();
            let map = crate::world_graph::world_at(start);
            let beyond = map
                .checked_sub(1)
                .and_then(|i| content.atlas()?.worlds.get(i));
            match beyond {
                None => frame.trace_clip(start, end, PLAYER_MINS, PLAYER_MAXS, mask),
                Some(clip) => trace_atlas_corpse(clip, map, start, end, mask),
            }
        });
        world.corpses_mut().slots[i] = stepped;
    }
    let live = world.corpses().entnums();
    world.corpse_dobj_tree_retain(&live);
}

fn trace_atlas_corpse(
    clip: &crate::world_graph::AtlasWorld,
    map: usize,
    start: [f32; 3],
    end: [f32; 3],
    mask: u32,
) -> trace_iw4::Trace {
    // A corpse keeps moving in the map where the player died, just as
    // the living player's capsule does. Base-map clip is in another slab.
    let backend = crate::bg::player_collision::PlayerCollision {
        brushes: &clip.brushes,
        bsp: &clip.bsp,
        mesh: &clip.mesh,
        glass_damage: &[],
        bodies: &[],
        self_entnum: u16::MAX,
        cmodels: &clip.cmodels.models,
        linked_brushes: &[],
        model_brushes: &[],
        offset: crate::world_graph::world_offset(map),
    };
    movement_iw4::CollisionBackend::trace(
        &backend,
        movement_iw4::GroundTraceInput {
            start,
            end,
            mins: PLAYER_MINS,
            maxs: PLAYER_MAXS,
            tracemask: mask,
        },
    )
}

fn step_corpse(
    mut slot: PlayerCorpseSlot,
    anim_delta: Option<[f32; 3]>,
    time_ms: i32,
    mut trace: impl FnMut([f32; 3], [f32; 3], u32) -> trace_iw4::Trace,
) -> PlayerCorpseSlot {
    let was_falling = slot.falling;
    let ragdoll = is_ragdoll_tr_type(slot.tr_type);
    let anim_delta = anim_delta.filter(|d| {
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > CORPSE_ANIM_DELTA_LEN_SQ_MIN
            && (!was_falling || !ragdoll)
    });
    if !was_falling && anim_delta.is_none() {
        return slot;
    }
    let world_delta = anim_delta.map(|delta| {
        let step = corpse_anim_world_delta(slot.viewangles, delta);
        if was_falling {
            step
        } else {
            [step[0], step[1], 0.0]
        }
    });

    let mut desired = evaluate_trajectory(&slot.trajectory(), time_ms);
    if let Some(step) = world_delta {
        for axis in 0..3 {
            desired[axis] += step[axis];
        }
    }
    let hit = trace(slot.origin, desired, MASK_PLAYER_SOLID);
    let endpos = if hit.fraction >= 1.0 {
        desired
    } else {
        hit.endpos
    };
    slot.origin = endpos;

    if hit.fraction >= 1.0 && hit.startsolid == 0 {
        let Some(step) = world_delta else {
            return slot;
        };
        if was_falling {
            for (base, step) in slot.tr_base.iter_mut().zip(step) {
                *base += step;
            }
            return slot;
        }
        slot.tr_type = if ragdoll {
            TR_RAGDOLL_SETTLED
        } else {
            TR_INTERPOLATE
        };
        slot.tr_base = endpos;
        slot.tr_time = 0;
        slot.tr_duration = 0;
        slot.tr_delta = [0.0; 3];
        follow_ground_or_fall(&mut slot, time_ms, ragdoll, &mut trace);
        return slot;
    }

    if !was_falling {
        slot.tr_base = endpos;
        if hit.fraction > 0.0 && hit.startsolid == 0 {
            follow_ground_or_fall(&mut slot, time_ms, ragdoll, &mut trace);
        }
        return slot;
    }

    if hit.allsolid != 0 {
        let mut lifted = slot.origin;
        lifted[2] += CORPSE_UNSTICK_Z_LIFT;
        let retry = trace(
            lifted,
            desired,
            MASK_PLAYER_SOLID & !CORPSE_UNSTICK_CLIPMASK_DROP,
        );
        if retry.allsolid == 0 {
            let f = retry.fraction;
            slot.origin = [
                lifted[0] + (desired[0] - lifted[0]) * f,
                lifted[1] + (desired[1] - lifted[1]) * f,
                lifted[2] + (desired[2] - lifted[2]) * f,
            ];
        }
        land_corpse(&mut slot, &hit);
        return slot;
    }
    if hit.normal[2] <= 0.0 {
        if hit.startsolid != 0 {
            for axis in 0..3 {
                slot.origin[axis] += hit.normal[axis];
            }
            slot.tr_base = slot.origin;
            slot.tr_time = time_ms;
            slot.tr_delta = [0.0; 3];
            return slot;
        }
        slide_down_wall(&mut slot, &hit, desired, time_ms, &mut trace);
        return slot;
    }
    land_corpse(&mut slot, &hit);
    slot
}

fn follow_ground_or_fall(
    slot: &mut PlayerCorpseSlot,
    time_ms: i32,
    ragdoll: bool,
    trace: &mut impl FnMut([f32; 3], [f32; 3], u32) -> trace_iw4::Trace,
) {
    let at = slot.origin;
    let mut probe = at;
    probe[2] -= CORPSE_GROUND_PROBE_DROP;
    let ground = trace(at, probe, MASK_PLAYER_SOLID);
    if ground.fraction < 1.0 || ground.startsolid != 0 {
        slot.falling = false;
        return;
    }
    let mut step_probe = at;
    step_probe[2] -= CORPSE_STEP_DOWN;
    let step = trace(at, step_probe, MASK_PLAYER_SOLID);
    if step.fraction < 1.0 && step.startsolid == 0 && step.allsolid == 0 && step.normal[2] > 0.0 {
        slot.origin = step.endpos;
        slot.tr_base = step.endpos;
        slot.falling = false;
        slot.ground_entity_num = i32::from(trace_iw4::trace_get_entity_hit_id(
            step.hit_type,
            step.hit_id,
        ));
        return;
    }
    slot.falling = true;
    slot.tr_type = if ragdoll {
        TR_BOUNCE_GRAVITY
    } else {
        TR_GRAVITY
    };
    slot.tr_base = at;
    slot.tr_time = time_ms;
    slot.tr_duration = 0;
    slot.tr_delta = [0.0; 3];
}

fn slide_down_wall(
    slot: &mut PlayerCorpseSlot,
    hit: &trace_iw4::Trace,
    desired: [f32; 3],
    time_ms: i32,
    trace: &mut impl FnMut([f32; 3], [f32; 3], u32) -> trace_iw4::Trace,
) {
    let n = hit.normal;
    let mut start = hit.endpos;
    let mut rest = [0.0f32; 3];
    for axis in 0..3 {
        start[axis] += n[axis] * CORPSE_WALL_SLIDE_GAP;
        rest[axis] = desired[axis] - hit.endpos[axis];
    }
    let into_wall = dot(rest, n);
    let mut end = start;
    for axis in 0..3 {
        end[axis] += rest[axis] - into_wall * n[axis];
    }
    let slid = trace(start, end, MASK_PLAYER_SOLID);
    if slid.allsolid == 0 && slid.startsolid == 0 {
        slot.origin = if slid.fraction >= 1.0 {
            end
        } else {
            slid.endpos
        };
        if slid.fraction < 1.0 && slid.normal[2] > 0.0 {
            land_corpse(slot, &slid);
            return;
        }
    }
    let speed = evaluate_trajectory_delta(&slot.trajectory(), time_ms);
    let mut velocity = [0.0, 0.0, speed[2]];
    let into = dot(velocity, n);
    if into < 0.0 {
        for axis in 0..3 {
            velocity[axis] -= into * n[axis];
        }
    }
    slot.tr_base = slot.origin;
    slot.tr_time = time_ms;
    slot.tr_duration = 0;
    slot.tr_delta = velocity;
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn corpse_anim_world_delta(angles: [f32; 3], delta: [f32; 3]) -> [f32; 3] {
    let (mut fwd, right, up) = math_iw4::angle_vectors(angles);
    let mut left = [
        right[0] * CORPSE_ANIM_AXIS_Y_SCALE,
        right[1] * CORPSE_ANIM_AXIS_Y_SCALE,
        right[2] * CORPSE_ANIM_AXIS_Y_SCALE,
    ];
    normalize(&mut fwd);
    normalize(&mut left);
    let mut out = [0.0f32; 3];
    for (axis, slot) in out.iter_mut().enumerate() {
        *slot = delta[0] * fwd[axis] + delta[1] * left[axis] + delta[2] * up[axis];
    }
    out
}

fn land_corpse(slot: &mut PlayerCorpseSlot, hit: &trace_iw4::Trace) {
    let ragdoll = is_ragdoll_tr_type(slot.tr_type);
    slot.tr_delta = [0.0; 3];
    slot.falling = false;
    slot.tr_type = if ragdoll {
        TR_RAGDOLL_SETTLED
    } else {
        TR_INTERPOLATE
    };
    slot.tr_base = slot.origin;
    slot.tr_time = 0;
    slot.tr_duration = 0;

    slot.ground_entity_num =
        i32::from(trace_iw4::trace_get_entity_hit_id(hit.hit_type, hit.hit_id));
    if hit.allsolid == 0 {
        slot.viewangles = corpse_land_angles(slot.viewangles[1], hit.normal);
    }
}

fn normalize(v: &mut [f32; 3]) {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let len = if len <= 0.0 { 1.0 } else { len };
    v[0] /= len;
    v[1] /= len;
    v[2] /= len;
}

pub(crate) fn phase_sync_corpse_info(world: &mut FrameWorld) {
    let n = world.corpses().slots.len();
    for i in 0..n {
        if world.corpses().slots[i].occupied {
            sync_corpse_info_player_anims(world, i);
        }
    }
}

pub(crate) fn sync_corpse_info_player_anims(world: &mut FrameWorld, slot: usize) {
    let body = world.corpses().slots[slot];
    let src = entity_iw4::CorpseInfoPlayerAnimCopy {
        legs_anim: body.anim.legs_anim,
        torso_anim: body.anim.torso_anim,
        torso_pitch: 0,
        waist_pitch: 0,
    };
    let Some(copied) = entity_iw4::corpse_info::copy_player_anims(false, src) else {
        return;
    };
    let slot = &mut world.corpses_mut().slots[slot];
    slot.anim.legs_anim = copied.legs_anim;
    slot.anim.torso_anim = copied.torso_anim;
}
