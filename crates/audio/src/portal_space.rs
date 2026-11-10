use asset_core::atlas::{WorldId, WorldLayout};
use sim::world_graph::{Atlas, AtlasDoor};

#[derive(Clone)]
struct Arrival {
    door: AtlasDoor,
    position: [f32; 3],
    distance: f32,
    direction: [f32; 3],
}

pub(crate) struct PortalSpace {
    world: WorldId,
    arrivals: Vec<Arrival>,
    frames: Option<Vec<asset_core::world_frame::WorldFrame>>,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        .sqrt()
}

fn centre(door: AtlasDoor, frames: Option<&[asset_core::world_frame::WorldFrame]>) -> [f32; 3] {
    if let Some(frame) = frames.and_then(|frames| frames.get(door.world.index())) {
        return frame.point_to_scene([
            door.base[0],
            door.base[1],
            door.base[2] + door.height * 0.5,
        ]);
    }
    let offset = WorldLayout.origin(door.world);
    std::array::from_fn(|axis| {
        door.base[axis] + offset[axis] + if axis == 2 { door.height * 0.5 } else { 0.0 }
    })
}

fn front(
    door: AtlasDoor,
    point: [f32; 3],
    frames: Option<&[asset_core::world_frame::WorldFrame]>,
) -> bool {
    let centre = centre(door, frames);
    let normal = frames
        .and_then(|frames| frames.get(door.world.index()))
        .map_or(door.normal, |frame| frame.vector_to_scene(door.normal));
    (0..3)
        .map(|axis| (point[axis] - centre[axis]) * normal[axis])
        .sum::<f32>()
        >= 0.0
}

impl PortalSpace {
    pub(crate) fn new(atlas: &Atlas, blocked: &[[bool; 2]], listener: [f32; 3]) -> Self {
        let frames = match &atlas.space {
            asset_core::world_frame::SceneSpace::Shared(frames) => Some(frames.clone()),
            _ => None,
        };
        let shared = frames.is_some();
        let world = WorldLayout.world_at(listener);
        let mut arrivals = vec![None; atlas.portals.len() * 2];
        let mut pending = vec![(world, listener, 0.0_f32, None, None)];
        let mut best = vec![f32::INFINITY; atlas.portals.len() * 2];
        while !pending.is_empty() {
            let next = (0..pending.len())
                .min_by(|&a, &b| pending[a].2.total_cmp(&pending[b].2))
                .unwrap();
            let (world, point, travelled, direction, key) = pending.swap_remove(next);
            if key.is_some_and(|key| travelled > best[key]) {
                continue;
            }
            for (index, portal) in atlas.portals.iter().enumerate() {
                if blocked
                    .get(index)
                    .is_some_and(|sides| sides.iter().any(|blocked| *blocked))
                {
                    continue;
                }
                for from_a in [true, false] {
                    let (entered, exit, _) = portal.oriented(from_a);
                    if (!shared && entered.world != world)
                        || !asset_core::atlas::portal_side_allowed(
                            entered.two_sided,
                            exit.two_sided,
                            portal.flip,
                            front(entered, point, frames.as_deref()),
                        )
                    {
                        continue;
                    }
                    let entry = centre(entered, frames.as_deref());
                    let distance = travelled + distance(point, entry);
                    let key = index * 2 + usize::from(from_a);
                    if !distance.is_finite() || distance >= best[key] {
                        continue;
                    }
                    best[key] = distance;
                    let direction = direction.unwrap_or_else(|| {
                        let offset: [f32; 3] =
                            std::array::from_fn(|axis| entry[axis] - listener[axis]);
                        let length = offset.iter().map(|v| v * v).sum::<f32>().sqrt();
                        if length > 0.001 {
                            offset.map(|v| v / length)
                        } else {
                            frames
                                .as_ref()
                                .and_then(|frames| frames.get(entered.world.index()))
                                .map_or(entered.normal, |frame| {
                                    frame.vector_to_scene(entered.normal)
                                })
                                .map(|v| -v)
                        }
                    });
                    let position = centre(exit, frames.as_deref());
                    arrivals[key] = Some(Arrival {
                        door: exit,
                        position,
                        distance,
                        direction,
                    });
                    pending.push((exit.world, position, distance, Some(direction), Some(key)));
                }
            }
        }
        Self {
            world,
            arrivals: arrivals.into_iter().flatten().collect(),
            frames,
        }
    }

    pub(crate) fn position(&self, listener: [f32; 3], source: [f32; 3]) -> Option<[f32; 3]> {
        let world = WorldLayout.world_at(source);
        let source = self
            .frames
            .as_ref()
            .and_then(|frames| frames.get(world.index()))
            .map_or(source, |frame| {
                frame.point_to_scene(std::array::from_fn(|i| {
                    source[i] - WorldLayout.origin(world)[i]
                }))
            });
        let mut best = (self.frames.is_some() || world == self.world)
            .then_some((distance(listener, source), source));
        for arrival in &self.arrivals {
            if (self.frames.is_none() && arrival.door.world != world)
                || (!arrival.door.two_sided && !front(arrival.door, source, self.frames.as_deref()))
            {
                continue;
            }
            let distance = arrival.distance + distance(arrival.position, source);
            if best.is_none_or(|(prior, _)| distance < prior) {
                best = Some((
                    distance,
                    std::array::from_fn(|axis| listener[axis] + arrival.direction[axis] * distance),
                ));
            }
        }
        best.map(|(_, position)| position)
    }
}
