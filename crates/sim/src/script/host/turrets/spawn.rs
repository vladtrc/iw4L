use super::*;

pub(crate) fn initialize_map(world: &mut World) -> Result<(), String> {
    let objects: Vec<_> = world
        .resource::<RoundScript>()
        .entities
        .iter()
        .filter(|(_, entity)| {
            entity.kind == EntityKind::Map
                && matches!(&*entity.classname, "misc_turret" | "misc_mg42")
        })
        .map(|(&object, _)| object)
        .collect();
    for object in objects {
        install(world, object)?;
    }
    Ok(())
}

pub(super) fn dynamic(world: &mut World, _: &Value, args: &[Value]) -> Result<Value, String> {
    let classname = string(args, 0)?;
    let origin = vector(args, 1)?;
    let weaponinfo = string(args, 2)?;
    let id = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let id = runtime.create_entity(EntityKind::Spawned, &classname)?;
        runtime.set_object_field(id, "origin", Value::Vector(origin));
        runtime.set_object_field(id, "angles", Value::Vector([0.; 3]));
        runtime.set_object_field(id, "weaponinfo", Value::string(&weaponinfo));
        id
    };
    if let Err(error) = install(world, id) {
        world.resource_mut::<RoundScript>().delete_entity(id);
        return Err(error);
    }
    Ok(Value::Object(id))
}

