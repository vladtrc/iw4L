use super::{
    args::{byte_string, string},
    arrays::new_array,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use asset_core::PathNode;
use std::collections::BTreeMap;
use std::sync::Arc;
pub(crate) mod blockers;
mod facing;
mod queries;

#[derive(Clone, Default)]
pub(crate) struct PathNodes {
    data: Arc<[PathNode]>,
    visibility: Arc<[u8]>,
    objects: Vec<u64>,
    indices: BTreeMap<u64, usize>,
    installed: bool,
    pub(crate) links: Vec<Vec<asset_core::PathLink>>,
    pub(crate) blockers: BTreeMap<u64, Vec<(usize, u16)>>,
    pub(crate) dangerous_until: Vec<[i32; 3]>,
}

impl std::fmt::Debug for PathNodes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathNodes")
            .field("data", &self.data)
            .field("visibility_bytes", &self.visibility.len())
            .field("objects", &self.objects)
            .field("indices", &self.indices)
            .field("installed", &self.installed)
            .field("links", &self.links)
            .field("blockers", &self.blockers)
            .field("dangerous_until", &self.dangerous_until)
            .finish()
    }
}

const FIELDS: [&str; 12] = [
    "targetname",
    "target",
    "animscript",
    "script_linkname",
    "script_noteworthy",
    "origin",
    "angles",
    "radius",
    "minusedistsq",
    "spawnflags",
    "type",
    "owner",
];

pub(crate) fn install(
    runtime: &mut RoundScript,
    data: Arc<[PathNode]>,
    visibility: Arc<[u8]>,
) -> Result<(), String> {
    if runtime.path_nodes.installed {
        return Err("path nodes already installed".into());
    }
    perf::instant!("path_visibility_loaded", "iw4l.sim", "nodes" => data.len() as u64,
        "bytes" => visibility.len() as u64);
    let mut objects = Vec::with_capacity(data.len());
    for node in data.iter() {
        let id = runtime.new_object()?;
        for (name, value) in [
            ("targetname", &node.targetname),
            ("target", &node.target),
            ("animscript", &node.animscript),
            ("script_linkname", &node.script_linkname),
            ("script_noteworthy", &node.script_noteworthy),
        ] {
            if let Some(value) = value {
                runtime.set_object_field(id, name, Value::string(value));
            }
        }
        for (name, value) in [
            ("origin", Value::Vector(node.origin)),
            ("angles", Value::Vector([0.0, node.yaw, 0.0])),
            ("radius", Value::Float(node.radius)),
            ("minusedistsq", Value::Float(node.min_use_dist_sq)),
            ("spawnflags", Value::Int(i32::from(node.spawnflags))),
            (
                "type",
                Value::string(node.type_name().ok_or("invalid path node type")?),
            ),
        ] {
            runtime.set_object_field(id, name, value);
        }
        objects.push(id);
    }
    let indices = objects
        .iter()
        .copied()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    runtime.path_nodes = PathNodes {
        dangerous_until: vec![[0; 3]; data.len()],
        links: data.iter().map(|node| node.links.clone()).collect(),
        blockers: Default::default(),
        data,
        visibility,
        objects,
        indices,
        installed: true,
    };
    blockers::install(runtime);
    Ok(())
}

