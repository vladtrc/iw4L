use crate::Tick;
use crate::frame::FrameWorld;
use crate::gentity::{EntityRunKind, ThinkEnter, apos_from_entity_state, pos_from_entity_state};
use entity_iw4::{
    evaluate_trajectory, link_entity_needs_rotated_radius, link_entity_world_bounds,
    parent_link_apply_local, parent_link_pose, parent_link_world_from_tag,
};
use math_iw4::snap_angles;

pub(crate) fn phase_run_entity_thinks(world: &mut FrameWorld, tick: Tick) {
    let time_ms = crate::corpse::level_time_ms(tick);
    crate::corpse::phase_run_corpse_move(world, time_ms);
    crate::corpse::phase_sync_corpse_info(world);
    let WalkedThinks { order, dispatch } = walk_thinks(world, tick, time_ms, true);
    world.stamp_think_order_rows(order);
    world.stamp_think_dispatch(dispatch);
}

pub(crate) fn phase_walk_entity_thinks(world: &mut FrameWorld) {
    let WalkedThinks { order, .. } = walk_thinks(world, Tick(0), 0, false);
    world.stamp_think_order_rows(order);
    world.stamp_think_dispatch(Vec::new());
}

struct WalkedThinks {
    order: Vec<crate::EntityRef>,
    dispatch: Vec<(i32, EntityRunKind)>,
}

struct WalkCtx<'a> {
    tick: Tick,
    time_ms: i32,
    order: &'a mut Vec<crate::EntityRef>,
    dispatch: &'a mut Vec<(i32, EntityRunKind)>,
    dispatch_machines: bool,
}

fn walk_thinks(
    world: &mut FrameWorld,
    tick: Tick,
    time_ms: i32,
    dispatch_machines: bool,
) -> WalkedThinks {
    let mut order = Vec::new();
    let mut dispatch = Vec::new();
    {
        let mut ctx = WalkCtx {
            tick,
            time_ms,
            order: &mut order,
            dispatch: &mut dispatch,
            dispatch_machines,
        };
        let mut number = crate::GENTITY_RESERVED_COUNT;
        while number < world.entity_kernel().high_water() {
            run_think(world, number, &mut ctx);
            number += 1;
        }
    }
    WalkedThinks { order, dispatch }
}

fn run_think(world: &mut FrameWorld, number: i32, ctx: &mut WalkCtx<'_>) {
    let entered = match world.entity_kernel_mut().begin_think(number) {
        Ok(ThinkEnter::Run {
            entity,
            kind: _,
            parent,
        }) => (entity, parent),
        Ok(ThinkEnter::Skip) | Err(_) => return,
    };
    if let Some(parent) = entered.1 {
        run_think(world, parent.number(), ctx);
    }
    let Ok(view) = world.entity_kernel().resolve(entered.0) else {
        return;
    };
    let kind = view.kind;
    let entity = entered.0;
    ctx.order.push(entity);
    if !ctx.dispatch_machines {
        return;
    }
    ctx.dispatch.push((entity.number(), kind));
    match kind {
        EntityRunKind::Missile => {
            crate::equipment::think_projectile(world, ctx.tick, entity.number());
        }
        EntityRunKind::Item => {
            let parented = view.relations.parent.is_some();
            think_parent_link(world, ctx.time_ms, entity);
            if parented {
                think_scheduled_or_refuse(world, entity);
            } else {
                crate::item::think_item_move(world, ctx.time_ms, entity.number());
            }
        }
        EntityRunKind::ScriptMover | EntityRunKind::PrimaryLight => {
            think_script_mover(world, ctx.tick, entity);
        }
        EntityRunKind::TempEvent => {}
        EntityRunKind::General => {
            think_parent_link(world, ctx.time_ms, entity);
            think_scheduled_or_refuse(world, entity);
        }
        EntityRunKind::PlayerCorpse => {
            think_player_corpse(world, entity);
        }
    }
}

fn think_player_corpse(world: &mut FrameWorld, entity: crate::EntityRef) {
    let number = entity.number();
    let entnums = world.corpses().entnums();
    if !entity_iw4::corpse_info::entnum_matched(&entnums, number) {
        panic!(
            "ET_PLAYER_CORPSE occupancy number not in the corpse table; a miss falls back to slot 0"
        );
    }
    let slot = entity_iw4::corpse_info::slot_for_entnum(&entnums, number);
    crate::corpse::sync_corpse_info_player_anims(world, slot);
    think_scheduled_or_refuse(world, entity);
}

