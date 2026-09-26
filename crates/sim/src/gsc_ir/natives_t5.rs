use super::entities::EntityKind;
use super::iw4_natives::{atof, string};
use super::natives_math::{arg, array_values, float, int, kind, new_array, optional, vector};
use super::*;
use crate::frame::FrameWorld;
use crate::script_player;
use crate::world::ClientId;
use bevy_ecs::prelude::World;
use natives::Namespace::{Function, Method};
use weapon_iw4::WeaponState;

#[derive(Clone, Debug, Default)]
pub(crate) struct T5State {
    influencers: BTreeMap<i32, Influencer>,
    next_influencer: i32,
    spawn_points: BTreeMap<Arc<str>, Vec<Value>>,
    randomness: f32,
    base_weights: Vec<(i32, [f32; 3], f32, f32)>,
    client_flags: BTreeMap<u64, u32>,
    dstats: BTreeMap<(u32, String), Value>,
    statuses: std::collections::BTreeSet<(u32, &'static str)>,
    client_systems: Vec<Arc<str>>,
    spawn_ids: BTreeMap<u32, i32>,
}

#[derive(Clone, Debug)]
struct Influencer {
    shape: Shape,
    origin: [f32; 3],
    score: f32,
    team_mask: i32,
    curve: i32,
    expires_ms: Option<i64>,
    owner: Option<u64>,
    enabled: bool,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Sphere {
        radius: f32,
    },
    Cylinder {
        forward: [f32; 3],
        radius: f32,
        length: f32,
    },
}

const CURVE_LINEAR: i32 = 1;
const CURVE_STEEP: i32 = 2;
const CURVE_INVERSE_LINEAR: i32 = 3;
const CURVE_NEGATIVE_TO_POSITIVE: i32 = 4;

const TEAM_MASK_FREE: i32 = 1;
const TEAM_MASK_AXIS: i32 = 2;
const TEAM_MASK_ALLIES: i32 = 4;

const STATS_TABLE: &str = "mp/statstable.csv";
const STATS_REFERENCE: usize = 4;
const STATS_CLASSES: usize = 11;
const STATS_SLOT: usize = 13;

fn t5(world: &mut World) -> &mut T5State {
    &mut world.resource_mut::<Runtime>().into_inner().t5
}

fn now_ms(world: &World) -> i64 {
    super::players::now_ms(world)
}

fn team_mask(team: &str) -> i32 {
    match team {
        "axis" => TEAM_MASK_AXIS,
        "allies" => TEAM_MASK_ALLIES,
        _ => TEAM_MASK_FREE,
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn axes(angles: [f32; 3]) -> [[f32; 3]; 3] {
    let (sp, cp) = angles[0].to_radians().sin_cos();
    let (sy, cy) = angles[1].to_radians().sin_cos();
    let (sr, cr) = angles[2].to_radians().sin_cos();
    [
        [cp * cy, cp * sy, -sp],
        [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp],
        [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp],
    ]
}

fn curve_weight(curve: i32, fraction: f32) -> f32 {
    let fraction = fraction.clamp(0.0, 1.0);
    match curve {
        CURVE_LINEAR => 1.0 - fraction,
        CURVE_STEEP => (1.0 - fraction) * (1.0 - fraction),
        CURVE_INVERSE_LINEAR => fraction,
        CURVE_NEGATIVE_TO_POSITIVE => 2.0 * fraction - 1.0,
        _ => 1.0,
    }
}

impl Influencer {
    fn score_at(&self, point: [f32; 3]) -> Option<f32> {
        let offset = sub(point, self.origin);
        let fraction = match self.shape {
            Shape::Sphere { radius } => {
                let distance = length(offset);
                (distance <= radius).then(|| distance / radius.max(1.0))?
            }
            Shape::Cylinder {
                forward,
                radius,
                length: reach,
            } => {
                let along = dot(offset, forward);
                if !(0.0..=reach).contains(&along) {
                    return None;
                }
                let across = length(sub(offset, forward.map(|f| f * along)));
                (across <= radius).then(|| along / reach.max(1.0))?
            }
        };
        Some(self.score * curve_weight(self.curve, fraction))
    }
}

fn origin_of(world: &mut World, value: &Value) -> [f32; 3] {
    vector_field(world, value, "origin")
}

fn vector_field(world: &mut World, value: &Value, name: &str) -> [f32; 3] {
    match super::players::entity_field(world, object_of(value).unwrap_or(0), name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

fn object_of(value: &Value) -> Option<u64> {
    match value {
        Value::Object(id) => Some(*id),
        _ => None,
    }
}

fn add_influencer(world: &mut World, influencer: Influencer) -> Value {
    let state = t5(world);
    let id = state.next_influencer;
    state.next_influencer += 1;
    state.influencers.insert(id, influencer);
    Value::Int(id)
}

fn timeout(world: &World, args: &[Value], index: usize) -> Result<Option<i64>, String> {
    let seconds = optional(args, index, float)?.unwrap_or(0.0);
    Ok((seconds > 0.0).then(|| now_ms(world) + (seconds * 1000.0) as i64))
}

fn settle_influencers(world: &mut World) {
    let now = now_ms(world);
    let owners: Vec<(i32, u64)> = world
        .resource::<Runtime>()
        .t5
        .influencers
        .iter()
        .filter_map(|(id, i)| i.owner.map(|o| (*id, o)))
        .collect();
    for (id, owner) in owners {
        let live = world.resource::<Runtime>().objects.contains_key(&owner);
        let origin = live.then(|| origin_of(world, &Value::Object(owner)));
        let state = t5(world);
        match origin {
            Some(origin) => state.influencers.get_mut(&id).unwrap().origin = origin,
            None => {
                state.influencers.remove(&id);
            }
        }
    }
    t5(world)
        .influencers
        .retain(|_, i| i.expires_ms.is_none_or(|at| at > now));
}

fn random_unit(world: &mut World) -> f32 {
    (super::natives_math::random(world) % 10_000) as f32 / 10_000.0
}

fn sorted_spawn_points(
    world: &mut World,
    point_team: &str,
    influencer_team: &str,
    player: Option<u64>,
) -> Result<Value, String> {
    settle_influencers(world);
    let points = t5(world)
        .spawn_points
        .get(point_team)
        .cloned()
        .unwrap_or_default();
    let mask = team_mask(influencer_team);
    let mut scored = Vec::with_capacity(points.len());
    for point in points {
        let origin = origin_of(world, &point);
        let yaw = vector_field(world, &point, "angles")[1].to_radians();
        let forward = [yaw.cos(), yaw.sin(), 0.0];
        let state = &world.resource::<Runtime>().t5;
        let mut score: f32 = state
            .influencers
            .values()
            .filter(|i| {
                i.enabled && i.team_mask & mask != 0 && (player.is_none() || i.owner != player)
            })
            .filter_map(|i| i.score_at(origin))
            .sum();
        for (weight_mask, at, facing, bonus) in &state.base_weights {
            let to = sub(*at, origin);
            let flat = length([to[0], to[1], 0.0]);
            let cos = if flat > 0.0 {
                dot(to, forward) / flat
            } else {
                1.0
            };
            if weight_mask & mask != 0 && cos >= facing.to_radians().cos() {
                score += bonus;
            }
        }
        let randomness = state.randomness;
        score += randomness * random_unit(world);
        scored.push((score, point));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    new_array(world, scored.into_iter().map(|(_, p)| p).collect())
}

fn players_on(world: &mut World, team: &str, except: Option<u32>) -> Vec<u32> {
    let clients: Vec<(u32, u64)> = world
        .resource::<Runtime>()
        .players
        .iter()
        .filter(|(c, slot)| &*slot.sessionstate == "playing" && Some(**c) != except)
        .map(|(c, slot)| (*c, slot.object))
        .collect();
    clients
        .into_iter()
        .filter(|(_, object)| {
            team == "free"
                || super::players::entity_field(world, *object, "team") == Value::string(team)
        })
        .map(|(c, _)| c)
        .collect()
}

fn eye(world: &mut World, client: u32) -> Option<[f32; 3]> {
    let frame = FrameWorld::from_world(world);
    let ps = frame.player(ClientId(client))?;
    (ps.health > 0).then(|| {
        [
            ps.origin[0],
            ps.origin[1],
            ps.origin[2] + ps.view_height_current,
        ]
    })
}

fn sight(world: &mut World, start: [f32; 3], end: [f32; 3]) -> bool {
    let t = FrameWorld::from_world(world).trace_world(
        start,
        end,
        [0.0; 3],
        [0.0; 3],
        crate::bullet_collision::MASK_SHOT,
    );
    t.fraction >= 1.0 && t.startsolid == 0
}

fn do_damage(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let amount = float(args, 0)? as i32;
    let origin = vector(args, 1)?;
    let runtime = world.resource::<Runtime>();
    let target = match runtime.player_client_of(receiver) {
        Some(client) => super::HitTarget::Player(ClientId(client)),
        None => match runtime.presence_of(receiver) {
            Some(id) => super::HitTarget::Entity(id),
            None => {
                super::natives_engine::entity_id(world, receiver)?;
                return Ok(Value::Undefined);
            }
        },
    };
    let attacker = args
        .get(2)
        .and_then(|value| runtime.player_client_of(value))
        .map(ClientId);
    let inflictor = args.get(3).and_then(|value| runtime.presence_of(value));
    let on_head = optional(args, 4, int)?.unwrap_or(0) != 0;
    let means = optional(args, 5, string)?
        .map(|name| super::entity_damage::means_named(&name))
        .transpose()?
        .unwrap_or("MOD_UNKNOWN");
    let flags = optional(args, 6, int)?.unwrap_or(0);
    let weapon = match optional(args, 7, string)? {
        Some(name) if name != "none" => {
            crate::script_player::weapon_named(&FrameWorld::from_world(world), &name)?
        }
        _ => 0,
    };
    let hitloc = if on_head {
        weapon_iw4::HITLOC_NAMES
            .iter()
            .position(|&name| name == "head")
            .map_or(0, |i| i as u8)
    } else {
        0
    };
    world.resource_mut::<Runtime>().hits.push(super::ScriptHit {
        target,
        amount,
        origin,
        attacker,
        inflictor,
        means,
        weapon,
        flags,
        hitloc,
    });
    Ok(Value::Undefined)
}

fn player_id(world: &World, receiver: &Value) -> Result<u32, String> {
    super::natives_player::player(world, receiver)
}

fn weapon_state(world: &mut World, receiver: &Value) -> Result<Option<WeaponState>, String> {
    let id = ClientId(player_id(world, receiver)?);
    Ok(FrameWorld::from_world(world)
        .player(id)
        .and_then(|ps| WeaponState::from_i32(ps.weaponstate_primary).ok()))
}

fn stats_row(
    world: &World,
    reference: &str,
) -> Option<(Arc<BTreeMap<String, StringTable>>, usize)> {
    let tables = world.resource::<Runtime>().tables.clone();
    let table = tables.get(&entities::table_key(STATS_TABLE))?;
    let row = (0..table.rows).find(|&row| {
        table
            .cell(row, STATS_REFERENCE)
            .is_some_and(|cell| cell.eq_ignore_ascii_case(reference))
    })?;
    Some((tables, row))
}

fn stats_cell(world: &World, reference: &str, column: usize) -> Option<String> {
    let (tables, row) = stats_row(world, reference)?;
    tables[&entities::table_key(STATS_TABLE)]
        .cell(row, column)
        .map(str::to_owned)
}

fn weapon_reference(name: &str) -> &str {
    name.strip_suffix("_mp").unwrap_or(name)
}

fn weapon_facts(world: &mut World, name: &str) -> Result<weapon_iw4::WeaponCombatFacts, String> {
    if name == "none" || name.is_empty() {
        return Ok(Default::default());
    }
    let frame = FrameWorld::from_world(world);
    let index = frame
        .weapon_index_by_script_name(name)
        .ok_or_else(|| format!("unknown weapon '{name}'"))?;
    Ok(frame.weapon_combat_row(index).unwrap_or_default())
}

fn dstat_path(args: &[Value]) -> Result<String, String> {
    let mut path = String::new();
    for (i, key) in args.iter().enumerate() {
        if i > 0 {
            path.push('.');
        }
        match key {
            Value::String(s) => path.push_str(&s.to_ascii_lowercase()),
            Value::Int(n) => path.push_str(&n.to_string()),
            other => {
                return Err(format!(
                    "stat key must be a string or int, not {}",
                    kind(other)
                ));
            }
        }
    }
    if path.is_empty() {
        return Err("a stat needs a key".into());
    }
    Ok(path)
}

fn client_bits(world: &World, clients: impl IntoIterator<Item = u32>) -> u64 {
    let _ = world;
    clients
        .into_iter()
        .filter(|c| *c < 64)
        .fold(0, |bits, c| bits | 1 << c)
}

fn visibility(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut entities::ScriptEntity, u64),
) -> Result<Value, String> {
    let id = super::natives_engine::entity_id(world, receiver)?;
    let clients: Vec<u32> = world
        .resource::<Runtime>()
        .players
        .keys()
        .copied()
        .collect();
    let everyone = client_bits(world, clients);
    let runtime = world.resource_mut::<Runtime>().into_inner();
    change(runtime.entities.get_mut(&id).unwrap(), everyone);
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    register_script(registry);
    register_spawning(registry);
    register_player(registry);
    register_entity(registry);
    register_platform(registry);
    register_refused(registry);
}

fn register_script(registry: &mut NativeRegistry) {
    registry.register(Function, "clamp", |_, _, args| {
        match (arg(args, 0)?, arg(args, 1)?, arg(args, 2)?) {
            (Value::Int(v), Value::Int(lo), Value::Int(hi)) => {
                Ok(Value::Int((*v).max(*lo).min(*hi)))
            }
            _ => {
                let (v, lo, hi) = (float(args, 0)?, float(args, 1)?, float(args, 2)?);
                Ok(Value::Float(v.max(lo).min(hi)))
            }
        }
    });
    registry.register(Function, "float", |_, _, args| {
        Ok(Value::Float(match arg(args, 0)? {
            Value::String(s) => atof(s) as f32,
            _ => float(args, 0)?,
        }))
    });
    registry.register(Function, "isarray", |_, _, args| {
        Ok(Value::Int(matches!(arg(args, 0)?, Value::Array(_)).into()))
    });
    registry.register(Function, "log", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.ln()))
    });
    registry.register(Function, "vectorcross", |_, _, args| {
        Ok(Value::Vector(cross(vector(args, 0)?, vector(args, 1)?)))
    });
    registry.register(Function, "rotatepoint", |_, _, args| {
        let point = vector(args, 0)?;
        let [forward, right, up] = axes(vector(args, 1)?);
        Ok(Value::Vector(std::array::from_fn(|i| {
            forward[i] * point[0] - right[i] * point[1] + up[i] * point[2]
        })))
    });
    registry.register(Function, "isai", |_, _, args| {
        arg(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "isvehicle", |world, _, args| {
        let vehicle = world
            .resource::<Runtime>()
            .entity(arg(args, 0)?)
            .is_some_and(|(_, e)| e.kind == EntityKind::Vehicle);
        Ok(Value::Int(vehicle.into()))
    });
    registry.register(Function, "sighttracepassed", |world, _, args| {
        let (start, end) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Int(sight(world, start, end).into()))
    });
    registry.register(Function, "tablelookupcolumnforrow", |world, _, args| {
        let name = string(args, 0)?;
        let (row, column) = (int(args, 1)?, int(args, 2)?);
        let tables = world.resource::<Runtime>().tables.clone();
        let cell = tables
            .get(&entities::table_key(&name))
            .filter(|_| row >= 0 && column >= 0)
            .and_then(|t| t.cell(row as usize, column as usize))
            .unwrap_or("");
        Ok(Value::string(cell))
    });
    registry.register(Function, "getdefaultclassslot", |world, _, args| {
        let class = string(args, 0)?.to_ascii_lowercase();
        let slot = string(args, 1)?;
        let tables = world.resource::<Runtime>().tables.clone();
        let Some(table) = tables.get(&entities::table_key(STATS_TABLE)) else {
            return Err(format!("{STATS_TABLE} is not loaded"));
        };
        let found = (0..table.rows).find(|&row| {
            table
                .cell(row, STATS_SLOT)
                .is_some_and(|s| s.eq_ignore_ascii_case(&slot))
                && table.cell(row, STATS_CLASSES).is_some_and(|classes| {
                    classes
                        .split_whitespace()
                        .any(|c| c.eq_ignore_ascii_case(&class))
                })
        });
        Ok(Value::string(
            found
                .and_then(|row| table.cell(row, STATS_REFERENCE))
                .unwrap_or("weapon_null"),
        ))
    });
    registry.register(Function, "getbaseweaponitemindex", |world, _, args| {
        let name = string(args, 0)?;
        let base = weapon_reference(&name)
            .split('_')
            .next()
            .unwrap_or("")
            .to_owned();
        let index = stats_cell(world, &base, 0).map_or(0, |n| super::iw4_natives::atoi(&n));
        Ok(Value::Int(index))
    });
    registry.register(Function, "getreffromitemindex", |world, _, args| {
        let index = int(args, 0)?.to_string();
        let tables = world.resource::<Runtime>().tables.clone();
        let reference = tables
            .get(&entities::table_key(STATS_TABLE))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| t.cell(row, 0) == Some(index.as_str()))
                    .and_then(|row| t.cell(row, STATS_REFERENCE))
            })
            .unwrap_or("");
        Ok(Value::string(reference))
    });
    registry.register(Function, "getitemgroupfromitemindex", |world, _, args| {
        let index = int(args, 0)?.to_string();
        let tables = world.resource::<Runtime>().tables.clone();
        let group = tables
            .get(&entities::table_key(STATS_TABLE))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| t.cell(row, 0) == Some(index.as_str()))
                    .and_then(|row| t.cell(row, 2))
            })
            .unwrap_or("");
        Ok(Value::string(group))
    });
    registry.register(Function, "getattachmentindex", |world, _, args| {
        let name = string(args, 0)?;
        let tables = world.resource::<Runtime>().tables.clone();
        let index = tables
            .get(&entities::table_key("mp/attachmenttable.csv"))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| {
                        t.cell(row, 4)
                            .is_some_and(|c| c.eq_ignore_ascii_case(&name))
                    })
                    .and_then(|row| t.cell(row, 9))
            })
            .map_or(0, super::iw4_natives::atoi);
        Ok(Value::Int(index))
    });
    registry.register(Function, "getitemattachment", |world, _, args| {
        let (item, slot) = (int(args, 0)?.to_string(), int(args, 1)?);
        let tables = world.resource::<Runtime>().tables.clone();
        let attachment = tables
            .get(&entities::table_key(STATS_TABLE))
            .and_then(|t| {
                (0..t.rows)
                    .find(|&row| t.cell(row, 0) == Some(item.as_str()))
                    .and_then(|row| t.cell(row, 8))
            })
            .and_then(|list| {
                list.split_whitespace()
                    .nth(slot.max(0) as usize)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "none".into());
        Ok(Value::String(attachment.into()))
    });
    registry.register(Function, "getweaponindexfromname", |world, _, args| {
        let name = string(args, 0)?;
        Ok(Value::Int(
            FrameWorld::from_world(world)
                .weapon_index_by_script_name(&name)
                .map_or(0, |i| i as i32),
        ))
    });
    registry.register(Function, "isweaponprimary", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int(
            (facts.inventory_type == 0 && facts.weap_class != 6).into(),
        ))
    });
    registry.register(Function, "isweaponequipment", |world, _, args| {
        let name = string(args, 0)?;
        let slot = stats_cell(world, weapon_reference(&name), STATS_SLOT).unwrap_or_default();
        Ok(Value::Int(slot.eq_ignore_ascii_case("equipment").into()))
    });
    registry.register(Function, "isweaponspecificuse", |world, _, args| {
        let name = string(args, 0)?;
        let group = stats_cell(world, weapon_reference(&name), 2).unwrap_or_default();
        Ok(Value::Int(group.eq_ignore_ascii_case("killstreak").into()))
    });
    registry.register(Function, "isweaponcliponly", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int(
            (facts.clip_size > 0 && facts.max_ammo == 0).into(),
        ))
    });
    registry.register(Function, "weaponissemiauto", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int((facts.fire_type == 1).into()))
    });
    registry.register(Function, "weaponreloadtime", |world, _, args| {
        let facts = weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Float(facts.reload_time_ms as f32 / 1000.0))
    });
    registry.register(Function, "weaponstartammo", |world, _, args| {
        Ok(Value::Int(
            weapon_facts(world, &string(args, 0)?)?.start_ammo,
        ))
    });
    registry.register(Function, "getweaponfusetime", |world, _, args| {
        let name = string(args, 0)?;
        let frame = FrameWorld::from_world(world);
        let index = frame
            .weapon_index_by_script_name(&name)
            .ok_or_else(|| format!("unknown weapon '{name}'"))?;
        Ok(Value::Int(
            frame
                .equipment_facts_for(index)
                .map_or(0, |e| e.fuse_time_ms),
        ))
    });
    registry.register(Function, "getweaponstowedmodel", |world, _, args| {
        weapon_facts(world, &string(args, 0)?)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getaitriggerflags", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getvehicletriggerflags", |_, _, _| {
        Ok(Value::Int(0))
    });
    for name in [
        "getallnodes",
        "getnodearray",
        "getwatcherweapons",
        "getretrievableweapons",
        "getvehicletreadfxarray",
    ] {
        registry.register(Function, name, |world, _, _| new_array(world, Vec::new()));
    }
    macro_rules! weapon_sound {
        ($($name:literal => $field:ident),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                let weapon = match arg(args, 0)? {
                    Value::String(name) => FrameWorld::from_world(world)
                        .weapon_index_by_script_name(name)
                        .ok_or_else(|| format!("unknown weapon '{name}'"))?,
                    _ => int(args, 0)? as u32,
                };
                let alias = FrameWorld::from_world(world)
                    .weapon_script_sounds(weapon)
                    .and_then(|sounds| sounds.$field.clone());
                Ok(alias.map_or(Value::Undefined, |alias| Value::String(alias.into())))
            });
        )*};
    }
    weapon_sound!(
        "getweaponfiresound" => fire,
        "getweaponfiresoundplayer" => fire_player,
        "getweaponpickupsound" => pickup,
        "getweaponpickupsoundplayer" => pickup_player,
        "getweaponprojexplosionsound" => proj_explosion,
    );
    registry.register(Method, "dodamage", do_damage);
    registry.register(
        Method,
        "getloadoutitemfromprofile",
        |world, receiver, args| {
            string(args, 0)?;
            string(args, 1)?;
            player_id(world, receiver)?;
            Ok(Value::Int(0))
        },
    );
    registry.register(Method, "setenemymodel", |world, receiver, args| {
        string(args, 0)?;
        super::natives_engine::entity_id(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "launchragdoll", |world, receiver, args| {
        vector(args, 0)?;
        super::natives_player::corpse_anim(world, receiver)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "getmaxactivecontracts", |_, _, _| {
        Ok(Value::Int(0))
    });
    registry.register(Function, "getnumgvrules", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "ispregameenabled", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "ispregamegamestarted", |_, _, _| {
        Ok(Value::Int(0))
    });
    registry.register(Function, "isdemoenabled", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isdemorecording", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "isglobalstatsserver", |_, _, _| Ok(Value::Int(0)));
    registry.register(Function, "getgametypeenumfromname", |_, _, args| {
        string(args, 0)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getassignedteam", |world, _, args| {
        player_id(world, arg(args, 0)?)?;
        Ok(Value::Int(0))
    });
    registry.register(Function, "getplayerspawnid", |world, _, args| {
        let client = player_id(world, arg(args, 0)?)?;
        let state = t5(world);
        let next = state.spawn_ids.len() as i32;
        Ok(Value::Int(*state.spawn_ids.entry(client).or_insert(next)))
    });
    registry.register(Function, "clientsysregister", |world, _, args| {
        let name: Arc<str> = string(args, 0)?.into();
        let systems = &mut t5(world).client_systems;
        let id = systems.iter().position(|s| *s == name).unwrap_or_else(|| {
            systems.push(name);
            systems.len() - 1
        });
        Ok(Value::Int(id as i32))
    });
    registry.register(Function, "playsoundatposition", |world, _, args| {
        let alias = string(args, 0)?;
        let origin = vector(args, 1)?;
        super::natives_engine::play_sound_at(world, origin, &alias);
        Ok(Value::Undefined)
    });
    registry.register(Function, "getdroppedweapons", |world, _, _| {
        let items: Vec<Value> = world
            .resource::<Runtime>()
            .entities
            .iter()
            .filter(|(_, e)| matches!(e.kind, EntityKind::Item(_)))
            .map(|(id, _)| Value::Object(*id))
            .collect();
        new_array(world, items)
    });
}

fn key_number(args: &[Value], index: usize) -> Result<f32, String> {
    match args.get(index) {
        Some(Value::String(text)) => Ok(atof(text) as f32),
        _ => float(args, index),
    }
}

fn register_spawning(registry: &mut NativeRegistry) {
    registry.register(Function, "addsphereinfluencer", |world, _, args| {
        let influencer = Influencer {
            origin: vector(args, 1)?,
            shape: Shape::Sphere {
                radius: key_number(args, 2)?,
            },
            score: key_number(args, 3)?,
            team_mask: int(args, 4)?,
            curve: optional(args, 6, int)?.unwrap_or(0),
            expires_ms: timeout(world, args, 7)?,
            owner: args.get(8).and_then(object_of),
            enabled: true,
        };
        Ok(add_influencer(world, influencer))
    });
    registry.register(Function, "addcylinderinfluencer", |world, _, args| {
        let forward = vector(args, 2)?;
        let norm = length(forward).max(f32::EPSILON);
        let influencer = Influencer {
            origin: vector(args, 1)?,
            shape: Shape::Cylinder {
                forward: forward.map(|f| f / norm),
                radius: key_number(args, 4)?,
                length: key_number(args, 5)?,
            },
            score: key_number(args, 6)?,
            team_mask: int(args, 7)?,
            curve: optional(args, 9, int)?.unwrap_or(0),
            expires_ms: timeout(world, args, 10)?,
            owner: args.get(11).and_then(object_of),
            enabled: true,
        };
        Ok(add_influencer(world, influencer))
    });
    registry.register(Function, "removeinfluencer", |world, _, args| {
        let id = int(args, 0)?;
        t5(world).influencers.remove(&id);
        Ok(Value::Undefined)
    });
    registry.register(Function, "enableinfluencer", |world, _, args| {
        let (id, on) = (int(args, 0)?, int(args, 1)? != 0);
        if let Some(i) = t5(world).influencers.get_mut(&id) {
            i.enabled = on;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "setinfluencerteammask", |world, _, args| {
        let (id, mask) = (int(args, 0)?, int(args, 1)?);
        if let Some(i) = t5(world).influencers.get_mut(&id) {
            i.team_mask = mask;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "clearspawnpoints", |world, _, _| {
        t5(world).spawn_points.clear();
        Ok(Value::Undefined)
    });
    registry.register(Function, "addspawnpoints", |world, _, args| {
        let team: Arc<str> = string(args, 0)?.into();
        let points = array_values(world, arg(args, 1)?)?;
        t5(world)
            .spawn_points
            .entry(team)
            .or_default()
            .extend(points);
        Ok(Value::Undefined)
    });
    registry.register(
        Function,
        "setspawnpointrandomvariation",
        |world, _, args| {
            t5(world).randomness = float(args, 0)?;
            Ok(Value::Undefined)
        },
    );
    registry.register(Function, "setspawnpointsbaseweight", |world, _, args| {
        let mask = int(args, 0)?;
        let origin = vector(args, 1)?;
        let facing = float(args, 2)?;
        let bonus = float(args, 3)?;
        t5(world).base_weights.push((mask, origin, facing, bonus));
        Ok(Value::Undefined)
    });
    registry.register(Function, "getsortedspawnpoints", |world, _, args| {
        let point_team = string(args, 0)?;
        let influencer_team = string(args, 1)?;
        let player = args.get(2).and_then(object_of);
        sorted_spawn_points(world, &point_team, &influencer_team, player)
    });
    registry.register(Function, "isspawnpointvisible", |world, _, args| {
        let origin = vector(args, 0)?;
        let team = string(args, 2)?;
        let except = args
            .get(3)
            .and_then(|p| world.resource::<Runtime>().player_client_of(p));
        let target = [origin[0], origin[1], origin[2] + 60.0];
        for client in players_on(world, &team, except) {
            if let Some(eye) = eye(world, client)
                && sight(world, eye, target)
            {
                return Ok(Value::Int(1));
            }
        }
        Ok(Value::Int(0))
    });
    registry.register(Function, "recordusedspawnpoint", |world, _, args| {
        player_id(world, arg(args, 0)?)?;
        Ok(Value::Undefined)
    });
}

fn register_player(registry: &mut NativeRegistry) {
    macro_rules! answers {
        ($value:expr => $($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                player_id(world, receiver)?;
                Ok($value)
            });
        )*};
    }
    answers!(Value::Int(0) => "isdemoclient", "issplitscreen", "istestclient", "isinvehicle",
        "isremotecontrolling", "isflared", "deathstreakactive", "isitemlocked",
        "depthofplayerinwater", "needsrevive");
    answers!(Value::Int(1) => "isitempurchased");
    answers!(Value::Undefined => "getvehicleoccupied");
    registry.register(Method, "islocaltohost", |world, receiver, _| {
        Ok(Value::Int((player_id(world, receiver)? == 0).into()))
    });
    registry.register(Method, "isplayeronsamemachine", |world, receiver, args| {
        let (a, b) = (
            player_id(world, receiver)?,
            player_id(world, arg(args, 0)?)?,
        );
        Ok(Value::Int((a == b).into()))
    });
    registry.register(Method, "isfiring", |world, receiver, _| {
        let state = weapon_state(world, receiver)?;
        Ok(Value::Int(
            matches!(state, Some(WeaponState::Firing)).into(),
        ))
    });
    registry.register(Method, "ismeleeing", |world, receiver, _| {
        let state = weapon_state(world, receiver)?;
        Ok(Value::Int(
            matches!(
                state,
                Some(WeaponState::MeleeInit | WeaponState::MeleeFire | WeaponState::MeleeEnd)
            )
            .into(),
        ))
    });
    registry.register(Method, "isthrowinggrenade", |world, receiver, _| {
        let state = weapon_state(world, receiver)?;
        Ok(Value::Int(
            matches!(
                state,
                Some(
                    WeaponState::OffhandInit
                        | WeaponState::OffhandPrepare
                        | WeaponState::OffhandHold
                        | WeaponState::OffhandStart
                        | WeaponState::Offhand
                        | WeaponState::OffhandEnd
                )
            )
            .into(),
        ))
    });
    registry.register(Method, "isswitchingweapons", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        let state = FrameWorld::from_world(world)
            .player(id)
            .map_or(0, |ps| ps.weaponstate_primary);
        Ok(Value::Int(super::weapons::is_changing_weapon(state).into()))
    });
    registry.register(Method, "getcurrentoffhand", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        let frame = FrameWorld::from_world(world);
        let weapon = frame
            .player(id)
            .map_or(0, |ps| ps.offhand_primary.max(0) as u32);
        let name = if weapon == 0 {
            "none".to_owned()
        } else {
            script_player::weapon_name(&frame, weapon).to_owned()
        };
        Ok(Value::String(name.into()))
    });
    registry.register(Method, "getweaponslist", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        let frame = FrameWorld::from_world(world);
        let names: Vec<Value> = script_player::weapons(&frame, id, script_player::WeaponList::All)
            .into_iter()
            .map(|w| Value::String(script_player::weapon_name(&frame, w).into()))
            .collect();
        new_array(world, names)
    });
    registry.register(Method, "getfractionstartammo", |world, receiver, args| {
        let id = ClientId(player_id(world, receiver)?);
        let name = string(args, 0)?;
        let facts = weapon_facts(world, &name)?;
        let frame = FrameWorld::from_world(world);
        let weapon = script_player::weapon_named(&frame, &name)?;
        let held = script_player::ammo_clip(&frame, id, weapon)
            + script_player::ammo_stock(&frame, id, weapon);
        let start = (facts.start_ammo + facts.clip_size).max(1);
        Ok(Value::Float(held as f32 / start as f32))
    });
    registry.register(Method, "getmovespeedscale", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        Ok(Value::Float(
            FrameWorld::from_world(world)
                .player(id)
                .map_or(1.0, |ps| ps.move_speed_scale_multiplier),
        ))
    });
    registry.register(Method, "getperks", |world, receiver, _| {
        let client = player_id(world, receiver)?;
        let perks: Vec<Value> = world
            .resource::<Runtime>()
            .players
            .get(&client)
            .map(|slot| {
                slot.perks
                    .iter()
                    .map(|p| Value::String(p.clone()))
                    .collect()
            })
            .unwrap_or_default();
        new_array(world, perks)
    });
    registry.register(Method, "getdstat", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        let path = dstat_path(args)?;
        Ok(t5(world)
            .dstats
            .get(&(client, path))
            .cloned()
            .unwrap_or(Value::Int(0)))
    });
    registry.register(Method, "setdstat", |world, receiver, args| {
        let client = player_id(world, receiver)?;
        let (value, keys) = args
            .split_last()
            .ok_or("setdstat needs a key and a value")?;
        let path = dstat_path(keys)?;
        t5(world).dstats.insert((client, path), value.clone());
        Ok(Value::Undefined)
    });
    macro_rules! status {
        ($($name:literal => $status:literal = $on:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                let client = player_id(world, receiver)?;
                let statuses = &mut t5(world).statuses;
                if $on {
                    statuses.insert((client, $status));
                } else {
                    statuses.remove(&(client, $status));
                }
                Ok(Value::Undefined)
            });
        )*};
    }
    status!(
        "setburn" => "burning" = true,
        "stopburning" => "burning" = false,
        "startpoisoning" => "poisoned" = true,
        "stoppoisoning" => "poisoned" = false,
    );
    registry.register(Method, "ispoisoned", |world, receiver, _| {
        let client = player_id(world, receiver)?;
        Ok(Value::Int(
            t5(world).statuses.contains(&(client, "poisoned")).into(),
        ))
    });
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                super::natives_player::present(world, receiver, $name, args)
            });
        )*};
    }
    presented!(
        "setclientuivisibilityflag",
        "displaychallengecomplete",
        "displaycontract",
        "displayendgame",
        "displayendgamemilestone",
        "displaygamemodemessage",
        "displaykillstreak",
        "displaymedal",
        "displayrankup",
        "displayteammessage",
        "displaywagerpopup",
        "clearcenterpopups",
        "clearpopups",
        "clearendgame",
        "directionalhitindicator",
        "visionsetlerpratio",
        "setplayerrenderoptions",
        "starttanning",
        "setsprintduration",
        "setsprintcooldown",
        "disableweaponcycling",
        "enableweaponcycling",
        "setlaststandprevweap",
        "setblockweaponpickup",
        "cameraactivate",
        "camerasetposition",
        "camerasetlookat",
        "setcameraspikeactive",
        "setspawnclientflag",
        "setnemesisxuid",
        "giveachievement",
        "disabledeathstreak",
        "enabledeathstreak",
        "setpregameclass",
        "setpregameteam",
        "beginlocationairstrikeselection",
        "beginlocationartilleryselection",
        "beginlocationcomlinkselection",
        "beginlocationmortarselection",
        "beginlocationnapalmselection",
    );
    registry.register(Method, "calcplayeroptions", |world, receiver, args| {
        player_id(world, receiver)?;
        Ok(Value::Int(int(args, 0)? | int(args, 1)? << 4))
    });
    registry.register(Method, "calcweaponoptions", |world, receiver, args| {
        player_id(world, receiver)?;
        Ok(Value::Int(int(args, 0)?))
    });
}

