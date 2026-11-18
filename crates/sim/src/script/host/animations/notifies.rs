use super::Animator;
use crate::script::{RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;
use xmodel_runtime::XAnimNodeId;

const NOTIFY_STEP_EPSILON: f32 = 0.001;
const MAX_NOTIFY_STEPS: usize = 4096;

struct Notification {
    owner: u64,
    node: usize,
    registration: Arc<str>,
    track: String,
}

fn notify_rate(anim: &Animator, node: usize) -> f32 {
    let mut ancestor = Some(XAnimNodeId(node as u16));
    while let Some(at) = ancestor {
        let state = anim.runtime.states()[at.0 as usize];
        if state.weight == 0.0 || state.goal_weight == 0.0 {
            return 0.0;
        }
        ancestor = anim.runtime.definition().nodes()[at.0 as usize].parent;
    }
    anim.runtime
        .sync_clock_rate(&anim.sync, XAnimNodeId(node as u16))
}

fn next_notify(anim: &Animator) -> Option<f32> {
    anim.notifies
        .iter()
        .enumerate()
        .filter_map(|(node, registration)| {
            registration.as_ref()?;
            let clip = anim.runtime.leaf_clip(anim.notify_children[node]?)?;
            let source = anim
                .sync
                .group(XAnimNodeId(node as u16))
                .unwrap_or(XAnimNodeId(node as u16));
            let state = anim.runtime.states()[source.0 as usize];
            let rate = notify_rate(anim, node);
            if rate <= 0.0
                || state.time == 1.0
                || (!clip.looping
                    && state.time == anim_iw4::XANIM_NONLOOP_END_PARK
                    && state.old_time < state.time)
            {
                return None;
            }
            let next = clip
                .notifies
                .iter()
                .filter(|track| track.time >= state.time)
                .map(|track| track.time)
                .min_by(f32::total_cmp)
                .unwrap_or(1.0);
            Some((next - state.time) / rate)
        })
        .min_by(f32::total_cmp)
}

fn crossed_notifies(
    anim: &Animator,
    owner: u64,
    old: &[xmodel_runtime::XAnimNodeState],
    seconds: f32,
    events: &mut Vec<Notification>,
) -> Result<(), String> {
    for (node, registration) in anim.notifies.iter().enumerate() {
        let Some(registration) = registration else {
            continue;
        };
        let rate = notify_rate(anim, node);
        if rate == 0.0 {
            continue;
        }
        let clip = anim
            .runtime
            .leaf_clip(anim.notify_children[node].unwrap())
            .unwrap();
        if rate * seconds == 0.0 {
            continue;
        }
        let state = anim.runtime.states()[node];
        let source = anim
            .sync
            .group(XAnimNodeId(node as u16))
            .unwrap_or(XAnimNodeId(node as u16));
        let previous = old[source.0 as usize];
        let cycles = usize::from(state.cycle_count.wrapping_sub(previous.cycle_count) as u16);
        if cycles > MAX_NOTIFY_STEPS {
            return Err("animation notification cycle budget exceeded".into());
        }
        let mut emit = |track: &str| -> Result<(), String> {
            if events.len() >= MAX_NOTIFY_STEPS {
                return Err("animation notification budget exceeded".into());
            }
            events.push(Notification {
                owner,
                node,
                registration: registration.clone(),
                track: track.into(),
            });
            Ok(())
        };
        if previous.time == 1.0 {
            emit("end")?;
            continue;
        }
        for cycle in 0..=cycles {
            let start = if cycle == 0 { previous.time } else { 0.0 };
            let wrap = cycle < cycles;
            let end = if wrap { 1.0 } else { state.time };
            let inclusive = wrap || (!clip.looping && end == 1.0);
            let remaining: Vec<_> = clip
                .notifies
                .iter()
                .filter(|track| track.time >= start)
                .collect();
            if remaining.is_empty() {
                if inclusive {
                    emit("end")?;
                }
            } else {
                for track in remaining {
                    if track.time < end || (inclusive && track.time == end) {
                        emit(&track.name)?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn advance(world: &mut World, seconds: f32, now: i64) -> Result<(), String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("animation frame duration must be finite and nonnegative".into());
    }
    for anim in world.resource_mut::<RoundScript>().animators.values_mut() {
        anim.runtime
            .begin_synced_update(seconds, &anim.sync)
            .map_err(|e| e.to_string())?;
    }
    super::scripted::begin_frame(world);
    let mut remaining = seconds;
    let mut iterations = 0;
    while remaining > 0.0 {
        if iterations == MAX_NOTIFY_STEPS {
            return Err("animation notification step budget exceeded".into());
        }
        iterations += 1;
        let runtime = world.resource::<RoundScript>();
        let next = runtime
            .animators
            .iter()
            .filter(|(owner, _)| runtime.can_receive_call(owner))
            .filter_map(|(_, anim)| next_notify(anim))
            .min_by(f32::total_cmp);
        let step = next.map_or(remaining, |next| {
            (next + NOTIFY_STEP_EPSILON).min(remaining)
        });
        let mut events = Vec::new();
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            for (&owner, anim) in &mut runtime.animators {
                let old = anim.runtime.states().to_vec();
                anim.runtime
                    .advance_synced(step, &anim.sync)
                    .map_err(|e| e.to_string())?;
                crossed_notifies(anim, owner, &old, step, &mut events)?;
            }
        }
        super::scripted::apply(world)?;
        remaining -= step;
        for event in events {
            let runtime = world.resource::<RoundScript>();
            let current = runtime.can_receive_call(&event.owner)
                && runtime
                    .animators
                    .get(&event.owner)
                    .and_then(|anim| anim.notifies.get(event.node))
                    .and_then(Option::as_ref)
                    .is_some_and(|registration| Arc::ptr_eq(registration, &event.registration));
            if !current {
                continue;
            }
            crate::script::runtime::raise(
                world,
                Value::Object(event.owner),
                &event.registration,
                vec![Value::String(event.track.into())],
            );
            crate::script::runtime::deliver_external(world, now);
            if world.resource::<RoundScript>().fault.is_some() {
                return Ok(());
            }
        }
    }
    super::scripted::finish_frame(world)?;
    for anim in world.resource_mut::<RoundScript>().animators.values_mut() {
        if let Some(sync) = anim
            .runtime
            .compact_restarts(anim.data.names.len(), &anim.sync)
            .map_err(|e| e.to_string())?
        {
            anim.sync = Arc::new(sync);
        }
    }
    Ok(())
}
