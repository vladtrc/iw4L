//! Ordinary locomotion searches yield between collision queries. Script natives
//! that return a route synchronously continue to use RouteSearch directly.
use super::*;
use routes::{RouteSearch, direct_supported_settled};

const MAX_CONNECTOR_QUERIES_PER_TICK: usize = 16;
const RESTART_DISTANCE: f32 = 30.0; // one actor diameter

#[derive(Clone)]
pub(super) struct Search {
    origin: [f32; 3],
    positions: Vec<[f32; 3]>,
    candidates: Vec<usize>,
    candidate: usize,
    route: RouteSearch,
    closed: Vec<bool>,
    active: Option<(usize, usize)>,
    started: bool,
    ends: Option<Vec<usize>>,
}
impl std::fmt::Debug for Search {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Search")
            .field("origin", &self.origin)
            .field(
                "visited",
                &self.closed.iter().filter(|closed| **closed).count(),
            )
            .field("candidate", &self.candidate)
            .finish()
    }
}
enum Progress {
    Pending,
    Failed,
    Found(Navigation),
}
impl Search {
    fn new(world: &World, origin: [f32; 3]) -> Self {
        let graph = &world.resource::<RoundScript>().path_nodes;
        let positions: Vec<_> = graph.nodes().iter().map(|node| node.origin).collect();
        let mut candidates: Vec<_> = (0..positions.len())
            .filter(|&i| !graph.links[i].is_empty())
            .collect();
        candidates.sort_by(|&a, &b| {
            distance(origin, positions[a])
                .total_cmp(&distance(origin, positions[b]))
                .then(a.cmp(&b))
        });
        let count = positions.len();
        Self {
            origin,
            positions,
            candidates,
            candidate: 0,
            route: RouteSearch {
                start: 0,
                costs: vec![f32::INFINITY; count],
                previous: vec![None; count],
            },
            closed: vec![false; count],
            active: None,
            started: false,
            ends: None,
        }
    }
    fn advance(
        &mut self,
        world: &mut World,
        id: u64,
        goal: &goals::Goal,
        current_origin: [f32; 3],
    ) -> Progress {
        let mut connector_queries = 0;
        'queries: while connector_queries < MAX_CONNECTOR_QUERIES_PER_TICK {
            if !self.started {
                let Some(&node) = self.candidates.get(self.candidate) else {
                    return Progress::Failed;
                };
                self.candidate += 1;
                connector_queries += 1;
                if direct_supported_settled(world, id, self.origin, self.positions[node], false) {
                    self.route.start = node as u16;
                    self.route.costs[node] = distance(self.origin, self.positions[node]);
                    self.started = true;
                    self.candidate = 0;
                }
                continue;
            }
            if let Some(ends) = &self.ends {
                let Some(&end) = ends.get(self.candidate) else {
                    return Progress::Failed;
                };
                self.candidate += 1;
                connector_queries += 1;
                if direct_supported_settled(world, id, self.positions[end], goal.position, false) {
                    let Some(path) = self.route.to_node(world, end as u16, goal) else {
                        return Progress::Failed;
                    };
                    // Collision and hazards may have changed while the search yielded.
                    // Validate the chosen route against the current scene before use.
                    if route_supported_with_negotiation(
                        world,
                        id,
                        &path,
                        current_origin,
                        false,
                        true,
                    ) {
                        return Progress::Found(path);
                    }
                    return Progress::Failed;
                }
                continue;
            }
            // Advance past exhausted adjacency lists without spending trace budget.
            let (at, link) = loop {
                if let Some((at, edge)) = self.active {
                    if let Some(link) = world.resource::<RoundScript>().path_nodes.links[at]
                        .get(edge)
                        .cloned()
                    {
                        self.active = Some((at, edge + 1));
                        break (at, link);
                    }
                    self.active = None;
                }
                let next = (0..self.positions.len())
                    .filter(|&i| !self.closed[i] && self.route.costs[i].is_finite())
                    .min_by(|&a, &b| {
                        self.route.costs[a]
                            .total_cmp(&self.route.costs[b])
                            .then(a.cmp(&b))
                    });
                let Some(next) = next else {
                    let mut ends: Vec<_> = (0..self.positions.len())
                        .filter(|&i| self.route.costs[i].is_finite())
                        .collect();
                    ends.sort_by(|&a, &b| {
                        (self.route.costs[a] + distance(self.positions[a], goal.position))
                            .total_cmp(
                                &(self.route.costs[b] + distance(self.positions[b], goal.position)),
                            )
                            .then(a.cmp(&b))
                    });
                    self.ends = Some(ends);
                    self.candidate = 0;
                    continue 'queries;
                };
                self.closed[next] = true;
                self.active = Some((next, 0));
            };
            let next = usize::from(link.node);
            if next >= self.positions.len() || self.closed[next] || link.disconnect_count != 0 {
                continue;
            }
            let from = self.positions[at];
            let to = self.positions[next];
            let usable = authored_edge_allowed(world, id, at as u16, link.node, false, true);
            let cost = self.route.costs[at] + distance(from, to);
            if usable && cost < self.route.costs[next] {
                self.route.costs[next] = cost;
                self.route.previous[next] = Some(at as u16);
            }
        }
        Progress::Pending
    }
}
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    glam::Vec3::from_array(a).distance(glam::Vec3::from_array(b))
}
pub(super) fn advance(
    world: &mut World,
    id: u64,
    origin: [f32; 3],
    goal: &goals::Goal,
) -> Navigation {
    let mut pending = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let path = &mut runtime.actors.get_mut(&id).unwrap().navigation;
        if path.goal.as_ref() == Some(goal) {
            path.pending.take()
        } else {
            None
        }
    }
    .filter(|search| {
        distance(search.origin, origin) <= RESTART_DISTANCE
            && search.positions.len() == world.resource::<RoundScript>().path_nodes.nodes().len()
    });
    super::super::super::presence::settle_collision(world);
    if pending.is_none() {
        if direct_supported_settled(world, id, origin, goal.position, false) {
            return Navigation {
                goal: Some(goal.clone()),
                points: vec![goal.position],
                ..Default::default()
            };
        }
        pending = Some(Box::new(Search::new(world, origin)));
    }
    let mut search = pending.unwrap();
    match search.advance(world, id, goal, origin) {
        Progress::Found(path) => path,
        Progress::Pending => Navigation {
            goal: Some(goal.clone()),
            pending: Some(search),
            ..Default::default()
        },
        Progress::Failed => Navigation {
            goal: Some(goal.clone()),
            ..Default::default()
        },
    }
}