pub(crate) fn install_spawn_fields(
    runtime: &mut RoundScript,
    entities: &[Vec<(String, String)>],
    keys: &BTreeMap<String, super::entities::KeyType>,
) -> Result<(), String> {
    if runtime.profile != asset_core::RuntimeProfile::Campaign
        && runtime.path_nodes.objects.is_empty()
    {
        return Ok(());
    }
    let nodes: Vec<_> = entities
        .iter()
        .filter(|pairs| {
            pairs.iter().any(|(key, value)| {
                key.eq_ignore_ascii_case("classname") && value.starts_with("node_")
            })
        })
        .collect();
    if nodes.is_empty() {
        return Ok(());
    }
    if !runtime.path_nodes.installed || nodes.len() != runtime.path_nodes.objects.len() {
        return Err(format!(
            "pathnode spawnvars count {} does not match installed graph {}",
            nodes.len(),
            runtime.path_nodes.objects.len()
        ));
    }
    let mut fields = 0;
    for (index, pairs) in nodes.iter().enumerate() {
        let object = runtime.path_nodes.objects[index];
        for (key, text) in pairs.iter() {
            let key = key.to_ascii_lowercase();
            if FIELDS.contains(&key.as_str()) {
                continue;
            }
            let value = match keys.get(&key) {
                Some(super::entities::KeyType::String) => Value::string(text),
                Some(super::entities::KeyType::Int) => Value::Int(super::natives::iw4::atoi(text)),
                Some(super::entities::KeyType::Float) => {
                    Value::Float(super::natives::iw4::atof(text) as f32)
                }
                Some(super::entities::KeyType::Vector) => {
                    return Err(format!(
                        "pathnode {index}: vector spawn field '{key}' is not supported"
                    ));
                }
                None => continue,
            };
            runtime.set_object_field(object, &key, value);
            fields += 1;
        }
    }
    perf::instant!("path_spawn_fields", "iw4l.sim", "nodes" => nodes.len() as u64,
        "fields" => fields as u64);
    Ok(())
}

pub(crate) fn check_field_write(
    runtime: &RoundScript,
    object: u64,
    field: &str,
) -> Result<(), String> {
    if runtime.path_nodes.indices.contains_key(&object) && FIELDS.contains(&field) {
        Err(format!("pathnode property '{field}' is read-only"))
    } else {
        Ok(())
    }
}

impl PathNodes {
    pub(crate) fn visible(&self, from: u16, to: u16) -> bool {
        let (from, to) = (usize::from(from), usize::from(to));
        if from >= self.data.len() || to >= self.data.len() {
            return false;
        }
        if from == to {
            return true;
        }
        let bit = from.min(to) * self.data.len() + from.max(to);
        self.visibility
            .get(bit / 8)
            .is_some_and(|byte| byte & (1 << (bit % 8)) != 0)
    }
    pub(crate) fn nodes(&self) -> &[PathNode] {
        &self.data
    }
    pub(crate) fn mark_connected_dangerous(
        &mut self,
        index: u16,
        team: u8,
        now: i32,
    ) -> Result<(), String> {
        let team = usize::from(team.checked_sub(1).ok_or("invalid danger team")?);
        if team >= 3 {
            return Err("invalid danger team".into());
        }
        let node = self
            .data
            .get(usize::from(index))
            .ok_or("invalid danger node")?;
        let indices = std::iter::once(usize::from(index))
            .chain(node.links.iter().map(|link| usize::from(link.node)));
        for index in indices {
            let until = self
                .dangerous_until
                .get_mut(index)
                .ok_or("invalid connected danger node")?;
            until[team] = now.wrapping_add(if now < until[team] { 60000 } else { 15000 });
        }
        Ok(())
    }
    pub(crate) fn mark_dangerous(&mut self, index: u16, team: u8, now: i32) -> Result<(), String> {
        let team = usize::from(team.checked_sub(1).ok_or("invalid danger team")?);
        if team >= 3 {
            return Err("invalid danger team".into());
        }
        let node = self
            .data
            .get(usize::from(index))
            .ok_or("invalid danger node")?;
        let indices = std::iter::once(usize::from(index)).chain(
            node.overlap_nodes
                .iter()
                .copied()
                .take_while(|&overlap| overlap >= 0)
                .map(|overlap| overlap as usize),
        );
        if indices.clone().any(|i| i >= self.dangerous_until.len()) {
            return Err("invalid overlapping danger node".into());
        }
        for index in indices {
            self.dangerous_until[index][team] = now.wrapping_add(15000);
        }
        Ok(())
    }
    pub(crate) fn at(&self, index: u16) -> Option<&PathNode> {
        self.data.get(usize::from(index))
    }
    pub(crate) fn value(&self, index: u16) -> Value {
        self.objects
            .get(usize::from(index))
            .map_or(Value::Undefined, |id| Value::Object(*id))
    }
    pub(crate) fn actor_angle(&self, index: u16) -> Option<f32> {
        let node = self.data.get(usize::from(index))?;
        if node.spawnflags & 0x8000 == 0 {
            return None;
        }
        Some(
            node.yaw
                + match node.kind {
                    6 => -90.,
                    7 => 90.,
                    _ => 0.,
                },
        )
    }

