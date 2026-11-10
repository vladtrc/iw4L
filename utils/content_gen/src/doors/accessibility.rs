//! Native playable access, before any door collision subtraction.
use super::{add, dist, scale};
pub(super) struct NativeAccess {
    world: sim::SimWorld,
    graph: bots::NavGraph,
    reachable: Vec<bool>,
}
fn stand(world: &sim::SimWorld, at: [f32; 3]) -> Option<[f32; 3]> {
    for lift in [48., 24., 8.] {
        let hit = world.trace_world(
            add(at, [0., 0., lift]),
            add(at, [0., 0., -96.]),
            [0.; 3],
            [0.; 3],
            sim::MASK_PLAYER_SOLID,
        );
        if hit.startsolid != 0
            || hit.fraction >= 1.
            || hit.normal[2] < 0.7
            || hit.endpos[2] > at[2] + 32.
        {
            continue;
        }
        let p = add(hit.endpos, [0., 0., 0.5]);
        let fit = world.trace_world(
            p,
            p,
            [-15., -15., 0.],
            [15., 15., 70.],
            sim::MASK_PLAYER_SOLID,
        );
        if fit.startsolid == 0 && fit.allsolid == 0 {
            return Some(p);
        }
    }
    None
}
impl NativeAccess {
    pub fn new(prepared: &assets::PreparedMatch) -> Result<Self, String> {
        let original = prepared
            .clip
            .as_deref()
            .ok_or("native side classification needs collision")?;
        let (brushes, bsp, mesh, cmodels) = session::sim_clip_map(original);
        let seeds_brushes = brushes.clone();
        let mut content = sim::SimContentBuilder::bootstrap();
        content.set_clip_map(brushes, bsp, mesh, cmodels);
        content.set_collision_complete(true);
        let mut world = sim::SimWorld::new();
        world.install_content(content.finish());
        let seeds: Vec<_> = prepared
            .prepared_map
            .spawns
            .iter()
            .filter_map(|s| stand(&world, s.origin))
            .collect();
        if seeds.is_empty() {
            return Err("no supported authored spawns for native sidedness".into());
        }
        let bounds =
            bots::playable_bounds(&seeds_brushes, &seeds).ok_or("no native playable bounds")?;
        let graph = bots::bake_seeded(&world, bounds, 0, &seeds);
        if graph.truncated || graph.nodes.is_empty() {
            return Err("incomplete native side reachability graph".into());
        }
        let mut reachable = vec![false; graph.nodes.len()];
        let mut queue = std::collections::VecDeque::new();
        for p in seeds {
            for (i, &at) in graph.nodes.iter().enumerate() {
                if dist(p, at) < 96.
                    && matches!(
                        bots::supported_walk(&mut world, p, at),
                        Ok(bots::WalkSample::Clear)
                    )
                    && !reachable[i]
                {
                    reachable[i] = true;
                    queue.push_back(i);
                }
            }
        }
        while let Some(i) = queue.pop_front() {
            for edge in &graph.adj[i] {
                let j = edge.to as usize;
                if !reachable[j]
                    && matches!(
                        edge.kind,
                        bots::TraversalKind::Walk | bots::TraversalKind::BreakGlass
                    )
                {
                    reachable[j] = true;
                    queue.push_back(j);
                }
            }
        }
        if !reachable.iter().any(|&v| v) {
            return Err("no spawn-connected native side supports".into());
        }
        Ok(Self {
            world,
            graph,
            reachable,
        })
    }
    /// [negative normal, positive normal], proven in the original map.
    pub fn sides(&mut self, base: [f32; 3], normal: [f32; 3]) -> [bool; 2] {
        let tangent = [-normal[1], normal[0], 0.];
        [-1., 1.].map(|side| {
            for along in [20., 24., 32., 48., 64., 96.] {
                for across in [-8., 0., 8.] {
                    let at = add(
                        base,
                        add(scale(normal, side * along), scale(tangent, across)),
                    );
                    let Some(p) = stand(&self.world, at) else {
                        continue;
                    };
                    if self.graph.nodes.iter().enumerate().any(|(i, &node)| {
                        self.reachable[i]
                            && dist(p, node) < 96.
                            && matches!(
                                bots::supported_walk(&mut self.world, p, node),
                                Ok(bots::WalkSample::Clear)
                            )
                            && matches!(
                                bots::supported_walk(&mut self.world, node, p),
                                Ok(bots::WalkSample::Clear)
                            )
                    }) {
                        return true;
                    }
                }
            }
            false
        })
    }
}
