use super::*;

pub(super) fn active(world: &World) -> bool {
    world.resource::<RoundScript>().profile == asset_core::RuntimeProfile::Campaign
}

fn require_campaign(world: &World) -> Result<(), String> {
    if active(world) {
        Ok(())
    } else {
        Err("SP objective native requires a campaign runtime".into())
    }
}

fn printable(text: &str, max: usize, controls: bool) -> Result<(), String> {
    if text.len() > max
        || text.bytes().any(|b| {
            !(0x20..=0x7e).contains(&b) && !(controls && matches!(b, 0x14 | 0x15))
                || matches!(b, b'\\' | b'"' | b';')
        })
    {
        return Err(format!("invalid objective string (maximum {max} bytes)"));
    }
    Ok(())
}

fn localized(key: &str) -> Result<(), String> {
    if key.len() > 1 && !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(
            "localized objective string must contain letters, numbers or underscores".into(),
        );
    }
    Ok(())
}

fn text_value(value: &Value) -> Result<String, String> {
    match value {
        Value::LocalizedString(key) => {
            localized(key)?;
            Ok(key.to_string())
        }
        Value::String(text) => {
            if !text.as_bytes().is_ascii() {
                return Err("objective string must be ASCII".into());
            }
            Ok(text.to_string())
        }
        Value::Int(_) | Value::Float(_) => crate::script::runtime::to_text(value)
            .ok_or_else(|| "objective string requires a string or number".into()),
        _ => Err("objective string requires a string or number".into()),
    }
}

fn construct(values: &[Value], max: usize) -> Result<String, String> {
    let mut result = String::new();
    for value in values {
        let text = text_value(value)?;
        if result.len() + text.len() + 1 > max {
            return Err(format!("objective message exceeds {max} bytes"));
        }
        if matches!(value, Value::LocalizedString(_)) {
            if !result.is_empty() {
                result.push('\x14');
            }
        } else if !text.is_empty() {
            result.push('\x15');
        }
        result.push_str(&text);
    }
    printable(&result, max, true)?;
    Ok(result)
}

fn position_value(args: &[Value], at: usize) -> Result<[f32; 3], String> {
    let value = vector(args, at)?;
    if value.iter().any(|v| {
        !v.is_finite() || f64::from(*v) < f64::from(i32::MIN) || f64::from(*v) > f64::from(i32::MAX)
    }) {
        return Err("objective coordinates must fit integer configstring coordinates".into());
    }
    Ok(value.map(|v| (v as i32) as f32))
}

fn icon_value(args: &[Value], at: usize) -> Result<String, String> {
    let icon = string(args, at)?;
    if icon.len() > 63 || !icon.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err("objective icon must contain at most 63 printable ASCII bytes".into());
    }
    Ok(icon)
}

fn update(world: &mut World, state: ObjectiveState, text: String) {
    let text = match state {
        ObjectiveState::Active | ObjectiveState::Current => text,
        ObjectiveState::Done => "GAME_OBJECTIVECOMPLETED".into(),
        ObjectiveState::Failed => "GAME_OBJECTIVEFAILED".into(),
        _ => return,
    };
    FrameWorld::from_world(world).push_objective_update(crate::ObjectiveUpdate { state, text });
}

pub(super) fn add(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let index = index(args)?;
    let state = state(args, 1)?;
    let text = args.get(2).map(text_value).transpose()?;
    if let Some(text) = &text {
        printable(text, 1023, false)?;
    }
    let position = (args.len() > 3)
        .then(|| position_value(args, 3))
        .transpose()?;
    let icon = (args.len() > 4).then(|| icon_value(args, 4)).transpose()?;
    {
        let mut row = objective(world, index);
        row.state = state;
        if let Some(text) = &text {
            row.campaign.text.clone_from(text);
        }
        if let Some(position) = position {
            row.campaign.positions[0] = position;
        }
        if let Some(icon) = icon {
            row.campaign.icon = icon;
        }
        row.campaign.pointer_text.clear();
    }
    update(
        world,
        state,
        text.unwrap_or_else(|| "GAME_OBJECTIVESUPDATED".into()),
    );
    Ok(Value::Undefined)
}

pub(super) fn set_state(world: &mut World, args: &[Value], message: bool) -> Result<Value, String> {
    let index = index(args)?;
    let state = state(args, 1)?;
    objective(world, index).state = state;
    if message {
        update(world, state, "GAME_OBJECTIVESUPDATED".into());
    }
    Ok(Value::Undefined)
}

