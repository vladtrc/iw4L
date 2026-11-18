use super::*;

struct Registration {
    name: &'static str,
    default: &'static str,
    flags: u16,
    domain: Domain,
}

enum Domain {
    Bool,
    Float(f32, f32),
    Int(i32, i32),
    Vec3,
    Color,
    String,
}

const fn event_distance(name: &'static str, default: &'static str) -> Registration {
    Registration {
        name,
        default,
        flags: 0xc4,
        domain: Domain::Float(0.0, f32::MAX),
    }
}

const REGISTRATIONS: &[Registration] = &[
    Registration {
        name: "player_sprintunlimited",
        default: "0",
        flags: 0xc4,
        domain: Domain::Bool,
    },
    Registration {
        name: "ui_hidemap",
        default: "0",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "hud_drawhud",
        default: "1",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "cg_drawhud",
        default: "1",
        flags: 0,
        domain: Domain::Bool,
    },
    Registration {
        name: "hud_showstance",
        default: "1",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "hud_gasmaskoverlay",
        default: "0",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "hud_fade_stance",
        default: "1.7",
        flags: 0x44,
        domain: Domain::Float(0., 30.),
    },
    Registration {
        name: "cg_hudstanceflash",
        default: "1 1 1 1",
        flags: 0,
        domain: Domain::Color,
    },
    Registration {
        name: "ai_corpsecount",
        default: "16",
        flags: 0x80,
        domain: Domain::Int(0, 16),
    },
    event_distance("ai_foliageseethroughdist", "128"),
    Registration {
        name: "hud_missionfailed",
        default: "0",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "cg_cinematicfullscreen",
        default: "1",
        flags: 0x40,
        domain: Domain::Bool,
    },
    event_distance("ai_eventdistfootstep", "256"),
    event_distance("ai_eventdistfootstepwalk", "128"),
    event_distance("ai_eventdistfootstepsprint", "400"),
    event_distance("ai_eventdistnewenemy", "1024"),
    event_distance("ai_eventdistpain", "512"),
    event_distance("ai_eventdistdeath", "1024"),
    event_distance("ai_eventdistexplosion", "1024"),
    event_distance("ai_eventdistgrenadeping", "512"),
    event_distance("ai_eventdistprojping", "128"),
    event_distance("ai_eventdistgunshot", "2048"),
    event_distance("ai_eventdistgunshotteam", "2048"),
    event_distance("ai_eventdistsilencedshot", "128"),
    event_distance("ai_eventdistprojimpact", "256"),
    event_distance("ai_eventdistbadplace", "256"),
    event_distance("ai_busyeventdistfootstep", "256"),
    event_distance("ai_busyeventdistfootstepwalk", "128"),
    event_distance("ai_busyeventdistfootstepsprint", "256"),
    event_distance("ai_busyeventdistnewenemy", "256"),
    event_distance("ai_busyeventdistpain", "256"),
    event_distance("ai_busyeventdistdeath", "256"),
    event_distance("ai_busyeventdistexplosion", "1024"),
    event_distance("ai_busyeventdistgrenadeping", "256"),
    event_distance("ai_busyeventdistprojping", "128"),
    event_distance("ai_busyeventdistgunshot", "400"),
    event_distance("ai_busyeventdistgunshotteam", "256"),
    event_distance("ai_busyeventdistsilencedshot", "128"),
    event_distance("ai_busyeventdistbullet", "128"),
    event_distance("ai_busyeventdistprojimpact", "256"),
    Registration {
        name: "ai_accuracydistscale",
        default: "1",
        flags: 0xc4,
        domain: Domain::Float(f32::MIN_POSITIVE, f32::MAX),
    },
    Registration {
        name: "ufohitstriggers",
        default: "0",
        flags: 0xc4,
        domain: Domain::Bool,
    },
    Registration {
        name: "ui_campaign",
        default: "american",
        flags: 0x40,
        domain: Domain::String,
    },
    Registration {
        name: "painvisionlerpinrate",
        default: "10",
        flags: 4,
        domain: Domain::Float(0., 30.),
    },
    Registration {
        name: "painvisionlerpoutrate",
        default: "0.3",
        flags: 4,
        domain: Domain::Float(0., 30.),
    },
    Registration {
        name: "player_dmgtimer_timeperpoint",
        default: "100",
        flags: 0xc,
        domain: Domain::Float(0., f32::MAX),
    },
    Registration {
        name: "player_dmgtimer_maxtime",
        default: "750",
        flags: 0xc,
        domain: Domain::Float(0., f32::MAX),
    },
    Registration {
        name: "player_damagemultiplier",
        default: "1",
        flags: 0xc4,
        domain: Domain::Float(0., 1000.),
    },
    Registration {
        name: "player_radiusdamagemultiplier",
        default: "1",
        flags: 0xc4,
        domain: Domain::Float(0., 1000.),
    },
    Registration {
        name: "player_meleedamagemultiplier",
        default: "1",
        flags: 0xc4,
        domain: Domain::Float(0., 1000.),
    },
    Registration {
        name: "g_friendlyfiredamagescale",
        default: "1",
        flags: 0xc4,
        domain: Domain::Float(0., 1000.),
    },
    Registration {
        name: "g_knockback",
        default: "1000",
        flags: 0x80,
        domain: Domain::Float(-f32::MAX, f32::MAX),
    },
    Registration {
        name: "player_deathinvulnerabletomelee",
        default: "0",
        flags: 0xc4,
        domain: Domain::Bool,
    },
    Registration {
        name: "player_deathinvulnerabletoprojectile",
        default: "1",
        flags: 0xc4,
        domain: Domain::Bool,
    },
    Registration {
        name: "g_deathdelay",
        default: "3000",
        flags: 0x41,
        domain: Domain::Int(0, i32::MAX),
    },
    Registration {
        name: "g_reloading",
        default: "0",
        flags: 0x2000,
        domain: Domain::Int(0, 4),
    },
    Registration {
        name: "con_typewritercolorbase",
        default: "1 1 1",
        flags: 0x40,
        domain: Domain::Vec3,
    },
    Registration {
        name: "compass",
        default: "1",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "ammocounterhide",
        default: "0",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "objectivealpha",
        default: "0.6",
        flags: 0x44,
        domain: Domain::Float(0., f32::MAX),
    },
    Registration {
        name: "snd_slavefadetime",
        default: "500",
        flags: 4,
        domain: Domain::Int(0, 5000),
    },
    Registration {
        name: "ai_friendlysuppression",
        default: "1",
        flags: 0xc4,
        domain: Domain::Bool,
    },
    Registration {
        name: "ai_friendlyfireblockduration",
        default: "2000",
        flags: 0xc4,
        domain: Domain::Int(0, i32::MAX),
    },
    Registration {
        name: "ai_eventdistbullet",
        default: "128",
        flags: 0xc4,
        domain: Domain::Float(0., f32::MAX),
    },
    Registration {
        name: "g_friendlynamedist",
        default: "15000",
        flags: 0xc4,
        domain: Domain::Float(0.0, 15000.0),
    },
    Registration {
        name: "g_gameskill_player_1",
        default: "1",
        flags: 2,
        domain: Domain::Int(0, 3),
    },
    Registration {
        name: "g_gameskill_player_2",
        default: "1",
        flags: 2,
        domain: Domain::Int(0, 3),
    },
    Registration {
        name: "scr_dof_enable",
        default: "1",
        flags: 0xc4,
        domain: Domain::Bool,
    },
    Registration {
        name: "cg_fov",
        default: "65",
        flags: 0x40,
        domain: Domain::Float(1.0, 160.0),
    },
    Registration {
        name: "cg_fovscale",
        default: "1",
        flags: 0x40,
        domain: Domain::Float(0.2, 2.0),
    },
    Registration {
        name: "g_speed",
        default: "190",
        flags: 0xc0,
        domain: Domain::Int(i32::MIN, i32::MAX),
    },
    Registration {
        name: "sv_saveonstartmap",
        default: "0",
        flags: 0x40,
        domain: Domain::Bool,
    },
    Registration {
        name: "r_lightgridenabletweaks",
        default: "0",
        flags: 0x48,
        domain: Domain::Bool,
    },
    Registration {
        name: "r_lightgridintensity",
        default: "1",
        flags: 0x48,
        domain: Domain::Float(0., 2.),
    },
    Registration {
        name: "r_lightgridcontrast",
        default: "0",
        flags: 0x48,
        domain: Domain::Float(-1., 1.),
    },
    Registration {
        name: "r_lightgridusetweakedvalues",
        default: "1",
        flags: 0,
        domain: Domain::Bool,
    },
    Registration {
        name: "r_lightcachelessfrequentperiod",
        default: "10",
        flags: 0,
        domain: Domain::Int(1, 100),
    },
    Registration {
        name: "r_lightcachelessfrequentmaxdistance",
        default: "64",
        flags: 0,
        domain: Domain::Float(0., 10000.),
    },
    Registration {
        name: "r_herolighting",
        default: "1",
        flags: 0,
        domain: Domain::Bool,
    },
];

