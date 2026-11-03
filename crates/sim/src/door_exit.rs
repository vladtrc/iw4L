//! Shared static clearance check for catalog generation and trimmed match load.
use crate::world_graph::AtlasDoor;
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|v| v * s)
}

pub fn validate_flight(
    door: &AtlasDoor,
    thickness: f32,
    trace: impl Fn([f32; 3], [f32; 3]) -> trace_iw4::Trace,
) -> Result<(), String> {
    let edge = door.width * 0.5 - 4.0;
    if !edge.is_finite()
        || edge < 0.0
        || !door.height.is_finite()
        || door.height < 8.0
        || !thickness.is_finite()
        || thickness <= 0.0
    {
        return Err("opening narrower than flight clearance".into());
    }
    let tangent = [-door.normal[1], door.normal[0], 0.0];
    let reach = thickness * 0.5 + 8.0;
    for across in [-edge, 0.0, edge] {
        for height in [
            4.0,
            door.height * 0.25,
            door.height * 0.5,
            door.height * 0.75,
            door.height - 4.0,
        ] {
            let center = add(door.base, add(scale(tangent, across), [0.0, 0.0, height]));
            for side in [-1.0, 1.0] {
                let from = add(center, scale(door.normal, side * reach));
                let to = add(center, scale(door.normal, -side * reach));
                let hit = trace(from, to);
                if hit.startsolid != 0 || hit.allsolid != 0 || hit.fraction < 1.0 {
                    return Err("flight corridor intersects solid geometry".into());
                }
            }
        }
    }
    Ok(())
}

pub fn validate(
    door: &AtlasDoor,
    thickness: f32,
    trace: impl Fn([f32; 3], [f32; 3]) -> trace_iw4::Trace,
) -> Result<(), String> {
    // A player must reach open ground on each naturally accessible side, not
    // "a straight lane is empty": furniture in front of a door, a raised floor
    // behind it or a curb at one jamb still leave a way out. Flood the stand
    // positions of the player hull on a grid around the opening, with the
    // walking step envelope, and require one connected region that reaches
    // open ground past the wall. A one-sided endpoint never searches its rear.
    const CELL: f32 = 4.0;
    const STEP: f32 = 18.0;
    let tangent = [-door.normal[1], door.normal[0], 0.0];
    let edge = door.width * 0.5 - 16.0;
    if edge < 0.0 {
        return Err("opening narrower than player hull".into());
    }
    // Past the wall: half its thickness plus the hull and a stride.
    let goal = thickness * 0.5 + 32.0;
    let reach = ((goal + 16.0) / CELL).ceil() as i32;
    let across = ((edge + 48.0) / CELL).ceil() as i32;
    let point = |s: i32, u: i32| {
        add(
            door.base,
            add(
                scale(door.normal, s as f32 * CELL),
                scale(tangent, u as f32 * CELL),
            ),
        )
    };
    // Grounded positions over (s, u) reachable from feet height `from`.
    // A small lift must start free, then trace down onto walkable support.
    let stand = |s: i32, u: i32, from: f32| {
        let p = point(s, u);
        [0.0, 0.5, 1.0, 2.0, 4.0, 8.0, 12.0, 16.0, STEP]
            .into_iter()
            .filter_map(|lift| {
                let hit = trace([p[0], p[1], from + lift], [p[0], p[1], from - STEP - 1.0]);
                (hit.startsolid == 0
                    && hit.allsolid == 0
                    && hit.fraction < 1.0
                    && hit.normal[2] >= 0.7)
                    .then(|| add(hit.endpos, [0.0, 0.0, 0.5]))
            })
            .collect::<Vec<_>>()
    };
    // Lift, traverse, lower: the same step envelope as walking, without
    // permitting a hull to tunnel through a wall.
    let walk = |start: [f32; 3], end: [f32; 3]| {
        let height = start[2].max(end[2]);
        let raised_start = [start[0], start[1], height];
        let raised_end = [end[0], end[1], height];
        [
            (start, raised_start),
            (raised_start, raised_end),
            (raised_end, end),
        ]
        .into_iter()
        .all(|(a, b)| {
            let hit = trace(a, b);
            hit.startsolid == 0 && hit.allsolid == 0 && hit.fraction >= 1.0
        })
    };
    let width = (2 * across + 1) as usize;
    let index = |s: i32, u: i32| (s + reach) as usize * width + (u + across) as usize;
    let mut found_start = false;
    let mut best = (0.0f32, 0.0f32);
    let lanes = (edge / CELL).floor() as i32;
    for u0 in -lanes..=lanes {
        // Each seed owns its search: a previous seed must not consume a
        // reachable cell without contributing its reach to this component.
        for start in stand(0, u0, door.base[2]) {
            let mut feet: Vec<Vec<[f32; 3]>> = vec![Vec::new(); (2 * reach + 1) as usize * width];
            found_start = true;
            feet[index(0, u0)].push(start);
            let mut queue = std::collections::VecDeque::from([(0i32, u0, start)]);
            let (mut front, mut back) = (0.0f32, 0.0f32);
            while let Some((s, u, here)) = queue.pop_front() {
                let along = s as f32 * CELL;
                front = front.max(along);
                back = back.max(-along);
                if front >= goal && (!door.two_sided || back >= goal) {
                    return Ok(());
                }
                // A short stride can bridge a rounded threshold whose middle
                // offers no walkable support. The whole stride is swept in both
                // directions; it cannot skip a wall or exceed the step limit.
                for (ds, du) in (1..=4)
                    .flat_map(|stride| [(stride, 0), (-stride, 0), (0, stride), (0, -stride)])
                {
                    let (s, u) = (s + ds, u + du);
                    if (!door.two_sided && s < 0) || s.abs() > reach || u.abs() > across {
                        continue;
                    }
                    for next in stand(s, u, here[2]) {
                        // One horizontal cell can have distinct reachable floors.
                        // Keeping only its first height loses the raised side of a
                        // threshold when a lower floor was discovered first.
                        if feet[index(s, u)]
                            .iter()
                            .any(|p| (p[2] - next[2]).abs() < 1.0)
                            || (next[2] - here[2]).abs() > STEP
                            || !walk(here, next)
                            || !walk(next, here)
                        {
                            continue;
                        }
                        feet[index(s, u)].push(next);
                        queue.push_back((s, u, next));
                    }
                }
            }
            let reached = if door.two_sided {
                front.min(back)
            } else {
                front
            };
            if reached > best.0 {
                best = (reached, front.max(back));
            }
        }
    }
    if !found_start {
        return Err("no free hull/walkable support in the aperture".into());
    }
    Err(format!(
        "no walkable way out on required sides: best region reaches {:.0}u and {:.0}u of {goal:.0}u",
        best.0, best.1
    ))
}