fn register_entity(registry: &mut NativeRegistry) {
    registry.register(Method, "setclientflag", |world, receiver, args| {
        let (id, bit) = (
            super::natives_engine::entity_id(world, receiver)?,
            int(args, 0)?,
        );
        *t5(world).client_flags.entry(id).or_default() |= 1 << bit.clamp(0, 31);
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearclientflag", |world, receiver, args| {
        let (id, bit) = (
            super::natives_engine::entity_id(world, receiver)?,
            int(args, 0)?,
        );
        *t5(world).client_flags.entry(id).or_default() &= !(1 << bit.clamp(0, 31));
        Ok(Value::Undefined)
    });
    registry.register(Method, "getclientflag", |world, receiver, args| {
        let (id, bit) = (
            super::natives_engine::entity_id(world, receiver)?,
            int(args, 0)?,
        );
        let flags = t5(world).client_flags.get(&id).copied().unwrap_or(0);
        Ok(Value::Int((flags >> bit.clamp(0, 31) & 1) as i32))
    });
    registry.register(Method, "setinvisibletoall", |world, receiver, _| {
        visibility(world, receiver, |e, _| {
            e.hidden = true;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "setvisibletoall", |world, receiver, _| {
        visibility(world, receiver, |e, _| {
            e.hidden = false;
            e.shown_to = 0;
        })
    });
    registry.register(Method, "setvisibletoplayer", |world, receiver, args| {
        let bit = client_bits(world, [player_id(world, arg(args, 0)?)?]);
        visibility(world, receiver, |e, _| {
            if e.hidden {
                e.shown_to |= bit;
            }
        })
    });
    registry.register(Method, "setinvisibletoplayer", |world, receiver, args| {
        let bit = client_bits(world, [player_id(world, arg(args, 0)?)?]);
        visibility(world, receiver, |e, everyone| {
            if !e.hidden {
                e.hidden = true;
                e.shown_to = everyone;
            }
            e.shown_to &= !bit;
        })
    });
    registry.register(Method, "setvisibletoteam", |world, receiver, args| {
        let team = string(args, 0)?;
        let clients = players_on(world, &team, None);
        let bits = client_bits(world, clients);
        visibility(world, receiver, |e, _| {
            e.hidden = true;
            e.shown_to = bits;
        })
    });
    registry.register(Method, "islinkedto", |world, receiver, args| {
        let runtime = world.resource::<Runtime>();
        let parent = object_of(arg(args, 0)?);
        let linked = runtime
            .entity(receiver)
            .and_then(|(_, e)| e.linked_to.as_ref())
            .is_some_and(|link| Some(link.parent) == parent);
        Ok(Value::Int(linked.into()))
    });
    registry.register(Method, "getgroundent", |world, receiver, _| {
        let id = ClientId(player_id(world, receiver)?);
        let number = FrameWorld::from_world(world)
            .player(id)
            .map_or(-1, |ps| ps.ground_entity_num);
        let runtime = world.resource::<Runtime>();
        Ok(runtime
            .entities
            .iter()
            .find(|(_, e)| e.number == number && e.kind != EntityKind::HudElem)
            .map_or(Value::Undefined, |(id, _)| Value::Object(*id)))
    });
    macro_rules! entity_fields {
        ($($name:literal => $field:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, args| {
                let id = super::natives_engine::entity_id(world, receiver)?;
                let value = arg(args, 0)?.clone();
                world.resource_mut::<Runtime>().set_object_field(id, $field, value);
                Ok(Value::Undefined)
            });
        )*};
    }
    entity_fields!(
        "setowner" => "owner",
        "setentityowner" => "owner",
        "setteam" => "team",
        "setattacker" => "attacker",
    );
    registry.register(Method, "clearentityowner", |world, receiver, _| {
        let id = super::natives_engine::entity_id(world, receiver)?;
        world
            .resource_mut::<Runtime>()
            .set_object_field(id, "owner", Value::Undefined);
        Ok(Value::Undefined)
    });
    macro_rules! entity_accepts {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Method, $name, |world, receiver, _| {
                super::natives_engine::entity_id(world, receiver)?;
                Ok(Value::Undefined)
            });
        )*};
    }
    entity_accepts!(
        "sethintlowpriority",
        "sethintstringforperk",
        "setperkfortrigger",
        "setignoreentfortrigger",
        "enablelinkto",
        "setforcenocull",
        "setrevivehintstring",
        "connectpaths",
        "disconnectpaths",
        "clientsyssetstate",
        "sendfaceevent",
    );
}

