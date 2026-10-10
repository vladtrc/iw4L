use fastfile_t5::{ScriptStrings, ZonePtr, ZoneStream, size as sz};

/// A node of a map's authored path network.
#[derive(Clone, Debug, PartialEq)]
pub struct PathNode {
    pub kind: PathNodeKind,
    pub origin: [f32; 3],
    pub yaw: f32,
    pub targetname: String,
    pub target: String,
    /// The traversal animscript a negotiation begin node names.
    pub animscript: String,
    pub links: Vec<PathLink>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathNodeKind {
    Path,
    NegotiationBegin,
    NegotiationEnd,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathLink {
    pub node: u16,
    pub distance: f32,
    /// Crossing this link is a traversal played by the begin node's animscript.
    pub negotiation: bool,
}

/// The path network a T5 game world was compiled with, nodes in map order.
pub fn path_network_t5(s: &ZoneStream<'_>, strings: &ScriptStrings) -> Vec<PathNode> {
    let Some(geometry) = s.path_data() else {
        return Vec::new();
    };
    let Some(nodes) = geometry.nodes else {
        return Vec::new();
    };
    let text = |node, off| {
        s.u16_at(node, off)
            .ok()
            .and_then(|id| strings.get(s, id))
            .unwrap_or("")
            .to_owned()
    };
    let float = |p, off| s.f32_at(p, off).unwrap_or(0.0);
    (0..geometry.node_count)
        .map(|index| {
            let node = nodes.at(index * sz::PATH_NODE);
            let kind = match s.u16_at(node, sz::PATH_NODE_TYPE_OFF).unwrap_or(0) {
                1 => PathNodeKind::Path,
                17 => PathNodeKind::NegotiationBegin,
                18 => PathNodeKind::NegotiationEnd,
                other => PathNodeKind::Other(other),
            };
            let count = s
                .u16_at(node, sz::PATH_NODE_TOTAL_LINK_COUNT_OFF)
                .unwrap_or(0) as usize;
            let links = match s.ptr_at(node, sz::PATH_NODE_LINKS_OFF) {
                Ok(ZonePtr::Offset(links)) => (0..count)
                    .filter_map(|k| {
                        let link = links.at(k * sz::PATH_LINK);
                        Some(PathLink {
                            node: s.u16_at(link, sz::PATH_LINK_NODE_OFF).ok()?,
                            distance: float(link, sz::PATH_LINK_DISTANCE_OFF),
                            negotiation: s.u8_at(link, sz::PATH_LINK_NEGOTIATION_OFF).ok()? != 0,
                        })
                    })
                    .filter(|link| (link.node as usize) < geometry.node_count)
                    .collect(),
                _ => Vec::new(),
            };
            PathNode {
                kind,
                origin: [
                    float(node, sz::PATH_NODE_ORIGIN_OFF),
                    float(node, sz::PATH_NODE_ORIGIN_OFF + 4),
                    float(node, sz::PATH_NODE_ORIGIN_OFF + 8),
                ],
                yaw: float(node, sz::PATH_NODE_ANGLE_OFF),
                targetname: text(node, sz::PATH_NODE_TARGETNAME_OFF),
                target: text(node, sz::PATH_NODE_TARGET_OFF),
                animscript: text(node, sz::PATH_NODE_ANIMSCRIPT_OFF),
                links,
            }
        })
        .collect()
}