pub(super) fn icon(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let index = index(args)?;
    let icon = icon_value(args, 1)?;
    objective(world, index).campaign.icon = icon;
    Ok(Value::Undefined)
}

pub(super) fn position(
    world: &mut World,
    args: &[Value],
    additional: bool,
) -> Result<Value, String> {
    let index = index(args)?;
    let slot = if additional { int(args, 1)? } else { 0 };
    if !(0..8).contains(&slot) {
        return Err("illegal objective position index".into());
    }
    let position = position_value(args, if additional { 2 } else { 1 })?;
    let mut row = objective(world, index);
    row.campaign.positions[slot as usize] = position;
    if !additional {
        row.entity = None;
        row.campaign.offset = [0.; 3];
    }
    Ok(Value::Undefined)
}

pub(super) fn on_entity(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let index = index(args)?;
    let target = arg(args, 1)?.clone();
    super::super::natives::engine::entity_id(world, &target)?;
    let offset = if args.len() > 2 {
        vector(args, 2)?
    } else {
        [0.; 3]
    };
    if !offset.iter().all(|v| v.is_finite()) {
        return Err("objective offset must be finite".into());
    }
    let offset = offset.map(|v| format!("{v:.2}").parse().unwrap_or(0.));
    let mut row = objective(world, index);
    row.entity = Some(target);
    row.campaign.offset = offset;
    Ok(Value::Undefined)
}

fn set_string(world: &mut World, args: &[Value], message: bool) -> Result<Value, String> {
    require_campaign(world)?;
    let index = index(args)?;
    let text = construct(&args[1..], 1023)?;
    let state = {
        let mut row = objective(world, index);
        row.campaign.text.clone_from(&text);
        row.state
    };
    if message && state != ObjectiveState::Done {
        update(world, state, text);
    }
    Ok(Value::Undefined)
}

fn current(
    world: &mut World,
    args: &[Value],
    message: bool,
    additional: bool,
) -> Result<Value, String> {
    require_campaign(world)?;
    let indices: Result<Vec<_>, _> = args
        .iter()
        .map(|arg| index(std::slice::from_ref(arg)))
        .collect();
    let indices = indices?;
    for index in 0..MAX_OBJECTIVES as u8 {
        let selected = indices.contains(&index);
        if selected {
            objective(world, index).state = ObjectiveState::Current;
        } else if !additional
            && let Some(row) = world
                .resource_mut::<RoundScript>()
                .engine
                .objectives
                .get_mut(&index)
            && row.state == ObjectiveState::Current
        {
            row.state = ObjectiveState::Active;
        }
    }
    if message {
        update(
            world,
            ObjectiveState::Current,
            "GAME_OBJECTIVESUPDATED".into(),
        );
    }
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Function,
        "objective_state_nomessage",
        |world, _, args| {
            require_campaign(world)?;
            set_state(world, args, false)
        },
    );
    registry.register(Namespace::Function, "objective_string", |world, _, args| {
        set_string(world, args, true)
    });
    registry.register(
        Namespace::Function,
        "objective_string_nomessage",
        |world, _, args| set_string(world, args, false),
    );
    registry.register(
        Namespace::Function,
        "objective_current",
        |world, _, args| current(world, args, true, false),
    );
    registry.register(
        Namespace::Function,
        "objective_current_nomessage",
        |world, _, args| current(world, args, false, false),
    );
    registry.register(
        Namespace::Function,
        "objective_additionalcurrent",
        |world, _, args| current(world, args, true, true),
    );
    registry.register(
        Namespace::Function,
        "objective_additionalposition",
        |world, _, args| {
            require_campaign(world)?;
            position(world, args, true)
        },
    );
    registry.register(Namespace::Function, "objective_ring", |world, _, args| {
        require_campaign(world)?;
        let index = index(args)?;
        let mut row = objective(world, index);
        row.campaign.ring = !row.campaign.ring;
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Function,
        "objective_setpointertextoverride",
        |world, _, args| {
            require_campaign(world)?;
            let index = index(args)?;
            let text = construct(&args[1..], 63)?;
            objective(world, index).campaign.pointer_text = text;
            Ok(Value::Undefined)
        },
    );
}
