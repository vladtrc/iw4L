use super::*;

fn relative_delta(
    world: &mut World,
    args: &[Value],
) -> Result<xmodel_runtime::RootMotionDelta, String> {
    let end = if args.len() == 1 || args.len() == 2 {
        1.0
    } else {
        float(args, 2)?
    };
    if !(0.0..=1.0).contains(&end) {
        return Err("end time must be between zero and one".into());
    }
    let start = if args.len() == 1 {
        0.0
    } else {
        float(args, 1)?
    };
    if !(0.0..=1.0).contains(&start) {
        return Err("start time must be between zero and one".into());
    }
    Ok(
        super::super::natives::player::animation_clip_or_branch(world, args, 0)?
            .map_or_else(Default::default, |clip| clip.relative_delta(start, end)),
    )
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "getmovedelta", |world, _, args| {
        Ok(Value::Vector(relative_delta(world, args)?.translation))
    });
    registry.register(Namespace::Function, "getangledelta", |world, _, args| {
        Ok(Value::Float(relative_delta(world, args)?.yaw()))
    });
}
