use bevy::prelude::ResMut;
use net::AuthorityWorld;

use super::{EXPAND_CAP, NavEdge, NavGraph, PathError, RouteSearch, TraversalKind, nearest_within};

const PATH_NODE_SNAP_IN: f32 = 512.0;

struct PathNav {
    graph: NavGraph,
    traversals: Vec<(usize, usize)>,
}

impl sim::script::ActorNav for PathNav {
    fn route(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        blocked: &[sim::script::Bounds],
    ) -> Option<Vec<[f32; 3]>> {
        let pruned;
        let graph = if blocked.is_empty() {
            &self.graph
        } else {
            let mut copy = self.graph.clone();
            let nodes = &copy.nodes;
            for (from, edges) in copy.adj.iter_mut().enumerate() {
                edges.retain(|edge| {
                    !blocked
                        .iter()
                        .any(|b| crosses(nodes[from], nodes[usize::from(edge.to)], b))
                });
            }
            pruned = copy;
            &pruned
        };
        let start = nearest_within(graph, from, PATH_NODE_SNAP_IN)?;
        let goal = nearest_within(graph, to, PATH_NODE_SNAP_IN)?;
        let mut search = RouteSearch::begin(graph, &[(start, 0.0)], &[goal]);
        let steps = loop {
            let mut budget = EXPAND_CAP;
            match search.advance(graph, &mut budget) {
                Err(PathError::BudgetExhausted { .. }) => {}
                result => break result.ok()?,
            }
        };
        Some(steps.iter().map(|step| step.position).chain([to]).collect())
    }

    fn traversal_at(&self, at: [f32; 3]) -> Option<(usize, usize)> {
        self.traversals
            .iter()
            .copied()
            .find(|&(start, _)| self.graph.nodes[start] == at)
    }
}

const LINK_HEIGHT: f32 = 36.0;

fn crosses(a: [f32; 3], b: [f32; 3], (mins, maxs): &sim::script::Bounds) -> bool {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for k in 0..3 {
        let lift = if k == 2 { LINK_HEIGHT } else { 0.0 };
        let (p, d) = (a[k] + lift, b[k] - a[k]);
        if d.abs() < f32::EPSILON {
            if p < mins[k] || p > maxs[k] {
                return false;
            }
            continue;
        }
        let (mut near, mut far) = ((mins[k] - p) / d, (maxs[k] - p) / d);
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        t0 = t0.max(near);
        t1 = t1.min(far);
        if t0 > t1 {
            return false;
        }
    }
    true
}

fn key<'a>(node: &'a sim::script::PathNode, name: &str) -> Option<&'a str> {
    node.keys
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.as_str())
}

fn traversals(nodes: &[sim::script::PathNode]) -> Vec<(usize, usize)> {
    nodes
        .iter()
        .enumerate()
        .filter(|(_, begin)| begin.kind == "Begin")
        .filter_map(|(b, begin)| {
            let target = key(begin, "target")?;
            let end = nodes
                .iter()
                .position(|end| end.kind == "End" && key(end, "targetname") == Some(target))?;
            Some((b, end))
        })
        .collect()
}

fn path_nav(nodes: &[sim::script::PathNode]) -> std::sync::Arc<dyn sim::script::ActorNav> {
    let n = nodes.len();
    let mut adj: Vec<Vec<NavEdge>> = nodes
        .iter()
        .map(|node| {
            node.links
                .iter()
                .filter(|(to, _)| usize::from(*to) < n)
                .map(|&(to, cost)| NavEdge {
                    to,
                    cost,
                    kind: TraversalKind::Walk,
                })
                .collect()
        })
        .collect();
    let traversals = traversals(nodes);
    for &(begin, end) in &traversals {
        if adj[begin].iter().any(|edge| usize::from(edge.to) == end) {
            continue;
        }
        let cost = nodes[begin]
            .origin
            .iter()
            .zip(nodes[end].origin)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt();
        adj[begin].push(NavEdge {
            to: end as u16,
            cost,
            kind: TraversalKind::Walk,
        });
    }
    std::sync::Arc::new(PathNav {
        graph: NavGraph {
            nodes: nodes.iter().map(|node| node.origin).collect(),
            component: vec![0; n],
            adj,
            ..Default::default()
        },
        traversals,
    })
}

pub(crate) fn install_actor_nav(world: Option<ResMut<AuthorityWorld>>) {
    let Some(mut world) = world else {
        return;
    };
    if let Some(nodes) = world.0.path_nodes_awaiting_nav() {
        let edges: usize = nodes.iter().map(|node| node.links.len()).sum();
        diag::info!(
            Sim,
            "actors: route graph over {} path nodes, {edges} links",
            nodes.len()
        );
        world.0.install_actor_nav(path_nav(&nodes));
    }
}
