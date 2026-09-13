use std::collections::HashMap;

use bevy::prelude::Resource;

#[derive(Debug, Clone, PartialEq)]
pub struct CreateFxLoopSound {
    pub soundalias: String,

    pub origin_inches: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateFxOneshot {
    pub fxid: String,

    pub origin_inches: [f32; 3],

    pub angles_deg: [f32; 3],

    pub delay: f32,
}

#[derive(Clone, Debug, Default, Resource)]
pub struct CreateFxOneshotEmitters(pub Vec<CreateFxOneshot>);

pub fn parse_createfx_effect_aliases(source: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in source.lines() {
        let Some((alias, path)) = parse_loadfx_alias_line(line.trim()) else {
            continue;
        };
        if !alias.is_empty() && !path.is_empty() {
            out.insert(alias, path);
        }
    }
    out
}

pub fn apply_createfx_effect_aliases(
    shots: &mut [CreateFxOneshot],
    aliases: &HashMap<String, String>,
) {
    for shot in shots {
        if let Some(path) = aliases.get(&shot.fxid) {
            shot.fxid = path.clone();
        }
    }
}

pub fn parse_createfx_loop_sounds(source: &str) -> Vec<CreateFxLoopSound> {
    let mut out = Vec::new();
    let mut pending_origin: Option<[f32; 3]> = None;
    let mut pending_alias: Option<String> = None;
    let mut in_loop = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains("createLoopSound") {
            flush_loop(&mut out, &mut pending_origin, &mut pending_alias, in_loop);
            in_loop = true;
            pending_origin = None;
            pending_alias = None;
            continue;
        }
        if !in_loop {
            continue;
        }
        if trimmed.contains("createOneshotEffect")
            || trimmed.contains("createExploder")
            || trimmed.contains("createIntervalSound")
        {
            flush_loop(&mut out, &mut pending_origin, &mut pending_alias, in_loop);
            in_loop = false;
            continue;
        }
        if let Some(origin) = parse_origin_line(trimmed) {
            pending_origin = Some(origin);
        }
        if let Some(alias) = parse_soundalias_line(trimmed) {
            pending_alias = Some(alias);
        }
    }
    flush_loop(&mut out, &mut pending_origin, &mut pending_alias, in_loop);
    out
}

pub fn parse_createfx_oneshots(source: &str) -> Vec<CreateFxOneshot> {
    let mut out = Vec::new();
    let mut pending = PendingOneshot::default();
    let mut in_oneshot = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(fxid) = parse_create_oneshot_line(trimmed) {
            flush_oneshot(&mut out, &mut pending, in_oneshot);
            in_oneshot = true;
            pending = PendingOneshot {
                fxid: Some(fxid),
                ..Default::default()
            };
            continue;
        }
        if !in_oneshot {
            continue;
        }
        if trimmed.contains("createLoopSound")
            || trimmed.contains("createExploder")
            || trimmed.contains("createIntervalSound")
            || trimmed.contains("createOneshotEffect")
        {
            flush_oneshot(&mut out, &mut pending, in_oneshot);
            in_oneshot = false;
            if let Some(fxid) = parse_create_oneshot_line(trimmed) {
                in_oneshot = true;
                pending = PendingOneshot {
                    fxid: Some(fxid),
                    ..Default::default()
                };
            }
            continue;
        }
        if let Some(origin) = parse_origin_line(trimmed) {
            pending.origin = Some(origin);
        }
        if let Some(angles) = parse_angles_line(trimmed) {
            pending.angles = Some(angles);
        }
        if let Some(fxid) = parse_string_field(trimmed, "fxid") {
            pending.fxid = Some(fxid);
        }
        if let Some(delay) = parse_delay_line(trimmed) {
            pending.delay = Some(delay);
        }
    }
    flush_oneshot(&mut out, &mut pending, in_oneshot);
    out
}