pub(crate) fn install(world: &mut World) {
    let mut runtime = world.resource_mut::<crate::script::MatchScript>();
    for registration in REGISTRATIONS {
        let pending = runtime.latched_dvars.remove(registration.name);
        let value = pending
            .as_ref()
            .or_else(|| runtime.dvars.get(registration.name))
            .and_then(|value| normalize(registration.name, value))
            .unwrap_or_else(|| registration.default.to_owned());
        runtime.dvars.insert(registration.name.into(), value);
    }
}

pub(super) fn is_latched(name: &str) -> bool {
    REGISTRATIONS
        .iter()
        .any(|row| row.name == name && row.flags & 2 != 0)
}

pub(crate) fn is_saved(name: &str) -> bool {
    REGISTRATIONS
        .iter()
        .any(|row| row.name == name && row.flags & 0x40 != 0)
}

pub(super) fn normalize(name: &str, value: &str) -> Option<String> {
    let Some(registration) = REGISTRATIONS.iter().find(|row| row.name == name) else {
        return Some(value.to_owned());
    };
    match registration.domain {
        Domain::String => Some(value.to_owned()),
        Domain::Vec3 | Domain::Color => {
            let count = if matches!(registration.domain, Domain::Color) {
                4
            } else {
                3
            };
            let values: Vec<_> = value.split_whitespace().map(atof).collect();
            (values.len() == count
                && values
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
            .then(|| {
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        }
        Domain::Bool => Some(i32::from(atoi(value) != 0).to_string()),
        Domain::Int(min, max) => {
            let value = atoi(value);
            (min..=max).contains(&value).then(|| value.to_string())
        }
        Domain::Float(min, max) => {
            let value = atof(value) as f32;
            (value.is_finite() && (min..=max).contains(&value)).then(|| value.to_string())
        }
    }
}

pub(super) fn set(world: &mut World, args: &[Value]) -> Result<Value, String> {
    let name = dvar_name(args)?;
    let registration = REGISTRATIONS
        .iter()
        .find(|row| row.name == name)
        .filter(|_| {
            world
                .resource::<crate::script::MatchScript>()
                .dvars
                .contains_key(&name)
        })
        .ok_or_else(|| format!("SetSavedDvar(): The dvar \"{name}\" does not exist"))?;
    if registration.flags & 0x40 == 0 {
        return Err("SetSavedDvar can only be called on a dvar with the savegame flag".into());
    }
    let value = dvar_value(args)?.replace('"', "'");
    set_script_dvar(world, &name, &value);
    Ok(Value::Undefined)
}

pub(crate) fn movement_speed(world: &World) -> Option<i32> {
    if world.resource::<RoundScript>().profile != asset_core::RuntimeProfile::Campaign {
        return None;
    }
    world
        .resource::<crate::script::MatchScript>()
        .dvars
        .get("g_speed")?
        .parse()
        .ok()
}

pub(crate) fn sprint_unlimited(world: &World) -> bool {
    world.resource::<RoundScript>().profile == asset_core::RuntimeProfile::Campaign
        && world
            .resource::<crate::script::MatchScript>()
            .dvars
            .get("player_sprintunlimited")
            .is_some_and(|value| value == "1")
}
