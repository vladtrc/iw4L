use asset_core::{PathLink, PathNode};
use fastfile_iw4::{PathDataGeometry, Ptr, ScriptStrings, ZonePtr, ZoneStream};

pub fn capture_path_nodes(
    stream: &ZoneStream<'_>,
    geometry: PathDataGeometry,
    strings: ScriptStrings,
) -> Result<Vec<PathNode>, String> {
    if geometry.node_count > u16::MAX as usize {
        return Err("path node count exceeds node-index capacity".into());
    }
    let Some(base) = geometry.nodes else {
        return if geometry.node_count == 0 {
            Ok(Vec::new())
        } else {
            Err("path nodes have no backing table".into())
        };
    };
    (0..geometry.node_count)
        .map(|index| {
            capture_node(
                stream,
                base.at(index * stream.layout(136, 168)),
                strings,
                geometry.node_count,
            )
            .map_err(|error| format!("path node {index}: {error}"))
        })
        .collect()
}

fn capture_node(
    s: &ZoneStream<'_>,
    p: Ptr,
    strings: ScriptStrings,
    count: usize,
) -> Result<PathNode, String> {
    let read = || -> fastfile_iw4::Result<PathNode> {
        Ok(PathNode {
            kind: s.u32_at(p, 0)?,
            spawnflags: s.u16_at(p, 4)?,
            origin: [s.f32_at(p, 20)?, s.f32_at(p, 24)?, s.f32_at(p, 28)?],
            yaw: s.f32_at(p, 32)?,
            forward: [s.f32_at(p, 36)?, s.f32_at(p, 40)?],
            radius: s.f32_at(p, 44)?,
            min_use_dist_sq: s.f32_at(p, 48)?,
            overlap_nodes: [s.u16_at(p, 52)? as i16, s.u16_at(p, 54)? as i16],
            ..Default::default()
        })
    };
    let mut node = read().map_err(|error| error.to_string())?;
    if node.type_name().is_none()
        || node
            .origin
            .into_iter()
            .chain([node.yaw, node.radius, node.min_use_dist_sq])
            .chain(node.forward)
            .any(|value| !value.is_finite())
    {
        return Err("invalid node type or nonfinite constants".into());
    }
    if node
        .overlap_nodes
        .iter()
        .any(|index| *index < -1 || (*index >= 0 && *index as usize >= count))
    {
        return Err("overlap node outside the node table".into());
    }
    for (offset, field) in [
        (6, &mut node.targetname),
        (8, &mut node.script_linkname),
        (10, &mut node.script_noteworthy),
        (12, &mut node.target),
        (14, &mut node.animscript),
    ] {
        let id = s.u16_at(p, offset).map_err(|error| error.to_string())?;
        *field = if id == 0 {
            None
        } else {
            Some(
                strings
                    .get(s, id)
                    .ok_or_else(|| format!("invalid script string {id}"))?
                    .to_owned(),
            )
        };
    }
    let link_count = s.u16_at(p, 56).map_err(|error| error.to_string())? as usize;
    let links = match s
        .ptr_at(p, s.layout(60, 64))
        .map_err(|error| error.to_string())?
    {
        ZonePtr::Null if link_count == 0 => return Ok(node),
        ZonePtr::Offset(pointer) => s.resolve_alias(pointer),
        _ => return Err("node links have no resolved backing table".into()),
    };
    for index in 0..link_count {
        let p = links.at(index * 12);
        let read = || -> fastfile_iw4::Result<PathLink> {
            Ok(PathLink {
                distance: s.f32_at(p, 0)?,
                node: s.u16_at(p, 4)?,
                disconnect_count: s.u8_at(p, 6)?,
                negotiation: s.u8_at(p, 7)? != 0,
                flags: s.u8_at(p, 8)?,
                bad_place_count: [s.u8_at(p, 9)?, s.u8_at(p, 10)?, s.u8_at(p, 11)?],
            })
        };
        let link = read().map_err(|error| error.to_string())?;
        if link.node as usize >= count || !link.distance.is_finite() || link.distance < 0.0 {
            return Err(format!("invalid outgoing link {index}"));
        }
        node.links.push(link);
    }
    Ok(node)
}
