use playerstate_iw4::{PlayerState, UserCmd, buttons, eflags};

use crate::check_prone::player_prone_allowed;
use crate::collision::CollisionBackend;
use crate::ladder::PMF_LADDER;
use crate::sprint::PMF_SPRINTING;
use crate::{GroundTraceInput, MoveBounds, Pml, add_predictable_event};

pub mod view_height {
    pub const PRONE: i32 = 0x0b;

    pub const LAST_STAND: i32 = 0x16;

    pub const CROUCH: i32 = 0x28;

    pub const STAND: i32 = 0x3c;
}

const PMF_LAST_STAND: u32 = 0x0040_0000;

pub const PMF_PRONE: u32 = 0x1;

pub const PMF_CROUCH: u32 = 0x2;

pub const STAND_MAXS_Z: f32 = 70.0;

pub const CROUCH_MAXS_Z: f32 = 50.0;

pub const PRONE_MAXS_Z: f32 = 30.0;

const EV_STANCE_FORCE_STAND: i32 = 6;

const EV_STANCE_FORCE_CROUCH: i32 = 7;

const EV_STANCE_FORCE_PRONE: i32 = 8;

const PMF_STANCE_LOCKED: u32 = 0xc00;

fn stance_hull_allsolid<C: CollisionBackend>(
    collision: &C,
    origin: [f32; 3],
    bounds: MoveBounds,
    maxs_z: f32,
) -> bool {
    let trace = collision.trace(GroundTraceInput {
        start: origin,
        end: origin,
        mins: bounds.mins,
        maxs: [bounds.maxs[0], bounds.maxs[1], maxs_z],
        tracemask: bounds.tracemask,
    });
    trace.allsolid != 0
}

pub fn pm_update_stance_flags<C: CollisionBackend>(
    ps: &mut PlayerState,
    cmd: &mut UserCmd,
    collision: &C,
    bounds: MoveBounds,
    weapon_blocks_prone: bool,
) {
    if ps.pm_type == 5 {
        ps.pm_flags &= !(PMF_PRONE | PMF_CROUCH);
        if (cmd.buttons & buttons::PRONE) != 0 {
            cmd.buttons &= !buttons::PRONE;
            add_predictable_event(ps, EV_STANCE_FORCE_STAND, 0);
        }
        ps.view_height_target = 0;
        return;
    }

    if ps.pm_type == 8 {
        ps.view_height_target = 8;
        return;
    }

    if (ps.pm_flags & PMF_SPRINTING) != 0 {
        ps.view_height_target = view_height::STAND;
        ps.e_flags &= !(eflags::DUCK | eflags::PRONE);
        ps.pm_flags &= !(PMF_PRONE | PMF_CROUCH);
        add_predictable_event(ps, EV_STANCE_FORCE_STAND, 0);
        return;
    }

    if (ps.pm_flags & PMF_STANCE_LOCKED) != 0 {
        return;
    }

    if ps.pm_type == 7 {
        ps.pm_flags = (ps.pm_flags & !PMF_CROUCH) | PMF_PRONE;
        return;
    }

    if (ps.pm_flags & PMF_LADDER) != 0 && (cmd.buttons & 0x300) != 0 {
        cmd.buttons &= !0x300;
        add_predictable_event(ps, EV_STANCE_FORCE_STAND, 0);
    }

    let crouch_btn = (cmd.buttons & buttons::CROUCH) != 0;
    let prone_btn = (cmd.buttons & buttons::PRONE) != 0;
    let stance_held = (cmd.buttons & buttons::STANCE_HELD) != 0;
    let origin = ps.origin;

    if prone_btn && (ps.pm_flags & 0x400) == 0 {
        if player_prone_allowed(ps, collision, bounds.maxs[0], weapon_blocks_prone) {
            ps.pm_flags = (ps.pm_flags & !PMF_CROUCH) | PMF_PRONE;
        } else if ps.ground_entity_num != playerstate_iw4::ENTITYNUM_NONE && !stance_held {
            let event = if (ps.pm_flags & (PMF_PRONE | PMF_CROUCH)) == 0 {
                EV_STANCE_FORCE_STAND
            } else {
                EV_STANCE_FORCE_CROUCH
            };
            add_predictable_event(ps, event, 3);
        }
    } else if crouch_btn {
        if (ps.pm_flags & PMF_PRONE) == 0 {
            ps.pm_flags |= PMF_CROUCH;
        } else if !stance_hull_allsolid(collision, origin, bounds, CROUCH_MAXS_Z) {
            ps.pm_flags = (ps.pm_flags & !PMF_PRONE) | PMF_CROUCH;
        } else if !stance_held {
            add_predictable_event(ps, EV_STANCE_FORCE_PRONE, 2);
        }
    } else if (ps.pm_flags & PMF_PRONE) != 0 {
        if !stance_hull_allsolid(collision, origin, bounds, STAND_MAXS_Z) {
            ps.pm_flags &= !(PMF_PRONE | PMF_CROUCH);
        } else if !stance_hull_allsolid(collision, origin, bounds, CROUCH_MAXS_Z) {
            ps.pm_flags = (ps.pm_flags & !PMF_PRONE) | PMF_CROUCH;
        } else if !stance_held {
            add_predictable_event(ps, EV_STANCE_FORCE_PRONE, 1);
        }
    } else if (ps.pm_flags & PMF_CROUCH) != 0 {
        if !stance_hull_allsolid(collision, origin, bounds, STAND_MAXS_Z) {
            ps.pm_flags &= !PMF_CROUCH;
        } else if !stance_held {
            add_predictable_event(ps, EV_STANCE_FORCE_CROUCH, 1);
        }
    }
}

