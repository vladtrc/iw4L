use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    None,
    Normal,
    Gravity,
    ZOnly,
    NoGravity,
    NoClip,
    PointRelative,
}

impl Mode {
    pub(super) fn uses_root_motion(self) -> bool {
        matches!(
            self,
            Self::Gravity | Self::ZOnly | Self::NoGravity | Self::NoClip
        )
    }
}

pub(super) fn effective(actor: &Actor) -> Mode {
    if actor.script_anim_mode == Mode::None && actor.traversal.active() {
        return actor.traversal.mode();
    }
    match actor.script_anim_mode {
        Mode::None
            if matches!(
                actor.anim_script_kind,
                Some(
                    scripted::ControlKind::Custom
                        | scripted::ControlKind::Death
                        | scripted::ControlKind::CoverArrival
                )
            ) =>
        {
            Mode::Gravity
        }
        Mode::None => Mode::Normal,
        mode => mode,
    }
}

pub(super) fn delta(world: &World, id: u64) -> Result<xmodel_runtime::RootMotionDelta, String> {
    let runtime = world.resource::<RoundScript>();
    let goal_weights = runtime.actors[&id].anim_script_kind == Some(scripted::ControlKind::Custom);
    let animator = runtime
        .animators
        .get(&id)
        .ok_or("actor root motion requires an animation tree")?;
    Ok(animator
        .runtime
        .relative_motion(xmodel_runtime::XAnimNodeId(0), goal_weights)
        .unwrap_or_else(Default::default))
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "animmode", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let mode = match text(args, 0)? {
            "none" => Mode::None,
            "normal" => Mode::Normal,
            "gravity" => Mode::Gravity,
            "zonly_physics" => Mode::ZOnly,
            "nogravity" => Mode::NoGravity,
            "nophysics" => Mode::NoClip,
            "angle deltas" => {
                return Err(format!(
                    "actor animmode '{}' physics is not implemented",
                    text(args, 0)?
                ));
            }
            name => return Err(format!("unknown actor animmode '{name}'")),
        };
        let clear_path = args.len() < 2 || super::super::args::int(args, 1)? != 0;
        let relative = {
            let runtime = world.resource::<RoundScript>();
            if runtime.entities[&id].linked_to.is_some() {
                return Err("linked actor animmode is not implemented".into());
            }
            runtime.actors[&id].script_anim_mode == Mode::PointRelative
        };
        if relative {
            super::super::animations::stop_actor_motion(world, id);
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime.actors.get_mut(&id).unwrap();
        if mode != Mode::None && clear_path {
            actor.navigation = Default::default();
        }
        actor.script_anim_mode = mode;
        Ok(Value::Undefined)
    });
}