    pub(crate) fn allowed_stances(&self, index: u16) -> Option<u8> {
        let node = self.data.get(usize::from(index))?;
        Some(7 & !(node.spawnflags >> 2) as u8)
    }

    pub(crate) fn trace_values(&self, roots: &mut Vec<Value>) {
        roots.extend(self.objects.iter().copied().map(Value::Object));
    }

    pub(super) fn index(&self, receiver: &Value) -> Result<u16, String> {
        let Value::Object(id) = receiver else {
            return Err("not a pathnode".into());
        };
        self.indices
            .get(id)
            .and_then(|index| u16::try_from(*index).ok())
            .ok_or_else(|| "not a pathnode".into())
    }

    pub(super) fn node(&self, receiver: &Value) -> Result<&PathNode, String> {
        let Value::Object(id) = receiver else {
            return Err("not a pathnode".into());
        };
        self.indices
            .get(id)
            .map(|index| &self.data[*index])
            .ok_or_else(|| "not a pathnode".into())
    }
}

fn find(runtime: &RoundScript, args: &[Value], array: bool) -> Result<Vec<Value>, String> {
    let value = byte_string(args, 0)?;
    let key = string(args, 1)?;
    let field = FIELDS.iter().position(|name| *name == key);
    match field {
        None if !array => return Ok(Vec::new()),
        None => return Err(format!("key '{key}' does not internally belong to nodes")),
        Some(5..) => return Err("key is not internally a string".into()),
        _ => {}
    }
    let mut found = Vec::new();
    for (index, node) in runtime.path_nodes.data.iter().enumerate() {
        let string = match field.unwrap() {
            0 => &node.targetname,
            1 => &node.target,
            2 => &node.animscript,
            3 => &node.script_linkname,
            4 => &node.script_noteworthy,
            _ => unreachable!(),
        };
        if string
            .as_ref()
            .is_some_and(|s| s.as_bytes() == value.as_bytes())
        {
            if !array && !found.is_empty() {
                return Err("getnode used with more than one node".into());
            }
            found.push(Value::Object(runtime.path_nodes.objects[index]));
        }
    }
    Ok(found)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    blockers::register(registry);
    queries::register(registry);
    use Namespace::{Function, Method};
    registry.register(Function, "getallnodes", |world, _, _| {
        let values = world
            .resource::<RoundScript>()
            .path_nodes
            .objects
            .iter()
            .copied()
            .map(Value::Object)
            .collect();
        new_array(world, values)
    });
    registry.register(Function, "getnode", |world, _, args| {
        Ok(find(world.resource::<RoundScript>(), args, false)?
            .into_iter()
            .next()
            .unwrap_or(Value::Undefined))
    });
    registry.register(Function, "getnodearray", |world, _, args| {
        let values = find(world.resource::<RoundScript>(), args, true)?;
        new_array(world, values)
    });
    registry.register(Method, "getvalidcoverpeekouts", |world, receiver, _| {
        let flags = world
            .resource::<RoundScript>()
            .path_nodes
            .node(receiver)?
            .spawnflags;
        // Authored node flags and script strings are the compatibility format.
        let values = [(0x200, "over"), (0x400, "left"), (0x800, "right")]
            .into_iter()
            .filter(|(mask, _)| flags & mask != 0)
            .map(|(_, name)| Value::string(name))
            .collect();
        new_array(world, values)
    });
    registry.register(Method, "gethighestnodestance", |world, receiver, _| {
        let flags = world
            .resource::<RoundScript>()
            .path_nodes
            .node(receiver)?
            .spawnflags;
        for (mask, name) in [(4, "stand"), (8, "crouch"), (16, "prone")] {
            if flags & mask == 0 {
                return Ok(Value::string(name));
            }
        }
        Err("node does not allow any stance".into())
    });
    registry.register(Method, "doesnodeallowstance", |world, receiver, args| {
        let flags = world
            .resource::<RoundScript>()
            .path_nodes
            .node(receiver)?
            .spawnflags;
        let stance = string(args, 0)?;
        let mask = match stance.as_str() {
            "stand" => 4,
            "crouch" => 8,
            "prone" => 16,
            _ => return Ok(Value::Undefined),
        };
        Ok(Value::Int(i32::from(flags & mask == 0)))
    });
}