pub fn pm_sync_stance_tail(ps: &mut PlayerState) -> f32 {
    match stance_surface_type(ps) {
        StanceSurface::Prone => {
            ps.e_flags = (ps.e_flags & !eflags::DUCK) | eflags::PRONE;
            ps.pm_flags = (ps.pm_flags & !PMF_CROUCH) | PMF_PRONE;
            PRONE_MAXS_Z
        }
        StanceSurface::Crouch => {
            ps.e_flags = (ps.e_flags & !eflags::PRONE) | eflags::DUCK;
            ps.pm_flags = (ps.pm_flags & !PMF_PRONE) | PMF_CROUCH;
            CROUCH_MAXS_Z
        }
        StanceSurface::LastStand => {
            ps.e_flags = (ps.e_flags & !eflags::DUCK) | eflags::PRONE;
            ps.pm_flags = (ps.pm_flags & !PMF_CROUCH) | PMF_PRONE;
            CROUCH_MAXS_Z
        }
        StanceSurface::Stand => {
            ps.e_flags &= !(eflags::DUCK | eflags::PRONE);
            ps.pm_flags &= !(PMF_PRONE | PMF_CROUCH);
            STAND_MAXS_Z
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StanceSurface {
    Stand = 0,

    Prone = 1,

    Crouch = 2,

    LastStand = 3,
}

#[must_use]
pub fn stance_surface_type(ps: &PlayerState) -> StanceSurface {
    match ps.view_height_target {
        view_height::LAST_STAND => StanceSurface::LastStand,
        view_height::CROUCH => StanceSurface::Crouch,
        view_height::PRONE => StanceSurface::Prone,
        _ => StanceSurface::Stand,
    }
}

#[must_use]
pub fn view_height_lerp_duration(lerp_target: i32, lerp_down: i32) -> i32 {
    match lerp_target {
        view_height::PRONE => 400,
        view_height::CROUCH => {
            if lerp_down != 0 {
                200
            } else {
                400
            }
        }
        _ => 200,
    }
}

type Knot = (i32, f32);

const CROUCH_FROM_STAND: &[Knot] = &[
    (0, 60.0),
    (1, 59.5),
    (4, 58.5),
    (30, 56.0),
    (80, 44.0),
    (90, 41.5),
    (95, 40.5),
    (100, 40.0),
];

const STAND_CURVE: &[Knot] = &[
    (0, 40.0),
    (5, 40.5),
    (10, 41.5),
    (20, 44.0),
    (70, 56.0),
    (96, 58.5),
    (99, 59.5),
    (100, 60.0),
];

const PRONE_CURVE: &[Knot] = &[
    (0, 40.0),
    (11, 38.0),
    (22, 33.0),
    (34, 25.0),
    (45, 16.0),
    (50, 15.0),
    (55, 16.0),
    (70, 18.0),
    (90, 17.0),
    (100, 11.0),
];

const CROUCH_FROM_PRONE: &[Knot] = &[
    (0, 11.0),
    (5, 10.0),
    (30, 21.0),
    (50, 25.0),
    (67, 31.0),
    (83, 34.0),
    (100, 40.0),
];

fn view_height_curve(lerp_target: i32, lerp_down: i32) -> &'static [Knot] {
    if lerp_target == view_height::PRONE {
        return PRONE_CURVE;
    }
    if lerp_target == view_height::CROUCH {
        if lerp_down != 0 {
            return CROUCH_FROM_STAND;
        }
        return CROUCH_FROM_PRONE;
    }
    STAND_CURVE
}

#[must_use]
fn view_height_lerp_sample(table: &[Knot], key: i32) -> f32 {
    if key != 0 {
        let mut index = 1;
        while index < table.len() {
            let (knot_key, knot_value) = table[index];
            if knot_key == key {
                return knot_value;
            }
            if key <= knot_key {
                let (prev_key, prev_value) = table[index - 1];
                let fraction = ((key - prev_key) as f32) / ((knot_key - prev_key) as f32);
                return (knot_value - prev_value) * fraction + prev_value;
            }
            index += 1;
        }
    }
    table[0].1
}

const VIEW_HEIGHT_RATE: f32 = 180.0;

const LAST_STAND_VIEW_HEIGHT_RATE: f32 = 120.0;

const REVERSE_REWIND_SCALE: f32 = 0.01;

#[allow(clippy::too_many_lines)]
pub fn pm_update_view_height(ps: &mut PlayerState, pml: &Pml, cmd: &UserCmd) {
    let target = ps.view_height_target;
    if target == 0 || ps.view_height_current == 0.0 {
        ps.view_height_current = if ps.pm_type == 5 { 0.0 } else { target as f32 };
        return;
    }

    let target_height = target as f32;
    if target_height == ps.view_height_current && ps.view_height_lerp_time == 0 {
        return;
    }

    let last_stand = (ps.pm_flags & PMF_LAST_STAND) != 0;
    let unknown_token = target != view_height::PRONE
        && target != view_height::CROUCH
        && target != view_height::STAND;
    if unknown_token || (last_stand && target_height < ps.view_height_current) {
        let rate = if last_stand {
            LAST_STAND_VIEW_HEIGHT_RATE
        } else {
            VIEW_HEIGHT_RATE
        };
        ps.view_height_lerp_time = 0;
        if ps.view_height_current < target_height {
            let raised = pml.frametime * rate + ps.view_height_current;
            ps.view_height_current = raised;
            if raised < target_height {
                return;
            }
        } else {
            let lowered = ps.view_height_current - pml.frametime * rate;
            ps.view_height_current = lowered;

            if lowered > target_height {
                return;
            }
        }
        ps.view_height_current = target_height;
        return;
    }

    let mut progress = 0i32;
    if ps.view_height_lerp_time != 0 {
        let lerp_target = ps.view_height_lerp_target;
        let duration = view_height_lerp_duration(lerp_target, ps.view_height_lerp_down);
        progress = cmd
            .server_time
            .wrapping_sub(ps.view_height_lerp_time)
            .wrapping_mul(100)
            / duration;
        if progress >= 100 {
            progress = 100;
            ps.view_height_lerp_time = 0;
            ps.view_height_current = lerp_target as f32;
        } else {
            if progress < 0 {
                progress = 0;
            }
            let table = view_height_curve(lerp_target, ps.view_height_lerp_down);
            ps.view_height_current = view_height_lerp_sample(table, progress);
        }
    }

    if ps.view_height_lerp_time == 0 {
        start_view_height_lerp(ps, cmd);
    } else {
        reverse_view_height_lerp(ps, cmd, progress);
    }
}

fn start_view_height_lerp(ps: &mut PlayerState, cmd: &UserCmd) {
    let target = ps.view_height_target;
    if (target as f32) == ps.view_height_current {
        return;
    }
    ps.view_height_lerp_time = cmd.server_time;

    const CROUCH_HEIGHT: f32 = 40.0;
    match target {
        view_height::PRONE => {
            ps.view_height_lerp_down = 1;
            ps.view_height_lerp_target = if ps.view_height_current <= CROUCH_HEIGHT {
                view_height::PRONE
            } else {
                view_height::CROUCH
            };
        }
        view_height::CROUCH => {
            if (target as f32) < ps.view_height_current {
                ps.view_height_lerp_down = 1;
                ps.view_height_lerp_target = view_height::CROUCH;
            } else {
                ps.view_height_lerp_down = 0;
                ps.view_height_lerp_target = view_height::CROUCH;
            }
        }
        view_height::STAND => {
            ps.view_height_lerp_down = 0;
            ps.view_height_lerp_target = if CROUCH_HEIGHT <= ps.view_height_current {
                view_height::STAND
            } else {
                view_height::CROUCH
            };
        }
        _ => {}
    }
}

fn reverse_view_height_lerp(ps: &mut PlayerState, cmd: &UserCmd, progress: i32) {
    let target = ps.view_height_target;
    let lerp_target = ps.view_height_lerp_target;
    if target == lerp_target {
        return;
    }

    let down = if target < lerp_target {
        if ps.view_height_lerp_down != 0 {
            return;
        }
        0
    } else {
        if ps.view_height_lerp_down == 0 {
            return;
        }
        ps.view_height_lerp_down
    };

    let progress = 100 - progress;
    let down = down ^ 1;
    ps.view_height_lerp_down = down;
    if down == 0 {
        match lerp_target {
            view_height::PRONE => ps.view_height_lerp_target = view_height::CROUCH,
            view_height::CROUCH => ps.view_height_lerp_target = view_height::STAND,
            _ => {}
        }
    } else {
        match lerp_target {
            view_height::STAND => ps.view_height_lerp_target = view_height::CROUCH,
            view_height::CROUCH => ps.view_height_lerp_target = view_height::PRONE,
            _ => {}
        }
    }

    if progress == 100 {
        ps.view_height_lerp_time = 0;
        ps.view_height_current = ps.view_height_lerp_target as f32;
        return;
    }

    let duration = view_height_lerp_duration(ps.view_height_lerp_target, down);
    let elapsed = ((duration as f32) * (progress as f32) * REVERSE_REWIND_SCALE) as i32;
    ps.view_height_lerp_time = cmd.server_time.wrapping_sub(elapsed);
}

#[must_use]
pub fn pm_update_stance_target(ps: &mut PlayerState) -> StanceChange {
    if ps.view_height_lerp_time != 0 {
        return StanceChange::Unchanged;
    }

    if ps.pm_type == 7 {
        if ps.view_height_target == view_height::LAST_STAND {
            return StanceChange::Unchanged;
        }
        ps.view_height_target = view_height::LAST_STAND;
    } else if (ps.pm_flags & PMF_PRONE) == 0 {
        let previous = ps.view_height_target;
        if previous == view_height::PRONE {
            ps.view_height_target = view_height::CROUCH;
        } else {
            ps.view_height_target = if (ps.pm_flags & PMF_CROUCH) != 0 {
                view_height::CROUCH
            } else {
                view_height::STAND
            };
            if previous != view_height::LAST_STAND {
                return StanceChange::Unchanged;
            }
        }
    } else {
        if ps.view_height_target == view_height::STAND {
            ps.view_height_target = view_height::CROUCH;
            return StanceChange::Unchanged;
        }
        if ps.view_height_target == view_height::PRONE {
            return StanceChange::Unchanged;
        }
        ps.view_height_target = view_height::PRONE;
        return StanceChange::EnteredProne;
    }

    StanceChange::Changed
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StanceChange {
    Unchanged,

    Changed,

    EnteredProne,
}

#[must_use]
pub fn stance_speed_scale(ps: &PlayerState, server_time: i32, last_stand_scale: f32) -> f32 {
    const TRANSITION_MS: f32 = 400.0;

    const PRONE_SCALE: f32 = 0.15;

    const CROUCH_SCALE: f32 = 0.65;

    if ps.view_height_lerp_time != 0 && ps.view_height_lerp_target == view_height::PRONE {
        let fraction = (server_time.wrapping_sub(ps.view_height_lerp_time) as f32) / TRANSITION_MS;
        if fraction >= 0.0 {
            let fraction = if fraction > 1.0 { 1.0 } else { fraction };
            if fraction != 0.0 {
                return fraction * PRONE_SCALE + (1.0 - fraction) * CROUCH_SCALE;
            }
        }
    }

    if ps.view_height_lerp_time != 0
        && ps.view_height_lerp_target == view_height::CROUCH
        && ps.view_height_lerp_down == 0
    {
        let fraction = (server_time.wrapping_sub(ps.view_height_lerp_time) as f32) / TRANSITION_MS;
        if fraction >= 0.0 {
            let fraction = if fraction > 1.0 { 1.0 } else { fraction };
            if fraction != 0.0 {
                return fraction * CROUCH_SCALE + (1.0 - fraction) * PRONE_SCALE;
            }
        }
    }

    match stance_surface_type(ps) {
        StanceSurface::LastStand => last_stand_scale,
        StanceSurface::Crouch => CROUCH_SCALE,
        StanceSurface::Prone => PRONE_SCALE,
        StanceSurface::Stand => 1.0,
    }
}
