use super::*;

#[derive(Clone, Debug)]
pub(in super::super) struct RouteSearch {
    pub(super) start: u16,
    pub(super) costs: Vec<f32>,
    pub(super) previous: Vec<Option<u16>>,
}

pub(in super::super) fn direct_supported(
    world: &mut World,
    id: u64,
    from: [f32; 3],
    to: [f32; 3],
    ignore_suppression: bool,
) -> bool {
    (ignore_suppression
        || !world.resource::<RoundScript>().actors[&id]
            .suppression
            .blocks_path(from, to))
        && supported_target(world, id, from, to).is_some()
}
pub(super) fn direct_supported_settled(
    world: &mut World,
    id: u64,
    from: [f32; 3],
    to: [f32; 3],
    ignore_suppression: bool,
) -> bool {
    (ignore_suppression
        || !world.resource::<RoundScript>().actors[&id]
            .suppression
            .blocks_path(from, to))
        && supported_target_settled(world, id, from, to).is_some()
}

impl RouteSearch {
    pub(in super::super) fn build(
        world: &mut World,
        id: u64,
        origin: [f32; 3],
        ignore_suppression: bool,
    ) -> Option<Self> {
        Self::build_with_negotiation(world, id, origin, ignore_suppression, false)
    }
    fn build_with_negotiation(
        world: &mut World,
        id: u64,
        origin: [f32; 3],
        ignore_suppression: bool,
        allow_negotiation: bool,
    ) -> Option<Self> {
        super::super::super::presence::settle_collision(world);
        let nodes = world.resource::<RoundScript>().path_nodes.nodes().to_vec();
        let links = world.resource::<RoundScript>().path_nodes.links.clone();
        let mut candidates: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| !links[*i].is_empty())
            .map(|(i, node)| {
                (
                    glam::Vec3::from_array(node.origin)
                        .distance_squared(glam::Vec3::from_array(origin)),
                    i,
                )
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let start = candidates
            .into_iter()
            .find(|(_, i)| {
                direct_supported_settled(world, id, origin, nodes[*i].origin, ignore_suppression)
            })?
            .1;
        let mut costs = vec![f32::INFINITY; nodes.len()];
        let mut previous = vec![None; nodes.len()];
        let mut closed = vec![false; nodes.len()];
        costs[start] =
            glam::Vec3::from_array(origin).distance(glam::Vec3::from_array(nodes[start].origin));
        while let Some(at) = (0..nodes.len())
            .filter(|&i| !closed[i] && costs[i].is_finite())
            .min_by(|&a, &b| costs[a].total_cmp(&costs[b]).then(a.cmp(&b)))
        {
            closed[at] = true;
            for link in &links[at] {
                let next = usize::from(link.node);
                if next >= nodes.len() || closed[next] || link.disconnect_count != 0 {
                    continue;
                }
                let usable = authored_edge_allowed(
                    world,
                    id,
                    at as u16,
                    link.node,
                    ignore_suppression,
                    allow_negotiation,
                );
                if !usable {
                    continue;
                }
                let cost = costs[at]
                    + glam::Vec3::from_array(nodes[at].origin)
                        .distance(glam::Vec3::from_array(nodes[next].origin));
                if cost < costs[next] {
                    costs[next] = cost;
                    previous[next] = Some(at as u16);
                }
            }
        }
        Some(Self {
            start: start as u16,
            costs,
            previous,
        })
    }
    pub(in super::super) fn cost(&self, node: u16) -> Option<f32> {
        self.costs
            .get(usize::from(node))
            .copied()
            .filter(|cost| cost.is_finite())
    }
    pub(in super::super) fn to_node(
        &self,
        world: &World,
        node: u16,
        goal: &goals::Goal,
    ) -> Option<Navigation> {
        self.cost(node)?;
        let mut route = vec![node];
        while *route.last()? != self.start {
            route.push(self.previous[usize::from(*route.last()?)]?);
        }
        route.reverse();
        let graph = &world.resource::<RoundScript>().path_nodes;
        let mut points: Vec<_> = route
            .iter()
            .map(|&node| graph.nodes()[usize::from(node)].origin)
            .collect();
        points.push(goal.position);
        Some(Navigation {
            goal: Some(goal.clone()),
            points,
            links: route.windows(2).map(|p| (p[0], p[1])).collect(),
            nodes: route,
            ..Default::default()
        })
    }
}

pub(in super::super) fn supported_plan(
    world: &mut World,
    id: u64,
    origin: [f32; 3],
    goal: &goals::Goal,
    ignore_suppression: bool,
) -> Option<Navigation> {
    plan(world, id, origin, goal, ignore_suppression, false)
}

fn plan(
    world: &mut World,
    id: u64,
    origin: [f32; 3],
    goal: &goals::Goal,
    ignore_suppression: bool,
    allow_negotiation: bool,
) -> Option<Navigation> {
    if direct_supported(world, id, origin, goal.position, ignore_suppression) {
        return Some(Navigation {
            goal: Some(goal.clone()),
            points: vec![goal.position],
            ..Default::default()
        });
    }
    let search = RouteSearch::build_with_negotiation(
        world,
        id,
        origin,
        ignore_suppression,
        allow_negotiation,
    )?;
    let nodes = world.resource::<RoundScript>().path_nodes.nodes().to_vec();
    let mut ends: Vec<_> = nodes
        .iter()
        .enumerate()
        .filter_map(|(i, node)| {
            search.cost(i as u16).map(|cost| {
                (
                    cost + glam::Vec3::from_array(node.origin)
                        .distance(glam::Vec3::from_array(goal.position)),
                    i,
                )
            })
        })
        .collect();
    ends.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let (_, end) = ends.into_iter().find(|(_, i)| {
        direct_supported_settled(
            world,
            id,
            nodes[*i].origin,
            goal.position,
            ignore_suppression,
        )
    })?;
    search.to_node(world, end as u16, goal)
}