fn register_platform(registry: &mut NativeRegistry) {
    macro_rules! platform {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, _| Ok(Value::Undefined));
        )*};
    }
    platform!(
        "bbprint",
        "incrementcounter",
        "pixbeginevent",
        "pixendevent",
        "pixmarker",
        "recordmatchbegin",
        "recordplayermatchend",
        "recordplayerstats",
        "uploadstats",
        "reportfilm",
        "changeadvertisedstatus",
        "pcserverupdateplaylist",
        "incrementescrow",
        "adddemobookmark",
        "setdemointermissionpoint",
        "startdemorecording",
        "stopdemorecording",
        "resettimeout",
        "setarchive",
        "setmatchtalkflag",
        "setqosgamedatapayload",
        "resetpregamedata",
        "debugstar",
        "sphere",
    );
    macro_rules! presented {
        ($($name:literal),* $(,)?) => {$(
            registry.register(Function, $name, |world, _, args| {
                world.resource_mut::<Runtime>().presented.insert($name, args.to_vec());
                Ok(Value::Undefined)
            });
        )*};
    }
    presented!(
        "settimescale",
        "announcement",
        "clientannouncement",
        "setmatchflag",
        "setscoreboardcolumns",
        "setbombtimer",
        "setvolfog",
        "clientsyssetstate",
        "reviveobituary",
        "artilleryiconlocation",
        "setteamsatellite",
        "setteamspyplane",
        "objective_setinvisibletoall",
        "objective_setinvisibletoplayer",
        "objective_setvisibletoplayer",
    );
    registry.register(Function, "getteamsatellite", |world, _, args| {
        let team = string(args, 0)?;
        Ok(presented_team_flag(world, "setteamsatellite", &team))
    });
    registry.register(Function, "getteamspyplane", |world, _, args| {
        let team = string(args, 0)?;
        Ok(presented_team_flag(world, "setteamspyplane", &team))
    });
}

