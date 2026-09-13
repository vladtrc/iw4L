use crate::{ConsoleCommand, ConsoleLine, ConsoleSettings, ConsoleState};
use bevy::prelude::*;
use render_frontend::assemble::drawsurf::fog::{FogDvars, MapFrameFog};
use render_frontend::prepare::scene::view_parms::PreparedSceneView;

const READONLY: &[&str] = &[
    "g_fogColorReadOnly",
    "g_fogStartDistReadOnly",
    "g_fogHalfDistReadOnly",
    "g_fogMaxOpacityReadOnly",
    "g_sunFogEnabledReadOnly",
    "g_sunFogColorReadOnly",
    "g_sunFogDirReadOnly",
    "g_sunFogBeginFadeAngleReadOnly",
    "g_sunFogEndFadeAngleReadOnly",
    "g_sunFogScaleReadOnly",
];
const CONTROLS: &[&str] = &[
    "r_fog",
    "r_zfar",
    "setexpfog",
    "scr_art_tweak",
    "scr_fog_disable",
    "scr_fog_nearplane",
    "scr_fog_exp_halfplane",
    "scr_fog_color",
    "scr_fog_max_opacity",
    "scr_sunFogEnabled",
    "scr_sunFogColor",
    "scr_sunFogDir",
    "scr_sunFogBeginFadeAngle",
    "scr_sunFogEndFadeAngle",
    "scr_sunFogScale",
    "scr_cmd_plr_sun",
    "scr_fog_fraction",
];
pub(crate) fn register(registry: &mut crate::ConsoleRegistry) {
    for &name in CONTROLS.iter().chain(READONLY) {
        registry.register(crate::CommandSpec::new(name).usage(match name {
            "setexpfog" => "setexpfog start half R G B maxOpacity seconds [sunR sunG sunB dirX dirY dirZ beginAngle endAngle scale] — local script bridge".to_owned(),
            "scr_art_tweak" => "scr_art_tweak [0|1] — enable local fog art sliders; other art families are not hosted".to_owned(),
            _ if READONLY.contains(&name) => format!("{name} — read-only last setexpfog input"),
            _ => format!("{name} [value(s)] — fog control"),
        }));
    }
}

fn values(args: &[String]) -> Result<Vec<f32>, &'static str> {
    args.iter()
        .map(|v| {
            v.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or("expected finite numeric values")
        })
        .collect()
}
fn scalar(v: &[f32], min: f32, max: f32) -> Result<f32, &'static str> {
    match v {
        [v] if (min..=max).contains(v) => Ok(*v),
        _ => Err("value outside domain or wrong arity"),
    }
}
fn boolean(v: &[f32]) -> Result<bool, &'static str> {
    match v {
        [0.0] => Ok(false),
        [1.0] => Ok(true),
        _ => Err("expected 0 or 1"),
    }
}
fn vector(v: &[f32], min: f32, max: f32) -> Result<[f32; 3], &'static str> {
    match v {
        [x, y, z] if v.iter().all(|v| (min..=max).contains(v)) => Ok([*x, *y, *z]),
        _ => Err("expected three values in domain"),
    }
}

