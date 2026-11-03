//! Doors between the worlds of a projectile_flight atlas, as the camera sees them.
//!
//! World `i` of the atlas is drawn at `x + i·PORTAL_SLAB` (`sim::projectile_flight`).
//! The camera is in world `C`, the slab of its eye, and the picture is drawn
//! in `C`'s coordinates: every other world is carried onto `C` by the rigid
//! moves of the doors that lead to it from `C`, found breadth first. Each
//! distinct route unfolds its own view, including doors within the same world.
//!
//! One frame tree supplies native DPVS membership and view cameras. Exact
//! passes draw each view through the source aperture's stencil region.

use glam::{DMat4, DVec3, DVec4};

use asset_core::atlas::{WorldId, WorldLayout};

pub use asset_core::atlas::PortalDoor;

/// Two doors that are one. `flip` sends motion along `a.normal` out against
/// `b.normal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalPair {
    pub a: PortalDoor,
    pub b: PortalDoor,
    pub flip: bool,
}

impl PortalPair {
    /// The yaw in radians that turns `a`'s side onto `b`'s.
    pub fn yaw(&self) -> f64 {
        let (bx, by) = if self.flip {
            (-self.b.normal[0], -self.b.normal[1])
        } else {
            (self.b.normal[0], self.b.normal[1])
        };
        f64::from(by).atan2(f64::from(bx))
            - f64::from(self.a.normal[1]).atan2(f64::from(self.a.normal[0]))
    }
}

/// An opening in the camera world's coordinates: whoever looks through it
/// from `worlds[0]` sees `worlds[1]`, and back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalOpening {
    /// Bottom centre.
    pub base: [f32; 3],
    /// Horizontal unit vector across the opening.
    pub across: [f32; 3],
    /// Unit vector up the opening.
    pub up: [f32; 3],
    pub width: f32,
    pub height: f32,
    pub worlds: [usize; 2],
}

/// The world surfaces of one appended atlas world, as indices of the merged
/// world's surfaces; the surfaces of no layer are world 0's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalSurfaces {
    pub world: usize,
    pub first: u32,
    pub end: u32,
}

/// The atlas seen from the camera's world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PortalView {
    pub camera_world: usize,
    pub pairs: std::sync::Arc<[PortalPair]>,
    /// Current posed world bounds by scene entity; sorted by entity number.
    pub entity_bounds: std::sync::Arc<[(u32, dpvs_iw4::Bounds)]>,
    pub plan: Option<crate::portal_plan::PortalPlan>,
    pub occlusion: Option<crate::portal_occlusion::PortalOcclusionFrame>,
    pub query_keys: Vec<Vec<u32>>,
    pub surfaces: std::sync::Arc<[PortalSurfaces]>,
    /// By world: the move from its drawn coordinates (at its slab) into the
    /// camera world's, or `None` when no door leads there.
    pub moves: Vec<Option<DMat4>>,
    pub openings: Vec<PortalOpening>,
    pub(crate) opening_blocked: Vec<bool>,
    pub blocked_exits: std::sync::Arc<[[bool; 2]]>,
    /// Physical world of each unfolded view; a world may appear several times.
    pub worlds: Vec<usize>,
    /// Native DPVS membership for each unfolded view, produced by scene culling.
    pub visibility: Vec<Option<PortalDrawVisibility>>,
    physical_worlds: usize,
    parents: Vec<Option<(usize, usize)>>,
}

/// Native surface and placement membership; indices remain in each physical
/// world's domain. Shared arrays survive extraction without copying per draw.
#[derive(Clone, Default, PartialEq)]
pub struct PortalDrawVisibility {
    pub surface_base: usize,
    pub surfaces: std::sync::Arc<[u8]>,
    pub smodel_base: usize,
    pub smodels: std::sync::Arc<[u8]>,
    /// Sorted global brush IDs admitted by this view's native cells/frustum.
    pub bmodels: std::sync::Arc<[u16]>,
}