fn presented_team_flag(world: &World, key: &str, team: &str) -> Value {
    match world
        .resource::<Runtime>()
        .presented
        .get(key)
        .map(Vec::as_slice)
    {
        Some([Value::String(t), value, ..]) if **t == *team => value.clone(),
        _ => Value::Int(0),
    }
}

fn register_refused(registry: &mut NativeRegistry) {
    macro_rules! refused {
        ($namespace:ident: $($name:literal),* => $message:literal) => {$(
            registry.register($namespace, $name, |_, _, _| Err($message.into()));
        )*};
    }
    refused!(Function: "addtestclient" => "test clients join through the session");
    refused!(Function: "ban", "killserver" => "the host decides who stays and when the server stops");
    refused!(Function: "sethostmigrationstatus" => "host migration is not simulated");
    refused!(Function: "getcontractname", "getcontractrequiredcount", "getcontractrequirements",
        "getcontractresetconditions", "getcontractrewardcp", "getcontractrewardxp",
        "getcontractstatname", "getcontractstattype"
        => "contracts are online-only; none are active");
    refused!(Method: "getindexforactivecontract", "getactivecontractprogress",
        "getactivecontracttimepassed", "hasactivecontractexpired",
        "incrementactivecontractprogress", "incrementactivecontracttime",
        "isactivecontractcomplete", "resetactivecontractprogress"
        => "contracts are online-only; none are active");
    refused!(Function: "getgvrule", "getwagergametypelist", "pregamestartgame"
        => "game variants, wagers and the pregame lobby are not simulated");
    refused!(Method: "getpregameclass" => "the pregame lobby is not simulated");
    refused!(Function: "getcustomclassloadoutitem", "getcustomclassmodifier"
        => "custom game mode classes are not loaded");

    refused!(Function: "getmaxvehicles", "spawnvehicle"
        => "vehicles are not simulated");
    refused!(Method: "getoccupantseat", "getseatoccupant", "getvehoccupants", "usevehicle",
        "launchvehicle", "makevehicleunusable", "setvehicleteam", "vehgetmodel",
        "gettreadhealth", "getspeed", "getspeedmph", "setspeed", "isvehicleimmunetodamage",
        "finishvehicledamage", "finishvehicleradiusdamage", "changeseatbuttonpressed",
        "setheliheightlock", "isinsideheliheightlock", "setviewclamp", "resetviewclamp",
        "returnplayercontrol", "setjitterparams", "heliturretdogtrace",
        "heliturretsighttrace", "setturrettargetvec", "setanim", "useanimtree"
        => "receiver is not a vehicle");

    refused!(Method: "spawnactor", "setgoalnode", "setgoalpos",
        "forceteleport", "orientmode", "traversemode", "setanimstate", "setaimanimweights",
        "getnegotiationstartnode", "setflashbanged", "finishactordamage", "getshootatpos",
        "clearentitytarget", "playbattlechattertoteam"
        => "actors are not simulated");
    refused!(Method: "setscriptgoal", "clearscriptgoal", "hasscriptgoal", "setscriptenemy",
        "clearscriptenemy", "getthreat", "getlookaheaddir", "getlookaheaddist",
        "pressattackbutton", "pressusebutton"
        => "bots are driven by the session, not by script goals");
    refused!(Method: "canplayerplaceturret", "carryturret", "stopcarryturret", "relinktoturret",
        "setturretcarried", "setturrethint", "setturretowner", "setturrettype",
        "setscanningpitch", "maketurretunusable", "startfiring", "stopfiring",
        "stopshootturret", "clearturrettarget", "actionslotfourbuttonpressed"
        => "receiver is not a turret");
    refused!(Function: "target_getarray", "target_isincircle", "target_istarget",
        "target_remove", "target_set", "target_setturretaquire"
        => "missile lock targets are not simulated");
    refused!(Method: "missile_settarget", "ismissileinsideheightlock", "getlockonradius",
        "getlockonspeed", "linkguidedmissilecamera", "unlinkguidedmissilecamera",
        "makegrenadedud", "predictgrenade", "launchbomb"
        => "receiver is not a guided missile");

    refused!(Function: "spawntimedfx", "boundswouldtelefrag", "testspawnpoint"
        => "not simulated");
    refused!(Method: "launch", "physicslaunch", "fakefire",
        "spawnnapalmgroundflame", "revive", "docowardswayanims",
        "ghost", "teleport", "istouchingswept", "istouchingvolume", "playersighttrace",
        "throwbuttonpressed", "actionslotonebuttonpressed", "depthinwater",
        "devaddpitch", "devaddroll", "devaddyaw"
        => "not simulated");
}