fn install(world: &mut World, object: u64) -> Result<(), String> {
    if world.resource::<RoundScript>().engine.turrets.len() >= MAX_TURRETS {
        return Err("turret pool exhausted (32)".into());
    }
    let (name, origin, angles, model) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::String(name) = runtime.object_field(object, "weaponinfo") else {
            return Err("map turret requires weaponinfo".into());
        };
        let Value::Vector(origin) = runtime.object_field(object, "origin") else {
            return Err("turret requires an origin".into());
        };
        let angles = match runtime.object_field(object, "angles") {
            Value::Vector(angles) => angles,
            _ => [0.; 3],
        };
        let model = match runtime.object_field(object, "model") {
            Value::String(name) if !name.is_empty() => Some(name.to_string()),
            _ => None,
        };
        (name.to_string(), origin, angles, model)
    };
    if !origin.into_iter().chain(angles).all(f32::is_finite) {
        return Err("turret pose must be finite".into());
    }
    let (weapon, settings, model) = {
        let frame = FrameWorld::from_world(world);
        let weapon = crate::script_player::weapon_named(&frame, &name)?;
        if frame
            .combat_facts_for(weapon)
            .is_none_or(|facts| facts.weap_class != weapon_iw4::WEAPCLASS_TURRET)
        {
            return Err(format!("turret weapon is not a turret: {name}"));
        }
        let settings = frame
            .content()
            .turret_settings(weapon)
            .ok_or_else(|| format!("turret weapon has no authored aiming defaults: {name}"))?;
        let model = model.or_else(|| {
            frame
                .weapon_world_model(weapon)
                .map(|(model, _)| model.to_owned())
                .filter(|model| !model.is_empty())
        });
        (weapon, settings, model)
    };
    let mut turret = Turret::new(weapon, settings);
    let mut runtime = world.resource_mut::<RoundScript>();
    let mut scalar = |name: &str| {
        match runtime.object_field(object, name) {
            Value::Float(value) => Some(value),
            Value::Int(value) => Some(value as f32),
            Value::String(value) => value.parse::<f32>().ok(),
            _ => None,
        }
        .filter(|value| value.is_finite())
    };
    for (index, name) in ["leftarc", "rightarc", "toparc", "bottomarc"]
        .into_iter()
        .enumerate()
    {
        if let Some(value) = scalar(name) {
            turret.arcs[index] = value.clamp(0., 180.);
        }
    }
    for (index, name) in ["pitchconvergencetime", "yawconvergencetime"]
        .into_iter()
        .enumerate()
    {
        if let Some(value) =
            scalar(name).or_else(|| (index == 1).then(|| scalar("convergencetime")).flatten())
        {
            turret.convergence[index] = value.max(0.);
        }
    }
    if let Some(value) = scalar("suppressiontime") {
        turret.suppression_seconds = value.max(0.);
    }
    if let Some(value) = scalar("aispread") {
        turret.ai_spread_degrees = value.max(0.);
    }
    if let Some(value) = scalar("playerspread") {
        turret.player_spread_degrees = value.max(0.);
    }
    let presence = super::super::presence::spawn_presence(world, origin)?;
    let number = FrameWorld::from_world(world)
        .gentity_number(presence)
        .ok_or("turret presence is missing")?;
    let mut frame = FrameWorld::from_world(world);
    let hints = frame
        .content()
        .turret_hints(weapon)
        .cloned()
        .unwrap_or([None, None]);
    if let Some(mover) = frame.script_mover_mut_by_number(number) {
        mover.state.index = weapon as i32;
    }
    let [use_hint, drop_hint] = hints.map(|key| {
        key.and_then(|key| {
            let key = key.trim_start_matches('@');
            if name == "turret_minigun_mp"
                && matches!(key, "PLATFORM_HOLD_TO_USE" | "PLATFORM_HOLD_TO_DROP")
            {
                frame.hud_string_index(&format!(
                    "{key}{}MP_MINIGUN",
                    crate::HUD_PRINT_ARG_SEPARATOR
                ))
            } else {
                frame.hud_string_index(key)
            }
        })
        .unwrap_or(-1)
    });
    turret.drop_hint = drop_hint;
    let birthtime = world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |request| crate::level_time_ms(request.tick));
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.set_object_field(object, "birthtime", Value::Int(birthtime));
    if matches!(
        runtime.object_field(object, "health"),
        Value::Undefined | Value::Int(0) | Value::Float(0.)
    ) {
        runtime.set_object_field(object, "health", Value::Int(100));
    }
    runtime.set_object_field(object, "code_classname", Value::string("misc_turret"));
    if let Some(model) = model {
        runtime.set_object_field(object, "model", Value::string(&model));
    }
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = Some(presence);
    entity.number = number;
    if entity.kind == EntityKind::Map {
        entity.usable = Some(super::super::entities::Usable {
            enabled: true,
            cursor: 1,
            hint: use_hint,
            ..Default::default()
        });
    }
    entity.contents = 0x20_0000;
    entity.solid = true;
    entity.can_damage = true;
    entity.link_enabled = true;
    runtime.engine.turrets.insert(object, turret);
    set_body(
        world,
        object,
        crate::EntityBodyBounds {
            midpoint: [0., 0., 28.],
            half_size: [32., 32., 28.],
            contents: 0x20_0000,
        },
    )?;
    diag::info!(
        Sim,
        "turret installed: object={object} weapon={name} presence={} origin={origin:?}",
        presence.to_wire()
    );
    Ok(())
}

pub(super) fn set_body(
    world: &mut World,
    object: u64,
    bounds: crate::EntityBodyBounds,
) -> Result<(), String> {
    let (presence, number, origin) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let origin = match runtime.object_field(object, "origin") {
            Value::Vector(value) => value,
            _ => return Err("turret origin is not a vector".into()),
        };
        let entity = runtime
            .entities
            .get_mut(&object)
            .ok_or("turret entity is missing")?;
        let presence = entity.presence.ok_or("turret model presence is missing")?;
        entity.solid = true;
        entity.contents = bounds.contents as i32;
        (presence, entity.number, origin)
    };
    let mut frame = FrameWorld::from_world(world);
    let mover = frame
        .script_mover_mut_by_number(number)
        .ok_or("turret mover is missing")?;
    mover.body = Some(bounds);
    mover.box_mid = bounds.midpoint;
    mover.box_half = bounds.half_size;
    let row = frame
        .collision_owner_mut(presence)
        .ok_or("turret collision row is missing")?;
    row.body = Some(crate::entity_body::EntityBodyPose {
        bounds,
        origin,
        number: number as u16,
    });
    row.solid = true;
    Ok(())
}