impl std::fmt::Debug for PortalDrawVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortalDrawVisibility")
            .field("surface_base", &self.surface_base)
            .field(
                "surfaces",
                &self.surfaces.iter().filter(|&&v| v != 0).count(),
            )
            .field("smodel_base", &self.smodel_base)
            .field("smodels", &self.smodels.iter().filter(|&&v| v != 0).count())
            .field("bmodels", &self.bmodels)
            .finish()
    }
}

impl PortalDrawVisibility {
    pub fn admits(&self, kind: &crate::RetainedDrawKind, run_surfaces: &[u16]) -> bool {
        match *kind {
            crate::RetainedDrawKind::World {
                surf,
                run,
                run_off,
                brush_model,
                ..
            } => {
                // Dynamic brush placements use their own scene visibility. Their
                // source BSP surface is not a stationary DPVS membership key.
                if let Some(model) = brush_model {
                    return self.bmodels.binary_search(&model).is_ok();
                }
                (0..usize::from(run.max(1))).any(|offset| {
                    let surface = if run > 1 {
                        run_surfaces
                            .get(run_off as usize + offset)
                            .copied()
                            .map(usize::from)
                    } else {
                        Some(usize::from(surf))
                    };
                    surface
                        .and_then(|i| i.checked_sub(self.surface_base))
                        .and_then(|i| self.surfaces.get(i))
                        .is_some_and(|&v| v != 0)
                })
            }
            crate::RetainedDrawKind::Smodel {
                visibility_slot, ..
            } => (visibility_slot as usize)
                .checked_sub(self.smodel_base)
                .and_then(|i| self.smodels.get(i))
                .is_some_and(|&v| v != 0),
            // Animated entities and effects are supplied by scene visibility;
            // sky must not be tested against a static placement's bit.
            _ => true,
        }
    }
}

fn slab(world: usize) -> DVec3 {
    DVec3::from_array(WorldLayout.origin(WorldId(world)).map(f64::from))
}

