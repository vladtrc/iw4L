use std::collections::HashMap;

use asset_game::FlameStreamView;
use bevy::prelude::*;
use fx_iw4::{FX_TRAIL_TANGENT_PACKED, FxSpriteAtlasUv, trail_pack_normal, trail_pack_texcoord};
use render_fx::FxWorldColorImages;

use crate::adapters::fx::system::{FxCodeMeshBind, code_mesh_bind};
use crate::assemble::drawsurf::MaterialGeneration;
use crate::assemble::drawsurf::tess::fx::FxCodeMeshPlan;

const HOLD_MS: i32 = 150;
const FIRE_CAP: usize = 768;
const TUBE_POINT_CAP: usize = 100;
const TUBE_MIN_SPACING_SQ: f32 = 1.0;
const TUBE_MIN_REACH_SQ: [f32; 2] = [144.0, 14400.0];
const FIRE_GROWTH_CAP: f32 = 70.0;
const MATERIAL_FIRE: usize = 0;
const MATERIAL_FUEL: usize = 4;
const MATERIAL_FLAME: usize = 6;

#[derive(Resource, Default)]
pub(crate) struct FlameStreams {
    streams: HashMap<i32, FlameStream>,
}

struct FlameStream {
    entity: Entity,
    player_view: bool,
    view: FlameStreamView,
    held_until_ms: i32,
    next_chunk_ms: i32,
    stepped_ms: i32,
    tag: Option<([f32; 3], [f32; 3])>,
    chunks: Vec<Chunk>,
    fires: Vec<Fire>,
    rng: u32,
}

struct Chunk {
    pos: Vec3,
    vel: Vec3,
    size: f32,
    growth: f32,
    birth_ms: i32,
    death_ms: i32,
    next_fire_ms: i32,
}

struct Fire {
    pos: Vec3,
    vel: Vec3,
    size: f32,
    growth: f32,
    rot: f32,
    rot_vel: f32,
    birth_ms: i32,
    death_ms: i32,
}

impl FlameStreams {
    pub(crate) fn note_fire(
        &mut self,
        number: i32,
        entity: Entity,
        player_view: bool,
        view: &FlameStreamView,
        now_ms: i32,
    ) {
        let stream = self.streams.entry(number).or_insert_with(|| FlameStream {
            entity,
            player_view,
            view: view.clone(),
            held_until_ms: now_ms,
            next_chunk_ms: now_ms,
            stepped_ms: now_ms,
            tag: None,
            chunks: Vec::new(),
            fires: Vec::new(),
            rng: 0x9e37_79b9 ^ number as u32,
        });
        if stream.player_view != player_view || stream.entity != entity {
            stream.player_view = player_view;
            stream.entity = entity;
            stream.view = view.clone();
            stream.tag = None;
        }
        if stream.held_until_ms < now_ms {
            stream.next_chunk_ms = now_ms;
            stream.tag = None;
        }
        stream.held_until_ms = now_ms + HOLD_MS;
    }

    pub(crate) fn tick(
        &mut self,
        now_ms: i32,
        tag_of: impl Fn(Entity, bool) -> Option<([f32; 3], [f32; 3])>,
    ) {
        self.streams.retain(|_, stream| {
            let tag = tag_of(stream.entity, stream.player_view);
            stream.tick(now_ms, tag);
            stream.held_until_ms >= now_ms || !stream.chunks.is_empty() || !stream.fires.is_empty()
        });
    }

    pub(crate) fn present(
        &self,
        cam: &Transform,
        color_images: &FxWorldColorImages,
        runtime: &MaterialGeneration,
        now_ms: i32,
        plan: &mut FxCodeMeshPlan,
    ) {
        let eye = cam.translation;
        for stream in self.streams.values() {
            let materials = &stream.view.materials;
            if let Some(fire) = materials[MATERIAL_FIRE].as_deref() {
                present_fires(stream, fire, cam, color_images, runtime, now_ms, plan);
            }
            for kind in [TubeKind::Fuel, TubeKind::Flame] {
                stream.present_tube(kind, eye, color_images, runtime, now_ms, plan);
            }
        }
    }
}