#[derive(Default)]
struct PendingOneshot {
    fxid: Option<String>,
    origin: Option<[f32; 3]>,
    angles: Option<[f32; 3]>,
    delay: Option<f32>,
}

fn flush_loop(
    out: &mut Vec<CreateFxLoopSound>,
    origin: &mut Option<[f32; 3]>,
    alias: &mut Option<String>,
    in_loop: bool,
) {
    if !in_loop {
        return;
    }
    if let (Some(origin), Some(soundalias)) = (origin.take(), alias.take()) {
        if !soundalias.is_empty() {
            out.push(CreateFxLoopSound {
                soundalias,
                origin_inches: origin,
            });
        }
    } else {
        *origin = None;
        *alias = None;
    }
}

fn flush_oneshot(out: &mut Vec<CreateFxOneshot>, pending: &mut PendingOneshot, in_oneshot: bool) {
    if !in_oneshot {
        return;
    }
    let Some(fxid) = pending.fxid.take() else {
        *pending = PendingOneshot::default();
        return;
    };
    if fxid.is_empty() {
        *pending = PendingOneshot::default();
        return;
    }
    let Some(origin) = pending.origin.take() else {
        *pending = PendingOneshot::default();
        return;
    };
    out.push(CreateFxOneshot {
        fxid,
        origin_inches: origin,
        angles_deg: pending.angles.take().unwrap_or([0.0, 0.0, 0.0]),
        delay: pending.delay.take().unwrap_or(0.0),
    });
    *pending = PendingOneshot::default();
}

fn parse_create_oneshot_line(line: &str) -> Option<String> {
    let key = "createOneshotEffect";
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let open = rest.find('(')?;
    let rest = rest[open + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn parse_loadfx_alias_line(line: &str) -> Option<(String, String)> {
    let key = "level._effect[";
    let idx = line.find(key)?;
    let rest = line[idx + key.len()..].trim_start();
    let alias = parse_quoted(rest)?;
    let after_alias = rest.find(&format!("\"{alias}\""))? + alias.len() + 2;
    let rest = rest[after_alias..].trim_start();
    let loadfx = "loadfx";
    let load_idx = rest.find(loadfx)?;
    let rest = rest[load_idx + loadfx.len()..].trim_start();
    let open = rest.find('(')?;
    let rest = rest[open + 1..].trim_start();
    let path = parse_quoted(rest)?;
    Some((alias, path))
}

fn parse_soundalias_line(line: &str) -> Option<String> {
    parse_string_field(line, "soundalias")
}

fn parse_string_field(line: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let key_pos = line.find(&key)?;
    let rest = &line[key_pos + key.len()..];
    let first = rest.find('"')?;
    let rest = &rest[first + 1..];
    let second = rest.find('"')?;
    Some(rest[..second].to_owned())
}

fn parse_origin_line(line: &str) -> Option<[f32; 3]> {
    parse_vec3_field(line, "origin")
}

fn parse_angles_line(line: &str) -> Option<[f32; 3]> {
    parse_vec3_field(line, "angles")
}

fn parse_vec3_field(line: &str, field: &str) -> Option<[f32; 3]> {
    let key = format!("\"{field}\"");
    let key_pos = line.find(&key)?;
    let rest = &line[key_pos..];
    let open = rest.find('(')?;
    let rest = &rest[open + 1..];
    let close = rest.find(')')?;
    let body = &rest[..close];
    let mut parts = body.split(',').map(|p| p.trim().parse::<f32>().ok());
    let x = parts.next()??;
    let y = parts.next()??;
    let z = parts.next()??;
    Some([x, y, z])
}

fn parse_delay_line(line: &str) -> Option<f32> {
    let key = "\"delay\"";
    let key_pos = line.find(key)?;
    let rest = &line[key_pos + key.len()..];
    let eq = rest.find('=')?;
    let rest = rest[eq + 1..].trim();
    let end = rest
        .find(|c: char| c == ';' || c.is_whitespace())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn parse_quoted(s: &str) -> Option<String> {
    let rest = s.trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}