fn vec(v: [f32; 3]) -> DVec3 {
    DVec3::new(f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}

fn array(v: DVec3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

/// The move that takes the drawn coordinates of `entered`'s world onto
/// `exit`'s, `entered` landing on `exit` turned by `yaw`.
fn carry(entered: &PortalDoor, exit: &PortalDoor, yaw: f64) -> DMat4 {
    DMat4::from_translation(slab(exit.world.index()) + vec(exit.base))
        * DMat4::from_rotation_z(yaw)
        * DMat4::from_translation(-(slab(entered.world.index()) + vec(entered.base)))
}

pub struct PortalAtlas<'a> {
    pub pairs: &'a [PortalPair],
    pub world_count: usize,
    pub surfaces: std::sync::Arc<[PortalSurfaces]>,
}

#[derive(Clone, Copy, Debug)]
pub struct PortalCamera {
    pub eye: [f32; 3],
    pub clip_from_world: glam::Mat4,
    pub viewport: [f32; 4],
}

impl PortalView {
    pub fn new(atlas: PortalAtlas<'_>, eye: [f32; 3]) -> Self {
        Self::build(atlas, eye, 2, None, &[], None, &mut |_, _| true)
    }

    pub fn new_render(
        atlas: PortalAtlas<'_>,
        camera: PortalCamera,
        settings: crate::portal_plan::PortalRenderSettings,
        visible: &mut dyn FnMut(&Self, usize) -> bool,
    ) -> Self {
        Self::new_render_with_blocked(atlas, camera, settings, &[], None, visible)
    }

    pub fn new_render_with_blocked(
        atlas: PortalAtlas<'_>,
        camera: PortalCamera,
        settings: crate::portal_plan::PortalRenderSettings,
        blocked_exits: &[[bool; 2]],
        occlusion: Option<crate::portal_occlusion::PortalOcclusionFrame>,
        visible: &mut dyn FnMut(&Self, usize) -> bool,
    ) -> Self {
        let camera_world = WorldLayout
            .world_at(camera.eye)
            .0
            .min(atlas.world_count.saturating_sub(1));
        let camera_only = settings.only == Some(usize::MAX) || settings.only == Some(camera_world);
        let depth = if camera_only {
            0
        } else {
            usize::from(settings.depth.min(3)) + 1
        };
        Self::build(
            atlas,
            camera.eye,
            depth,
            Some((camera.clip_from_world, camera.viewport, settings)),
            blocked_exits,
            occlusion,
            visible,
        )
    }

    fn build(
        atlas: PortalAtlas<'_>,
        eye: [f32; 3],
        max_depth: usize,
        render: Option<(
            glam::Mat4,
            [f32; 4],
            crate::portal_plan::PortalRenderSettings,
        )>,
        blocked_exits: &[[bool; 2]],
        occlusion: Option<crate::portal_occlusion::PortalOcclusionFrame>,
        visible: &mut dyn FnMut(&Self, usize) -> bool,
    ) -> Self {
        let PortalAtlas {
            pairs,
            world_count,
            surfaces,
        } = atlas;
        let camera_world = WorldLayout
            .world_at(eye)
            .0
            .min(world_count.saturating_sub(1));
        let mut moves = vec![None; world_count];
        moves[camera_world] = Some(DMat4::IDENTITY);
        let mut worlds: Vec<_> = (0..world_count).collect();
        let mut parents = vec![None; world_count];
        let mut openings = Vec::new();
        let mut routes = vec![vec![camera_world as u32]; world_count];
        let mut query_keys = Vec::new();
        let mut opening_blocked = Vec::new();
        let mut projections = Vec::new();
        let mut rects = vec![None; world_count];
        if let Some((_, vp, _)) = render {
            rects[camera_world] = Some([vp[0] as u32, vp[1] as u32, vp[2] as u32, vp[3] as u32]);
        }
        let mut queue = std::collections::VecDeque::from([(camera_world, Vec::<usize>::new())]);
        while let Some((view, path)) = queue.pop_front() {
            if path.len() >= max_depth {
                continue;
            }
            let here = moves[view].expect("a queued view has a move");
            let world = worlds[view];
            for (pair_id, pair) in pairs.iter().enumerate() {
                if path.contains(&pair_id) {
                    continue;
                }
                let yaw = pair.yaw();
                for (side, (near, far, far_onto_near)) in
                    [(&pair.a, &pair.b, -yaw), (&pair.b, &pair.a, yaw)]
                        .into_iter()
                        .enumerate()
                {
                    if near.world.index() != world
                        || far.world.index() >= world_count
                        || moves.len() >= 255
                    {
                        continue;
                    }

                    let local_eye = here.inverse().transform_point3(vec(eye)) - slab(world);
                    let front = (local_eye - vec(near.base)).dot(vec(near.normal)) > 0.0;
                    if !asset_core::atlas::portal_side_allowed(
                        near.two_sided,
                        far.two_sided,
                        pair.flip,
                        front,
                    ) {
                        continue;
                    }
                    // A nested door outside an ancestor aperture cannot contribute
                    // any pixels. Do this before unfolding another whole world.
                    let center = here.transform_point3(
                        slab(world) + vec(near.base) + DVec3::Z * f64::from(near.height) * 0.5,
                    );
                    let radius = (near.width.hypot(near.height) * 0.5) as f64;
                    if !route_sphere_visible(
                        camera_world,
                        &parents,
                        &openings,
                        view,
                        vec(eye),
                        center,
                        radius,
                    ) {
                        continue;
                    }
                    let normal = vec(near.normal);
                    let across = DVec3::new(-normal.y, normal.x, 0.0).normalize_or_zero();
                    let mut opening = PortalOpening {
                        base: array(here.transform_point3(slab(world) + vec(near.base))),
                        across: array(here.transform_vector3(across)),
                        up: array(here.transform_vector3(DVec3::Z)),
                        width: near.width,
                        height: near.height,
                        worlds: [view, 0],
                    };
                    let projection = if let Some((clip, vp, _)) = render {
                        let Some(mut p) =
                            crate::portal_plan::project_aperture(&opening, clip, eye, vp)
                        else {
                            continue;
                        };
                        let Some(rect) =
                            rects[view].and_then(|r| crate::portal_plan::intersect_rect(r, p.rect))
                        else {
                            continue;
                        };
                        p.rect = rect;
                        Some(p)
                    } else {
                        None
                    };
                    let moved = here * carry(far, near, far_onto_near);
                    // Keep one route per aperture, even if two routes have the
                    // same transform: their ancestor masks and visibility differ.
                    let next = if moves[far.world.index()].is_none() {
                        far.world.index()
                    } else {
                        moves.push(None);
                        worlds.push(far.world.index());
                        parents.push(None);
                        rects.push(None);
                        routes.push(Vec::new());
                        moves.len() - 1
                    };
                    moves[next] = Some(moved);
                    parents[next] = Some((view, openings.len()));
                    opening.worlds[1] = next;
                    rects[next] = projection.map(|p| p.rect);
                    projections.push(projection);
                    let mut route = routes[view].clone();
                    route.push((pair_id * 2 + side) as u32);
                    routes[next] = route.clone();
                    query_keys.push(route);
                    openings.push(opening);
                    let blocked = blocked_exits
                        .get(pair_id)
                        .is_some_and(|doors| doors[1 - side]);
                    opening_blocked.push(blocked);
                    if !blocked {
                        let mut next_path = path.clone();
                        next_path.push(pair_id);
                        queue.push_back((next, next_path));
                    }
                }
            }
        }
        let mut out = Self {
            camera_world,
            pairs: pairs.into(),
            entity_bounds: Default::default(),
            plan: None,
            occlusion,
            query_keys,
            surfaces,
            visibility: vec![None; moves.len()],
            moves,
            openings,
            opening_blocked,
            blocked_exits: blocked_exits.into(),
            worlds,
            physical_worlds: world_count,
            parents,
        };
        if let Some((_, vp, settings)) = render {
            out.plan = Some(crate::portal_plan::PortalPlan::new(
                &out,
                &projections,
                vp,
                settings,
                eye,
                visible,
            ));
        }
        out
    }

    /// The camera frustum narrowed by each ancestor opening, expressed in this
    /// view's physical world. All planes keep points with n·p+d >= 0.
    pub fn clip_planes_in(
        &self,
        view: usize,
        eye: [f32; 3],
        frustum: &[[f32; 4]],
    ) -> Option<Vec<[f32; 4]>> {
        let transform = self.moves.get(view).copied().flatten()?.transpose();
        let eye = vec(eye);
        let mut planes: Vec<DVec4> = frustum
            .iter()
            .map(|p| DVec4::new(p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64))
            .collect();
        let mut current = view;
        while current != self.camera_world
            && !self
                .plan
                .as_ref()
                .is_some_and(|p| p.settings.only.is_some())
        {
            let (parent, index) = self.parents.get(current).copied().flatten()?;
            let o = self.openings[index];
            let base = vec(o.base);
            let across = vec(o.across) * f64::from(o.width) * 0.5;
            let up = vec(o.up) * f64::from(o.height);
            let corners = [
                base - across,
                base + across,
                base + across + up,
                base - across + up,
            ];
            let middle = base + up * 0.5;
            let plane_normal = vec(o.across).cross(vec(o.up)).normalize_or_zero();
            let delta = eye - base;
            let in_near_aperture = plane_normal.dot(delta).abs() < 1.0
                && vec(o.across).dot(delta).abs() <= f64::from(o.width) * 0.5
                && vec(o.up).dot(delta) >= 0.0
                && vec(o.up).dot(delta) <= f64::from(o.height);
            if !in_near_aperture {
                for i in 0..4 {
                    let mut n = (corners[i] - eye)
                        .cross(corners[(i + 1) % 4] - eye)
                        .normalize_or_zero();
                    if n.dot(middle - eye) < 0.0 {
                        n = -n;
                    }
                    planes.push(n.extend(-n.dot(eye) + 0.001));
                }
            }
            let mut n = vec(o.across).cross(vec(o.up)).normalize_or_zero();
            if n.dot(eye - base) > 0.0 {
                n = -n;
            }
            planes.push(n.extend(-n.dot(base) + 0.001));
            current = parent;
        }
        Some(
            planes
                .into_iter()
                .filter_map(|p| {
                    let p = transform * p;
                    let length = p.truncate().length();
                    (length > 1e-8).then(|| (p / length).as_vec4().to_array())
                })
                .collect(),
        )
    }

    /// Positive half-space beyond this view's exit. Coordinates are those of
    /// the destination slab; a physical/debug root has no exit plane.
    pub fn exit_plane_in(&self, view: usize, eye: [f32; 3]) -> Option<DVec4> {
        if view == self.plan.as_ref().map_or(self.camera_world, |p| p.root) {
            return None;
        }
        let (_, index) = self.parents.get(view).copied().flatten()?;
        let opening = self.openings[index];
        let base = vec(opening.base);
        let mut normal = vec(opening.across)
            .cross(vec(opening.up))
            .normalize_or_zero();
        if normal.dot(vec(eye) - base) > 0.0 {
            normal = -normal;
        }
        let plane = normal.extend(-normal.dot(base) + 0.01);
        let transformed = self.moves.get(view).copied().flatten()?.transpose() * plane;
        let length = transformed.truncate().length();
        (length > 1e-8).then_some(transformed / length)
    }

    /// Seed DPVS on the destination side of the exit plane. The virtual eye
    /// itself can be behind an unrelated wall and is not a valid cell seed.
    pub fn seed_in(&self, view: usize, eye: [f32; 3]) -> Option<[f32; 3]> {
        if view == self.camera_world {
            return Some(eye);
        }
        let (_, index) = self.parents.get(view).copied().flatten()?;
        let o = self.openings[index];
        let base = vec(o.base);
        let mut n = vec(o.across).cross(vec(o.up)).normalize_or_zero();
        if n.dot(vec(eye) - base) > 0.0 {
            n = -n;
        }
        let center = base + vec(o.up) * f64::from(o.height) * 0.5 + n * 0.5;
        Some(array(
            self.moves
                .get(view)
                .copied()
                .flatten()?
                .inverse()
                .transform_point3(center),
        ))
    }

    /// All views of one physical world, as GPU draw tags (0 is untagged).
    pub fn is_selected(&self, index: usize) -> bool {
        self.plan
            .as_ref()
            .is_none_or(|p| p.selected.get(index).copied().unwrap_or(false))
    }

    pub fn view_depth(&self, mut index: usize) -> Option<u8> {
        let root = self
            .plan
            .as_ref()
            .map_or(self.camera_world, |plan| plan.root);
        for depth in 0..self.moves.len() {
            if index == root {
                return u8::try_from(depth).ok();
            }
            index = self.parents.get(index).copied().flatten()?.0;
        }
        None
    }

    pub fn draw_views(&self, world: usize) -> Vec<u8> {
        self.worlds
            .iter()
            .enumerate()
            .filter(|(i, w)| **w == world && self.moves[*i].is_some() && self.is_selected(*i))
            .map(|(i, _)| (i + 1) as u8)
            .collect()
    }

    pub fn seen_in(&self, view: usize, point: [f32; 3]) -> Option<[f32; 3]> {
        Some(array(
            self.moves
                .get(view)
                .copied()
                .flatten()?
                .transform_point3(vec(point)),
        ))
    }

    /// Only the camera world's geometry belongs in its dynamic sun map.
    pub fn camera_sun_caster(&self, kind: &crate::RetainedDrawKind) -> bool {
        let tag = match *kind {
            crate::RetainedDrawKind::World { surf, .. } => self.surface_tag(u32::from(surf)),
            crate::RetainedDrawKind::Smodel {
                owner_world_from_local,
                ..
            } => self.point_tag(owner_world_from_local.w_axis.truncate().to_array()),
            crate::RetainedDrawKind::XModel {
                world_from_local, ..
            } => self.point_tag(world_from_local.w_axis.truncate().to_array()),
            _ => return true,
        };
        tag as usize == self.camera_world + 1
    }

    /// The draw tag of world surface `surf`: its world plus one.
    pub fn surface_tag(&self, surf: u32) -> u8 {
        let world = self
            .surfaces
            .iter()
            .find(|layer| (layer.first..layer.end).contains(&surf))
            .map_or(0, |layer| layer.world);
        world as u8 + 1
    }

    /// Where the eye stands in `world`'s drawn coordinates, carried back
    /// through the doors; `None` when no door leads there.
    pub fn eye_in(&self, world: usize, eye: [f32; 3]) -> Option<[f32; 3]> {
        let carried = self.moves.get(world).copied().flatten()?;
        Some(array(carried.inverse().transform_point3(vec(eye))))
    }

    /// `point`, drawn in the world of its slab, where the camera world sees
    /// it through the doors; `None` when no door leads there.
    pub fn seen(&self, point: [f32; 3]) -> Option<[f32; 3]> {
        let world = WorldLayout.world_at(point).0;
        let carried = self.moves.get(world).copied().flatten()?;
        Some(array(carried.transform_point3(vec(point))))
    }

    /// The world the segment `from`..`to` of the camera world ends in: it
    /// changes world at each opening it passes, as the exact passes' sight
    /// lines do.
    pub fn world_along(&self, from: [f32; 3], to: [f32; 3]) -> usize {
        let (mut current, mut passed) = (self.camera_world, 0.0);
        while let Some((t, next)) = self.next_opening(from, to, current, passed) {
            (current, passed) = (next, t);
        }
        current
    }

    /// The first opening the segment `from`..`to` of the camera world
    /// passes out of the camera world: the fraction along it and the world
    /// beyond.
    pub fn first_opening(&self, from: [f32; 3], to: [f32; 3]) -> Option<(f32, usize)> {
        self.next_opening(from, to, self.camera_world, 0.0)
            .map(|(t, world)| (t as f32, world))
    }

    fn next_opening(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        current: usize,
        passed: f64,
    ) -> Option<(f64, usize)> {
        let (start, ray) = (vec(from), vec(to) - vec(from));
        let (mut best, mut next) = (1.0, None);
        for (index, opening) in self.openings.iter().enumerate() {
            if self.opening_blocked.get(index).copied().unwrap_or(false)
                || !opening.worlds.contains(&current)
            {
                continue;
            }
            let (across, up) = (vec(opening.across), vec(opening.up));
            let normal = across.cross(up);
            let facing = ray.dot(normal);
            if facing.abs() < 1e-8 {
                continue;
            }
            let t = (vec(opening.base) - start).dot(normal) / facing;
            if t <= passed || t >= best {
                continue;
            }
            let q = start + ray * t - vec(opening.base);
            let height = q.dot(up);
            if q.dot(across).abs() > f64::from(opening.width) * 0.5
                || height < 0.0
                || height > f64::from(opening.height)
            {
                continue;
            }
            best = t;
            next = Some(opening.worlds[usize::from(opening.worlds[0] == current)]);
        }
        next.map(|world| (best, world))
    }

    /// A direction in `world`'s drawn coordinates as the camera world sees
    /// it; `None` when no door leads there.
    pub fn seen_direction(&self, world: usize, direction: [f32; 3]) -> Option<[f32; 3]> {
        let carried = self.moves.get(world).copied().flatten()?;
        Some(array(carried.transform_vector3(vec(direction))))
    }

    /// A camera-world pose at `origin` taken into `world`'s drawn
    /// coordinates: the origin there and the yaw, in degrees, its angles
    /// turn by.
    pub fn pose_in(&self, world: usize, origin: [f32; 3]) -> Option<([f32; 3], f32)> {
        let into = self.moves.get(world).copied().flatten()?.inverse();
        let turn = into.x_axis.y.atan2(into.x_axis.x).to_degrees();
        Some((array(into.transform_point3(vec(origin))), turn as f32))
    }

    /// Conservative aperture culling for a sphere in a connected atlas world.
    /// The sphere must intersect every doorway cone along the unfolded route.
    pub fn sphere_visible(&self, eye: [f32; 3], center: [f32; 3], radius: f32) -> bool {
        !self.visible_centers(eye, center, radius).is_empty()
    }

    pub fn sphere_visible_in(
        &self,
        view: usize,
        eye: [f32; 3],
        center: [f32; 3],
        radius: f32,
    ) -> bool {
        self.seen_in(view, center).is_some_and(|center| {
            route_sphere_visible(
                self.camera_world,
                &self.parents,
                &self.openings,
                view,
                vec(eye),
                vec(center),
                f64::from(radius),
            )
        })
    }

    /// Visible placements of a physical sphere across all of its door views.
    pub fn visible_centers(&self, eye: [f32; 3], center: [f32; 3], radius: f32) -> Vec<[f32; 3]> {
        let world = self.point_tag(center).saturating_sub(1) as usize;
        self.worlds
            .iter()
            .enumerate()
            .filter_map(|(view, &physical)| {
                if physical != world || !self.is_selected(view) {
                    return None;
                }
                let center = self.seen_in(view, center)?;
                if !route_sphere_visible(
                    self.camera_world,
                    &self.parents,
                    &self.openings,
                    view,
                    vec(eye),
                    vec(center),
                    f64::from(radius),
                ) {
                    return None;
                }
                Some(center)
            })
            .collect()
    }

    /// The draw tag of a model placed at `origin`: the world of its slab
    /// plus one.
    pub fn point_tag(&self, origin: [f32; 3]) -> u8 {
        let last = self.physical_worlds.saturating_sub(1);
        WorldLayout.world_at(origin).0.min(last) as u8 + 1
    }
}

/// Test every ancestor cone in camera coordinates. A sphere keeps this
/// conservative for doors and geometry that straddle an aperture edge.
fn route_sphere_visible(
    root: usize,
    parents: &[Option<(usize, usize)>],
    openings: &[PortalOpening],
    mut view: usize,
    eye: DVec3,
    center: DVec3,
    radius: f64,
) -> bool {
    while view != root {
        let Some((from, index)) = parents.get(view).copied().flatten() else {
            return false;
        };
        let o = openings[index];
        let base = vec(o.base);
        let across = vec(o.across) * f64::from(o.width) * 0.5;
        let up = vec(o.up) * f64::from(o.height);
        let corners = [
            base - across,
            base + across,
            base + across + up,
            base - across + up,
        ];
        let middle = base + up * 0.5;
        for edge in 0..4 {
            let mut normal = (corners[edge] - eye)
                .cross(corners[(edge + 1) % 4] - eye)
                .normalize_or_zero();
            if normal.dot(middle - eye) < 0.0 {
                normal = -normal;
            }
            if normal.dot(center - eye) < -radius {
                return false;
            }
        }
        // Reject spheres wholly between the eye and the doorway as well.
        let normal = vec(o.across).cross(vec(o.up)).normalize_or_zero();
        let eye_side = normal.dot(eye - base).signum();
        if normal.dot(center - base) * eye_side > radius {
            return false;
        }
        view = from;
    }
    true
}