fn think_scheduled_or_refuse(world: &mut FrameWorld, entity: crate::EntityRef) {
    let due = world
        .entity_kernel_mut()
        .take_due_think(entity)
        .expect("scheduled think occupancy vanished");
    if due {
        panic!("generic think has no handler");
    }
}

fn think_script_mover(world: &mut FrameWorld, tick: Tick, entity: crate::EntityRef) {
    let time_ms = crate::corpse::level_time_ms(tick);
    let Ok(view) = world.entity_kernel().resolve(entity) else {
        return;
    };
    if view.relations.parent.is_some() {
        think_parent_link(world, time_ms, entity);
        think_scheduled_or_refuse(world, entity);
    }
    let at_time = i32::try_from(tick.0.saturating_mul(crate::MATCH_TICK_MS)).unwrap_or(i32::MAX);
    let Some(mover) = world.script_mover_by_number(entity.number()) else {
        return;
    };
    let content = world.content();
    if let Some(row) = world.collision_owner_mut(mover.id) {
        crate::presence::follow_mover(row, &mover, at_time, &content);
    }
}

fn think_parent_link(world: &mut FrameWorld, time_ms: i32, entity: crate::EntityRef) {
    let Ok(view) = world.entity_kernel().resolve(entity) else {
        return;
    };
    let Some(parent) = view.relations.parent else {
        return;
    };
    let tag = view.relations.parent_tag;
    let Some((origin, angles)) = parent_world_pose(world, parent, time_ms) else {
        panic!("parented think has no parent currentOrigin/angles");
    };
    let (origin, angles) = if tag >= 0 {
        let Some(mat) = world.anim_mat(parent.number(), tag) else {
            panic!("tag-linked think needs a computed bone matrix for the parent tag");
        };
        parent_link_world_from_tag(origin, angles, mat)
    } else {
        (origin, angles)
    };
    let (origin, angles) = parent_link_apply_local(
        origin,
        angles,
        view.relations.parent_link_axis,
        view.relations.parent_link_origin,
    );
    apply_parent_link_pose(world, entity.number(), view.kind, origin, angles);
}

fn parent_world_pose(
    world: &FrameWorld,
    parent: crate::EntityRef,
    time_ms: i32,
) -> Option<([f32; 3], [f32; 3])> {
    let kind = world.entity_kernel().resolve(parent).ok()?.kind;
    match kind {
        EntityRunKind::ScriptMover | EntityRunKind::PrimaryLight => {
            world.script_mover_by_number(parent.number()).map(|mover| {
                (
                    evaluate_trajectory(&pos_from_entity_state(&mover.state), time_ms),
                    evaluate_trajectory(&apos_from_entity_state(&mover.state), time_ms),
                )
            })
        }
        EntityRunKind::Item => world.dropped_item_by_number(parent.number()).map(|item| {
            (
                item.origin,
                evaluate_trajectory(&apos_from_entity_state(&item.state), time_ms),
            )
        }),
        EntityRunKind::Missile
        | EntityRunKind::TempEvent
        | EntityRunKind::General
        | EntityRunKind::PlayerCorpse => None,
    }
}

fn apply_parent_link_pose(
    world: &mut FrameWorld,
    number: i32,
    kind: EntityRunKind,
    origin: [f32; 3],
    angles: [f32; 3],
) {
    match kind {
        EntityRunKind::ScriptMover | EntityRunKind::PrimaryLight => {
            if let Some(mover) = world.script_mover_mut_by_number(number) {
                parent_link_pose(&mut mover.state, origin, angles);
                let snapped = snap_angles(angles);
                if link_entity_needs_rotated_radius(snapped, mover.box_half) {
                    panic!("link yaw/pitch radius Bounds when r.half is non-zero");
                }
                let bounds = link_entity_world_bounds(origin, mover.box_mid, mover.box_half);
                mover.link_mid = bounds.mid;
                mover.link_half = bounds.half;
            }
        }
        EntityRunKind::Item => {
            if let Some(item) = world.dropped_item_mut_by_number(number) {
                item.origin = origin;
                parent_link_pose(&mut item.state, origin, angles);
            }
        }
        EntityRunKind::Missile
        | EntityRunKind::TempEvent
        | EntityRunKind::General
        | EntityRunKind::PlayerCorpse => {}
    }
}
