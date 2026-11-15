use asset_core::{IW4_NODE_TYPES, PathNode};
use asset_iw4::size as sz;
use fastfile_iw4::{ScriptStrings, ZonePtr, ZoneStream};

const NODE_STRINGS: [(usize, &str); 5] = [
    (6, "targetname"),
    (8, "script_linkname"),
    (10, "script_noteworthy"),
    (12, "target"),
    (14, "animscript"),
];

pub fn path_nodes(s: &ZoneStream<'_>, strings: &ScriptStrings) -> Vec<PathNode> {
    let Some(data) = s.path_data() else {
        return Vec::new();
    };
    let Some(nodes) = data.nodes else {
        return Vec::new();
    };
    let stride = s.layout(sz::PATH_NODE, 168);
    (0..data.node_count)
        .map(|i| -> Option<PathNode> {
            let n = nodes.at(i * stride);
            let f = |off| s.f32_at(n, off).ok();
            let kind = *IW4_NODE_TYPES.get(s.u32_at(n, 0).ok()? as usize)?;
            let keys = NODE_STRINGS
                .iter()
                .filter_map(|&(off, key)| {
                    let value = strings.get(s, s.u16_at(n, off).ok()?)?;
                    (!value.is_empty()).then(|| (key, value.to_owned()))
                })
                .collect();
            let link_count = s.u16_at(n, 56).ok()? as usize;
            let links = match s.ptr_at(n, s.layout(60, 64)).ok()? {
                ZonePtr::Offset(p) => {
                    let p = s.resolve_alias(p);
                    (0..link_count)
                        .filter_map(|k| {
                            let link = p.at(k * sz::PATH_LINK);
                            Some((s.u16_at(link, 4).ok()?, s.f32_at(link, 0).ok()?))
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            Some(PathNode {
                kind,
                origin: [f(20)?, f(24)?, f(28)?],
                yaw: f(32)?,
                radius: f(44)?,
                spawnflags: i32::from(s.u16_at(n, 4).ok()?),
                keys,
                links,
            })
        })
        .map(Option::unwrap_or_default)
        .collect()
}