fn execute(
    cmd: &ConsoleCommand,
    dvars: &mut FogDvars,
    map: Option<&mut MapFrameFog>,
    time: i32,
    prepared: &PreparedSceneView,
) -> Result<String, &'static str> {
    let name = cmd.name.as_str();
    if name == "r_fog" || name == "r_zfar" {
        if !cmd.args.is_empty() {
            let v = values(&cmd.args)?;
            if name == "r_fog" {
                dvars.enabled = boolean(&v)?;
            } else {
                dvars.zfar = scalar(&v, 0.0, f32::MAX)?;
            }
        }
        return Ok(format!(
            "{name} = {}",
            if name == "r_fog" {
                u8::from(dvars.enabled) as f32
            } else {
                dvars.zfar
            }
        ));
    }
    let map = map.ok_or("fog unavailable: map has no authored fog")?;
    if READONLY.contains(&name) {
        if !cmd.args.is_empty() {
            return Err("read-only; use setexpfog or scr_art_tweak sliders");
        }
        let f = map.fog;
        let s = map.readonly_sun;
        let value = match name {
            "g_fogColorReadOnly" => format!("{:?}", f.color_rgb),
            "g_fogStartDistReadOnly" => f.start_dist.to_string(),
            "g_fogHalfDistReadOnly" => f.halfway_dist.to_string(),
            "g_fogMaxOpacityReadOnly" => f.max_opacity.to_string(),
            "g_sunFogEnabledReadOnly" => u8::from(f.sun.is_some()).to_string(),
            "g_sunFogColorReadOnly" => format!("{:?}", s.color_rgb),
            "g_sunFogDirReadOnly" => format!("{:?}", s.sun_dir),
            "g_sunFogBeginFadeAngleReadOnly" => s.begin_angle_deg.to_string(),
            "g_sunFogEndFadeAngleReadOnly" => s.end_angle_deg.to_string(),
            "g_sunFogScaleReadOnly" => s.scale.to_string(),
            _ => unreachable!(),
        };
        return Ok(format!("{name} = {value} (read-only)"));
    }
    if name == "scr_fog_fraction" {
        return Err(
            "scr_fog_fraction is initialized to 1 by retail art script but has no fog consumer",
        );
    }
    let v = values(&cmd.args)?;
    if name == "setexpfog" {
        if v.len() != 7 && v.len() != 16 {
            return Err("setexpfog expects 7 values or 16 with flattened sun direction");
        }
        let mut f = map.fog;
        f.start_dist = scalar(&v[0..1], 0.0, f32::MAX)?;
        f.halfway_dist = scalar(&v[1..2], f32::MIN_POSITIVE, f32::MAX)?;
        f.color_rgb = vector(&v[2..5], 0.0, 1.0)?;
        f.max_opacity = scalar(&v[5..6], 0.0, 1.0)?;
        f.transition_time = scalar(&v[6..7], 0.0, i32::MAX as f32 / 1000.0)?;
        f.volumetric = None;
        f.sun = if v.len() == 16 {
            Some(assets::SunFog {
                color_rgb: vector(&v[7..10], 0.0, 1.0)?,
                sun_dir: vector(&v[10..13], -f32::MAX, f32::MAX)?,
                begin_angle_deg: scalar(&v[13..14], 0.0, 180.0)?,
                end_angle_deg: scalar(&v[14..15], v[13], 180.0)?,
                scale: scalar(&v[15..16], 0.0, f32::MAX)?,
            })
        } else {
            None
        };
        map.set(f, time);
        return Ok(format!(
            "setexpfog: local target start={} half={} opacity={} seconds={} sun={}",
            f.start_dist,
            f.halfway_dist,
            f.max_opacity,
            f.transition_time,
            f.sun.is_some()
        ));
    }
    if v.is_empty() {
        let f = map.sliders;
        let s = map.slider_sun;
        let value = match name {
            "scr_art_tweak" => u8::from(map.art_tweak).to_string(),
            "scr_fog_disable" => u8::from(map.script_disabled).to_string(),
            "scr_fog_nearplane" => f.start_dist.to_string(),
            "scr_fog_exp_halfplane" => f.halfway_dist.to_string(),
            "scr_fog_color" => format!("{:?}", f.color_rgb),
            "scr_fog_max_opacity" => f.max_opacity.to_string(),
            "scr_sunFogEnabled" => u8::from(f.sun.is_some()).to_string(),
            "scr_sunFogColor" => format!("{:?}", s.color_rgb),
            "scr_sunFogDir" => format!("{:?}", s.sun_dir),
            "scr_sunFogBeginFadeAngle" => s.begin_angle_deg.to_string(),
            "scr_sunFogEndFadeAngle" => s.end_angle_deg.to_string(),
            "scr_sunFogScale" => s.scale.to_string(),
            "scr_cmd_plr_sun" => "0".into(),
            _ => unreachable!(),
        };
        return Ok(format!("{name} = {value}"));
    }

    let mut next = *map;
    match name {
        "scr_art_tweak" => {
            let enabled = boolean(&v)?;
            if enabled && !next.art_tweak {
                next.sliders = next.fog;
                next.slider_sun = next.readonly_sun;
            }
            next.art_tweak = enabled;
        }
        "scr_fog_disable" => next.script_disabled = boolean(&v)?,
        "scr_fog_nearplane" => next.sliders.start_dist = scalar(&v, 0.0, f32::MAX)?,
        "scr_fog_exp_halfplane" => {
            next.sliders.halfway_dist = scalar(&v, f32::MIN_POSITIVE, f32::MAX)?
        }
        "scr_fog_color" => next.sliders.color_rgb = vector(&v, 0.0, 1.0)?,
        "scr_fog_max_opacity" => next.sliders.max_opacity = scalar(&v, 0.0, 1.0)?,
        "scr_sunFogEnabled" => next.sliders.sun = boolean(&v)?.then_some(next.slider_sun),
        "scr_sunFogColor" => next.slider_sun.color_rgb = vector(&v, 0.0, 1.0)?,
        "scr_sunFogDir" => next.slider_sun.sun_dir = vector(&v, -f32::MAX, f32::MAX)?,
        "scr_sunFogBeginFadeAngle" => {
            next.slider_sun.begin_angle_deg = scalar(&v, 0.0, next.slider_sun.end_angle_deg)?
        }
        "scr_sunFogEndFadeAngle" => {
            next.slider_sun.end_angle_deg = scalar(&v, next.slider_sun.begin_angle_deg, 180.0)?
        }
        "scr_sunFogScale" => next.slider_sun.scale = scalar(&v, 0.0, f32::MAX)?,
        "scr_cmd_plr_sun" => {
            if boolean(&v)? {
                if !prepared.ready {
                    return Err("player view unavailable");
                }
                next.slider_sun.sun_dir =
                    (-prepared.view_from_world.inverse().z_axis.truncate()).to_array();
            }
        }
        _ => unreachable!(),
    }
    if next.art_tweak {
        next.apply_sliders(time);
    }
    *map = next;
    Ok(format!(
        "{name} = {:?} (fog art tweak {})",
        v, map.art_tweak
    ))
}

pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    mut dvars: ResMut<FogDvars>,
    mut map: Option<ResMut<MapFrameFog>>,
    clock: Option<Res<net::CgFrameClock>>,
    prepared: Res<PreparedSceneView>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
) {
    for cmd in commands.read() {
        if !CONTROLS.contains(&cmd.name.as_str()) && !READONLY.contains(&cmd.name.as_str()) {
            continue;
        }
        let msg = execute(
            cmd,
            &mut dvars,
            map.as_deref_mut(),
            clock.as_ref().map(|c| c.time()).unwrap_or(0),
            &prepared,
        )
        .unwrap_or_else(|reason| format!("{}: refused: {reason}", cmd.name));
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, settings.log_capacity);
    }
}
