use super::{
    args::{byte_string, string},
    arrays::new_array,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use asset_core::PathNode;
use std::collections::BTreeMap;
use std::sync::Arc;
pub(crate) mod blockers;

#[derive(Clone, Debug, Default)]
pub(crate) struct PathNodes {
    data: Arc<[PathNode]>,
    objects: Vec<u64>,
    indices: BTreeMap<u64, usize>,
    installed: bool,
    pub(crate) links: Vec<Vec<asset_core::PathLink>>,
    pub(crate) blockers: BTreeMap<u64, blockers::Blocker>,
    pub(crate) dangerous_until: Vec<[i32; 3]>,
}

const FIELDS: [&str; 11] = [
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
];

pub(crate) fn install(runtime: &mut RoundScript, data: Arc<[PathNode]>) -> Result<(), String> {
    if runtime.path_nodes.installed {
        return Err("path nodes already installed".into());
    }
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
        objects,
        indices,
        installed: true,
    };
    blockers::install(runtime);
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
    pub(crate) fn mark_dangerous(&mut self, index: u16, team: u8, now: i32) -> Result<(), String> {
        let team = usize::from(team.checked_sub(1).ok_or("invalid danger team")?);
        if team >= 3 {
            return Err("invalid danger team".into());
        }
        let node = self
            .data
            .get(usize::from(index))
            .ok_or("invalid danger node")?;
        let mut indices = vec![usize::from(index)];
        for &overlap in &node.overlap_nodes {
            if overlap < 0 {
                break;
            }
            indices.push(overlap as usize);
        }
        if indices.iter().any(|&i| i >= self.dangerous_until.len()) {
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
    pub(crate) fn is_empty(&self) -> bool {
        self.data.is_empty()
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