impl FlameStream {
    fn rand01(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn rand_signed(&mut self) -> f32 {
        self.rand01() * 2.0 - 1.0
    }

    fn tick(&mut self, now_ms: i32, tag: Option<([f32; 3], [f32; 3])>) {
        if self.held_until_ms >= now_ms
            && let Some(tag) = tag
        {
            self.spawn_chunks(now_ms, tag);
            self.tag = Some(tag);
        } else {
            self.tag = tag.filter(|_| self.held_until_ms >= now_ms);
        }
        let from = self.stepped_ms;
        self.stepped_ms = now_ms;
        let dt = (now_ms - from).max(0) as f32 * 0.001;
        if dt > 0.0 {
            self.step_chunks(now_ms, dt);
            self.step_fires(now_ms, dt);
        }
        self.shed_fires(now_ms);
    }

    fn spawn_chunks(&mut self, now_ms: i32, (origin, forward): ([f32; 3], [f32; 3])) {
        let v = &self.view;
        if v.chunk_speed <= 0.0 || v.chunk_life_s <= 0.0 {
            return;
        }
        let interval_ms =
            ((v.chunk_dist_scalar * v.chunk_size[0] / v.chunk_speed) * 1000.0).max(1.0) as i32;
        let (prev_origin, prev_forward) = self.tag.unwrap_or((origin, forward));
        let span = (now_ms - self.next_chunk_ms).max(1) as f32;
        let mut t = self.next_chunk_ms;
        while t <= now_ms {
            let f = 1.0 - (now_ms - t) as f32 / span;
            let pos = Vec3::from_array(prev_origin).lerp(Vec3::from_array(origin), f);
            let dir = Vec3::from_array(prev_forward)
                .lerp(Vec3::from_array(forward), f)
                .normalize_or(Vec3::from_array(forward));
            let noise = self.rand01();
            let v = &self.view;
            let life_ms = (v.chunk_life_s * 1000.0) as i32;
            let start = (1.0 - v.chunk_size_rand[0] * noise) * v.chunk_size[0];
            let end = (1.0 - v.chunk_size_rand[1] * noise) * v.chunk_size[1];
            let first_fire_ms = (v.fire_first_life_frac * 1000.0) as i32;
            let interval = (v.fire_interval_s[0] * 1000.0) as i32;
            let jitter = (self.rand01() * interval as f32) as i32;
            let mut chunk = Chunk {
                pos,
                vel: dir * self.view.chunk_speed,
                size: start,
                growth: (end - start) / self.view.chunk_life_s,
                birth_ms: t,
                death_ms: t + life_ms,
                next_fire_ms: t + first_fire_ms - jitter,
            };
            let catch_up = (now_ms - t) as f32 * 0.001;
            if catch_up > 0.0 {
                step_chunk(&self.view, &mut chunk, now_ms, catch_up);
            }
            self.chunks.push(chunk);
            t += interval_ms;
        }
        self.next_chunk_ms = t;
    }

    fn step_chunks(&mut self, now_ms: i32, dt: f32) {
        let view = &self.view;
        self.chunks.retain_mut(|chunk| {
            if now_ms >= chunk.death_ms {
                return false;
            }
            step_chunk(view, chunk, now_ms, dt);
            true
        });
    }

    fn shed_fires(&mut self, now_ms: i32) {
        let mut spawns = Vec::new();
        for (index, chunk) in self.chunks.iter_mut().enumerate() {
            let life = (chunk.death_ms - chunk.birth_ms).max(1) as f32;
            while chunk.next_fire_ms <= now_ms {
                let frac = ((chunk.next_fire_ms - chunk.birth_ms) as f32 / life).clamp(0.0, 1.0);
                let interval_s = self.view.fire_interval_s[0]
                    + (self.view.fire_interval_s[1] - self.view.fire_interval_s[0]) * frac;
                chunk.next_fire_ms += ((interval_s * 1000.0) as i32).max(1);
                spawns.push(index);
            }
        }
        for index in spawns {
            self.spawn_fire(index, now_ms);
        }
    }

    fn spawn_fire(&mut self, chunk_index: usize, now_ms: i32) {
        let (r_life, r_speed, r_side, r_z, r_rot) = (
            self.rand01(),
            self.rand01(),
            self.rand_signed(),
            self.rand01(),
            self.rand_signed(),
        );
        let v = &self.view;
        let chunk = &self.chunks[chunk_index];
        let age_ms = now_ms - chunk.birth_ms;
        let chunk_frac = age_ms as f32 / (chunk.death_ms - chunk.birth_ms).max(1) as f32;
        let [early_s, late_s] = v.fire_min_life_age_s;
        let age_s = age_ms as f32 * 0.001;
        let weight = if age_s < early_s {
            v.fire_min_life_frac
        } else if age_s > late_s || late_s <= early_s {
            0.0
        } else {
            v.fire_min_life_frac * (1.0 - (age_s - early_s) / (late_s - early_s))
        };
        let start_frac = chunk_frac.max(weight).min(0.99);
        let life_s = r_life * v.fire_life_rand_s + v.fire_life_s;
        if life_s <= 0.0 {
            return;
        }
        let birth_ms = now_ms - (life_s * start_frac * 1000.0) as i32;
        let size = v.fire_start_size_scale * chunk.size;
        let end_size = v.fire_end_size_scale * chunk.size + v.fire_end_size_add;
        let growth = ((end_size - size) / ((1.0 - start_frac) * life_s)).min(FIRE_GROWTH_CAP);
        let mut vel = chunk.vel
            * (((1.0 - v.fire_speed_scale_rand) + r_speed * v.fire_speed_scale_rand)
                * v.fire_speed_scale);
        let flat = Vec2::new(vel.x, vel.y);
        let flat_len = flat.length();
        if flat_len > 0.0 {
            let side = Vec3::new(flat.y, -flat.x, 0.0) / flat_len;
            vel += side * (r_side * v.fire_vel_add_sideways * flat_len);
        }
        vel.z += r_z * v.fire_vel_add_z_rand + v.fire_vel_add_z;
        let fire = Fire {
            pos: chunk.pos,
            vel,
            size,
            growth,
            rot: 0.0,
            rot_vel: r_rot * v.fire_max_rot_vel,
            birth_ms,
            death_ms: birth_ms + (life_s * 1000.0) as i32,
        };
        if self.fires.len() >= FIRE_CAP {
            self.fires.remove(0);
        }
        self.fires.push(fire);
    }

    fn step_fires(&mut self, now_ms: i32, dt: f32) {
        let v = &self.view;
        self.fires.retain_mut(|fire| {
            if now_ms >= fire.death_ms {
                return false;
            }
            let frac =
                (now_ms - fire.birth_ms) as f32 / (fire.death_ms - fire.birth_ms).max(1) as f32;
            fire.rot += fire.rot_vel * dt;
            fire.size += fire.growth * dt;
            fire.vel *= (1.0 - dt * v.fire_friction).max(0.0);
            let gravity = v.fire_gravity[0] + (v.fire_gravity[1] - v.fire_gravity[0]) * frac;
            fire.vel.z -= gravity * dt;
            fire.pos += fire.vel * dt;
            true
        });
    }
}

fn step_chunk(view: &FlameStreamView, chunk: &mut Chunk, now_ms: i32, dt: f32) {
    let frac = ((now_ms - chunk.birth_ms) as f32 / (chunk.death_ms - chunk.birth_ms).max(1) as f32)
        .clamp(0.0, 1.0);
    chunk.size = (chunk.size + chunk.growth * dt).min(view.chunk_max_size.max(chunk.size));
    chunk.vel *= (1.0 - dt * view.chunk_decel).max(0.0);
    let gravity = view.chunk_gravity[0] + (view.chunk_gravity[1] - view.chunk_gravity[0]) * frac;
    chunk.vel.z -= gravity * dt;
    chunk.pos += chunk.vel * dt;
}

fn material_atlas(runtime: &MaterialGeneration, material: &str) -> [u32; 2] {
    let [rows, cols] = runtime
        .catalog
        .material_for_name(asset_game::material_bind_name(material))
        .map_or([1, 1], |m| m.texture_atlas);
    [u32::from(rows.max(1)), u32::from(cols.max(1))]
}

fn atlas_frame([rows, cols]: [u32; 2], frac: f32) -> FxSpriteAtlasUv {
    let count = rows * cols;
    if count <= 1 {
        return FxSpriteAtlasUv::FULL;
    }
    let frame = ((frac.max(0.0) * count as f32) as u32).min(count - 1);
    FxSpriteAtlasUv {
        s0: (frame % cols) as f32 / cols as f32,
        ds: 1.0 / cols as f32,
        t0: (frame / cols) as f32 / rows as f32,
        dt: 1.0 / rows as f32,
        entry_count: count as u16,
        atlas_index: frame as u16,
    }
}

fn present_fires(
    stream: &FlameStream,
    material: &str,
    cam: &Transform,
    color_images: &FxWorldColorImages,
    runtime: &MaterialGeneration,
    now_ms: i32,
    plan: &mut FxCodeMeshPlan,
) {
    if stream.fires.is_empty() {
        return;
    }
    let FxCodeMeshBind::Ready {
        color,
        sort_key,
        ordinal,
    } = code_mesh_bind(material, color_images, runtime)
    else {
        return;
    };
    let atlas = material_atlas(runtime, material);
    let slot = plan.begin_material_draw(color, sort_key, Some(ordinal));
    for fire in stream.fires.iter().rev() {
        if fire.size <= 0.0 {
            continue;
        }
        let frac = (now_ms - fire.birth_ms) as f32 / (fire.death_ms - fire.birth_ms).max(1) as f32;
        let transform = Transform {
            translation: fire.pos,
            rotation: cam.rotation * Quat::from_rotation_z(fire.rot.to_radians()),
            scale: Vec3::new(2.0 * fire.size, 2.0 * fire.size, 1.0),
        };
        if !plan.push_quad(transform, [255; 4], atlas_frame(atlas, frac)) {
            break;
        }
    }
    plan.end_material_draw(slot);
}

enum TubeKind {
    Fuel,
    Flame,
}

impl FlameStream {
    fn present_tube(
        &self,
        kind: TubeKind,
        eye: Vec3,
        color_images: &FxWorldColorImages,
        runtime: &MaterialGeneration,
        now_ms: i32,
        plan: &mut FxCodeMeshPlan,
    ) {
        let stream = self;
        let (tube, slot) = match kind {
            TubeKind::Fuel => (&stream.view.fuel, MATERIAL_FUEL),
            TubeKind::Flame => (&stream.view.flame, MATERIAL_FLAME),
        };
        let materials = &stream.view.materials;
        let (Some(material), Some(_)) =
            (materials[slot].as_deref(), materials[slot + 1].as_deref())
        else {
            return;
        };

        let Some((origin, _)) = stream.tag else {
            return;
        };
        if tube.length <= 0.0 || stream.chunks.is_empty() {
            return;
        }
        let origin = Vec3::from_array(origin);
        let reach_sq = tube.length * tube.length;
        let min_sq = TUBE_MIN_REACH_SQ[usize::from(!stream.player_view)];
        let mut points = vec![origin];
        let oldest = stream.chunks.len() - 1;
        for (i, chunk) in stream.chunks.iter().rev().enumerate() {
            if points.len() >= TUBE_POINT_CAP {
                break;
            }
            let from_tag = chunk.pos.distance_squared(origin);
            let within = from_tag < reach_sq && (from_tag > min_sq || i == oldest);
            if within && chunk.pos.distance_squared(*points.last().unwrap()) > TUBE_MIN_SPACING_SQ {
                points.push(chunk.pos);
            }
        }
        if points.len() < 2 {
            return;
        }
        let segments = (tube.segments as usize).clamp(2, TUBE_POINT_CAP);
        let points = resample(&points, segments);
        let FxCodeMeshBind::Ready {
            color,
            sort_key,
            ordinal,
        } = code_mesh_bind(material, color_images, runtime)
        else {
            return;
        };
        let loop_ms = (tube.anim_loop_s * 1000.0) as i32;
        let phase = if loop_ms > 0 {
            now_ms.rem_euclid(loop_ms) as f32 / loop_ms as f32
        } else {
            0.0
        };
        let uv = atlas_frame(material_atlas(runtime, material), phase);
        let slot = plan.begin_material_draw(color, sort_key, Some(ordinal));
        let base = plan.mesh.vert_used;
        let last = points.len() - 1;
        let normal = trail_pack_normal([0.0, 0.0, 1.0]);
        for (i, point) in points.iter().enumerate() {
            let dir = points[i.min(last - 1) + 1] - points[i.min(last - 1)];
            // perp = delta x view; the winding needs the first vert on the negative side.
            let side = dir.cross(*point - eye).normalize_or(Vec3::Z);
            let f = i as f32 / last as f32;
            let half = tube.size[0] + (tube.size[1] - tube.size[0]) * f;
            let v = uv.t0 + (1.0 - f) * uv.dt;
            for (sign, u) in [(-1.0, uv.s0), (1.0, uv.s0 + uv.ds)] {
                if !plan.push_trail_vert(
                    (*point + side * (sign * half)).to_array(),
                    [255; 4],
                    trail_pack_texcoord(u, v),
                    normal,
                    FX_TRAIL_TANGENT_PACKED,
                ) {
                    plan.shrink_verts_to(base);
                    plan.end_material_draw(slot);
                    return;
                }
            }
        }
        let index_used = plan.mesh.index_used;
        for i in 0..last as u32 {
            let q = base + i * 2;
            if !plan.extend_indices(&[q + 1, q + 3, q, q, q + 3, q + 2]) {
                plan.shrink_verts_to(base);
                plan.shrink_indices_to(index_used);
                break;
            }
        }
        plan.end_material_draw(slot);
    }
}

fn resample(points: &[Vec3], count: usize) -> Vec<Vec3> {
    let total: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    if total <= 0.0 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(count);
    let mut seg = 0;
    let mut seg_start = 0.0;
    for i in 0..count {
        let target = total * i as f32 / (count - 1) as f32;
        while seg + 1 < points.len() - 1
            && seg_start + points[seg].distance(points[seg + 1]) < target
        {
            seg_start += points[seg].distance(points[seg + 1]);
            seg += 1;
        }
        let len = points[seg].distance(points[seg + 1]).max(f32::EPSILON);
        out.push(points[seg].lerp(
            points[seg + 1],
            ((target - seg_start) / len).clamp(0.0, 1.0),
        ));
    }
    out
}

pub(crate) fn tick_flame_streams(
    mut flames: ResMut<FlameStreams>,
    host: Res<render_fx::HostFxSystem>,
    fpv_bolts: Res<crate::adapters::anim::fpv_present::FpvBoltTargets>,
    world_bolts: Query<(
        &net::CEntityRuntime,
        &crate::adapters::anim::remote_body::RemoteFxBolts,
    )>,
) {
    if flames.streams.is_empty() {
        return;
    }
    let tag = |target: Option<fx::FxBoltTarget>| {
        target.map(|target| (target.orientation.origin, target.orientation.axis[0]))
    };
    flames.tick(host.0.msec_now, |entity, player_view| {
        if player_view {
            tag(fpv_bolts.flash[0])
        } else {
            tag(world_bolts
                .get(entity)
                .ok()
                .filter(|(runtime, _)| runtime.in_next_snap())
                .and_then(|(_, bolts)| bolts.flash))
        }
    });
}
