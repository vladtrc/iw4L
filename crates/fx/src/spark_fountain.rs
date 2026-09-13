use fx_iw4::{
    FX_SPARK_FOUNTAIN_CELLS, FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX, FX_SPARK_FOUNTAIN_HANDLE_NONE,
    FX_SPARK_FOUNTAIN_INTEGRATE_BUDGET, FX_SPARK_FOUNTAIN_INTEGRATE_CELLS,
    fx_spark_fountain_accel_from_gravity, fx_spark_fountain_cone_dir,
    fx_spark_fountain_handle_for_slot, fx_spark_fountain_integrate_cell,
    fx_spark_fountain_integrate_cell_begin, fx_spark_fountain_isotropic_dir,
    fx_spark_fountain_mark_ready, fx_spark_fountain_slot_for_handle,
    fx_spark_fountain_spark_n_clamped, fx_spark_fountain_speed, fx_spark_fountain_spray_dir,
    fx_spark_fountain_update_keyframe_cursor, msvcrt_rand,
};

use crate::system::FxSystemHost;

#[derive(Clone, Copy, Debug, Default)]
pub struct FxSparkFountainCell {
    pub times: [f32; 4],
    pub origins: [[f32; 3]; 4],
    pub vels: [[f32; 3]; 4],
}

#[derive(Clone, Debug)]
pub struct FxSparkFountainClusterSlot {
    pub occupied: bool,
    pub next_free: u16,
    pub ready: u8,
    pub spark_n: u8,
    pub write_spark: u8,
    pub keyframe: u8,
    pub gravity: f32,
    pub bounce_frac: f32,
    pub bounce_rand: f32,
    pub spark_length: f32,
    pub loop_time: f32,
    pub boost_time: f32,
    pub boost_factor: f32,
    pub mesh_idx: [u16; FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX as usize],
}

impl Default for FxSparkFountainClusterSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            next_free: FX_SPARK_FOUNTAIN_HANDLE_NONE,
            ready: 0,
            spark_n: 0,
            write_spark: 0,
            keyframe: 0,
            gravity: 0.0,
            bounce_frac: 0.0,
            bounce_rand: 0.0,
            spark_length: 0.0,
            loop_time: 0.0,
            boost_time: 0.0,
            boost_factor: 0.0,
            mesh_idx: [FX_SPARK_FOUNTAIN_HANDLE_NONE; FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX as usize],
        }
    }
}

#[derive(Clone, Debug)]
pub struct FxSparkFountainMeshSlot {
    pub occupied: bool,
    pub next_free: u16,
    pub cells: [FxSparkFountainCell; FX_SPARK_FOUNTAIN_CELLS as usize],
}

impl Default for FxSparkFountainMeshSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            next_free: FX_SPARK_FOUNTAIN_HANDLE_NONE,
            cells: [FxSparkFountainCell::default(); FX_SPARK_FOUNTAIN_CELLS as usize],
        }
    }
}

pub(crate) fn alloc_spark_fountain(host: &mut FxSystemHost) -> Option<u16> {
    let dense = host.spark_fountain_first_free?;
    if dense >= host.spark_fountains.len() {
        host.spark_fountain_first_free = None;
        host.spark_fountain_alloc_failures = host.spark_fountain_alloc_failures.saturating_add(1);
        return None;
    }
    let next = host.spark_fountains[dense].next_free;
    host.spark_fountain_first_free = if next == FX_SPARK_FOUNTAIN_HANDLE_NONE {
        None
    } else {
        Some(next as usize)
    };
    host.spark_fountains[dense] = FxSparkFountainClusterSlot {
        occupied: true,
        next_free: FX_SPARK_FOUNTAIN_HANDLE_NONE,
        ..FxSparkFountainClusterSlot::default()
    };
    host.spark_fountain_live_count = host.spark_fountain_live_count.saturating_add(1);
    Some(fx_spark_fountain_handle_for_slot(dense as u32))
}

fn alloc_spark_fountain_mesh(host: &mut FxSystemHost) -> Option<u16> {
    let dense = host.spark_fountain_mesh_first_free?;
    if dense >= host.spark_fountain_meshes.len() {
        host.spark_fountain_mesh_first_free = None;
        return None;
    }
    let next = host.spark_fountain_meshes[dense].next_free;
    host.spark_fountain_mesh_first_free = if next == FX_SPARK_FOUNTAIN_HANDLE_NONE {
        None
    } else {
        Some(next as usize)
    };
    host.spark_fountain_meshes[dense] = FxSparkFountainMeshSlot {
        occupied: true,
        next_free: FX_SPARK_FOUNTAIN_HANDLE_NONE,
        cells: [FxSparkFountainCell::default(); FX_SPARK_FOUNTAIN_CELLS as usize],
    };
    Some(dense as u16)
}

fn free_spark_fountain_mesh(host: &mut FxSystemHost, dense: u16) {
    let d = dense as usize;
    if d >= host.spark_fountain_meshes.len() || !host.spark_fountain_meshes[d].occupied {
        return;
    }
    let next_free = host
        .spark_fountain_mesh_first_free
        .map(|x| x as u16)
        .unwrap_or(FX_SPARK_FOUNTAIN_HANDLE_NONE);
    host.spark_fountain_meshes[d] = FxSparkFountainMeshSlot {
        occupied: false,
        next_free,
        ..FxSparkFountainMeshSlot::default()
    };
    host.spark_fountain_mesh_first_free = Some(d);
}

pub(crate) fn free_spark_fountain(host: &mut FxSystemHost, handle: u16) {
    let Some(dense) = fx_spark_fountain_slot_for_handle(handle) else {
        return;
    };
    if dense >= host.spark_fountains.len() || !host.spark_fountains[dense].occupied {
        return;
    }
    for idx in host.spark_fountains[dense].mesh_idx {
        if idx != FX_SPARK_FOUNTAIN_HANDLE_NONE {
            free_spark_fountain_mesh(host, idx);
        }
    }
    let next_free = host
        .spark_fountain_first_free
        .map(|d| d as u16)
        .unwrap_or(FX_SPARK_FOUNTAIN_HANDLE_NONE);
    host.spark_fountains[dense] = FxSparkFountainClusterSlot {
        occupied: false,
        next_free,
        ..FxSparkFountainClusterSlot::default()
    };
    host.spark_fountain_first_free = Some(dense);
    host.spark_fountain_live_count = host.spark_fountain_live_count.saturating_sub(1);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FountainSpray {
    Ready,
    MeshFull,
}

pub(crate) fn spray_spark_fountain(
    host: &mut FxSystemHost,
    handle: u16,
    origin: [f32; 3],
    cone_axis: [f32; 3],
    flags: i32,
    spark_count: i32,
    vel_min: f32,
    vel_max: f32,
    vel_cone_frac: f32,
    gravity: f32,
    spark_length: f32,
    loop_time: f32,
    boost_time: f32,
    boost_factor: f32,
    bounce_frac: f32,
    bounce_rand: f32,
) -> FountainSpray {
    let Some(dense) = fx_spark_fountain_slot_for_handle(handle) else {
        return FountainSpray::MeshFull;
    };
    if dense >= host.spark_fountains.len() || !host.spark_fountains[dense].occupied {
        return FountainSpray::MeshFull;
    }
    let spark_n = fx_spark_fountain_spark_n_clamped(spark_count);
    let mut meshes = [FX_SPARK_FOUNTAIN_HANDLE_NONE; FX_SPARK_FOUNTAIN_CLUSTER_MESH_MAX as usize];
    let mut i = 0u8;
    while i < spark_n {
        match alloc_spark_fountain_mesh(host) {
            Some(m) => meshes[i as usize] = m,
            None => {
                for idx in meshes {
                    if idx != FX_SPARK_FOUNTAIN_HANDLE_NONE {
                        free_spark_fountain_mesh(host, idx);
                    }
                }
                return FountainSpray::MeshFull;
            }
        }
        i = i.saturating_add(1);
    }
    let mut hold = host.spark_fountain_holdrand;
    i = 0;
    while i < spark_n {
        let mesh = meshes[i as usize] as usize;
        let mut cell = 0u32;
        while cell < FX_SPARK_FOUNTAIN_CELLS {
            let rz = msvcrt_rand(&mut hold);
            let ry = msvcrt_rand(&mut hold);
            let rx = msvcrt_rand(&mut hold);
            let dir = fx_spark_fountain_spray_dir(cone_axis, [rx, ry, rz], vel_cone_frac);
            let speed = fx_spark_fountain_speed(msvcrt_rand(&mut hold), vel_min, vel_max);
            host.spark_fountain_meshes[mesh].cells[cell as usize] = FxSparkFountainCell {
                times: [0.0; 4],
                origins: [origin, [0.0; 3], [0.0; 3], [0.0; 3]],
                vels: [
                    [dir[0] * speed, dir[1] * speed, dir[2] * speed],
                    [0.0; 3],
                    [0.0; 3],
                    [0.0; 3],
                ],
            };
            cell = cell.saturating_add(1);
        }
        i = i.saturating_add(1);
    }
    host.spark_fountain_holdrand = hold;
    let (ready, write_spark, keyframe) = fx_spark_fountain_mark_ready(spark_n, flags);
    host.spark_fountains[dense].spark_n = spark_n;
    host.spark_fountains[dense].mesh_idx = meshes;
    host.spark_fountains[dense].ready = ready;
    host.spark_fountains[dense].write_spark = write_spark;
    host.spark_fountains[dense].keyframe = keyframe;
    host.spark_fountains[dense].gravity = gravity;
    host.spark_fountains[dense].bounce_frac = bounce_frac;
    host.spark_fountains[dense].bounce_rand = bounce_rand;
    host.spark_fountains[dense].spark_length = spark_length;
    host.spark_fountains[dense].loop_time = loop_time;
    host.spark_fountains[dense].boost_time = boost_time;
    host.spark_fountains[dense].boost_factor = boost_factor;
    FountainSpray::Ready
}

pub(crate) fn emit_spark_fountain_custom_cells(
    host: &FxSystemHost,
    handle: u16,
    camera: [f32; 3],
    size0: f32,
    age_msec: i32,
) -> Vec<[fx_iw4::GfxPosTexVertex; 8]> {
    use fx_iw4::{
        FX_SPARK_FOUNTAIN_HANDLE_NONE, fx_spark_fountain_atlas_uv, fx_spark_fountain_boost,
        fx_spark_fountain_cell_verts, fx_spark_fountain_generate_ribbon,
        fx_spark_fountain_slot_for_handle, fx_spark_fountain_wrap_loop_time,
    };
    let mut out = Vec::new();
    let Some(dense) = fx_spark_fountain_slot_for_handle(handle) else {
        return out;
    };
    let Some(cluster) = host.spark_fountains.get(dense).filter(|c| c.occupied) else {
        return out;
    };
    if cluster.spark_length == 0.0 || size0 == 0.0 {
        return out;
    }
    let (warped, length_scale) =
        fx_spark_fountain_boost(cluster.boost_time, cluster.boost_factor, age_msec as f32);
    let length = cluster.spark_length * length_scale;
    if length == 0.0 {
        return out;
    }
    let wrapped = fx_spark_fountain_wrap_loop_time(warped, cluster.loop_time);
    let t0 = wrapped - length;
    let mut s = 0u8;
    while s < cluster.spark_n {
        let mesh = cluster.mesh_idx[s as usize];
        if mesh != FX_SPARK_FOUNTAIN_HANDLE_NONE
            && (mesh as usize) < host.spark_fountain_meshes.len()
        {
            let slot = &host.spark_fountain_meshes[mesh as usize];
            if slot.occupied {
                let mut cell = 0u32;
                while cell < FX_SPARK_FOUNTAIN_CELLS {
                    let c = slot.cells[cell as usize];
                    let Some((ribbon, uv_v_lerp)) = fx_spark_fountain_generate_ribbon(
                        c.times,
                        c.origins,
                        c.vels,
                        cluster.gravity,
                        t0,
                        wrapped,
                    ) else {
                        cell = cell.saturating_add(1);
                        continue;
                    };
                    let uv = fx_spark_fountain_atlas_uv(0, 0, cell);
                    out.push(fx_spark_fountain_cell_verts(
                        ribbon, camera, size0, uv[0], uv[1], uv[2], uv[3], uv_v_lerp,
                    ));
                    cell = cell.saturating_add(1);
                }
            }
        }
        s = s.saturating_add(1);
    }
    out
}

pub(crate) fn update_spark_fountain(
    host: &mut FxSystemHost,
    handle: u16,
    mut on_trace: impl FnMut([f32; 3], [f32; 3]) -> (f32, [f32; 3]),
) {
    let Some(dense) = fx_spark_fountain_slot_for_handle(handle) else {
        return;
    };
    if dense >= host.spark_fountains.len() || !host.spark_fountains[dense].occupied {
        return;
    }
    let write = host.spark_fountains[dense].write_spark;
    let spark_n = host.spark_fountains[dense].spark_n;
    let keyframe = host.spark_fountains[dense].keyframe;
    if write == spark_n {
        return;
    }
    let mesh = host.spark_fountains[dense].mesh_idx[write as usize];
    if (mesh as usize) >= host.spark_fountain_meshes.len()
        || !host.spark_fountain_meshes[mesh as usize].occupied
    {
        return;
    }
    let begin = fx_spark_fountain_integrate_cell_begin(keyframe) as usize;
    let end =
        (begin + FX_SPARK_FOUNTAIN_INTEGRATE_CELLS as usize).min(FX_SPARK_FOUNTAIN_CELLS as usize);
    let gravity = host.spark_fountains[dense].gravity;
    let bounce_frac = host.spark_fountains[dense].bounce_frac;
    let bounce_rand = host.spark_fountains[dense].bounce_rand;
    let accel = fx_spark_fountain_accel_from_gravity(gravity);
    let mut hold = host.spark_fountain_holdrand;
    let mut i = begin;
    while i < end {
        let origin = host.spark_fountain_meshes[mesh as usize].cells[i].origins[0];
        let vel = host.spark_fountain_meshes[mesh as usize].cells[i].vels[0];
        let (times, origins, vels) = fx_spark_fountain_integrate_cell(
            origin,
            vel,
            accel,
            FX_SPARK_FOUNTAIN_INTEGRATE_BUDGET,
            bounce_frac,
            |start, end| {
                let (fraction, normal) = on_trace(start, end);
                if bounce_rand == 0.0 {
                    (fraction, normal)
                } else {
                    let rz = msvcrt_rand(&mut hold);
                    let ry = msvcrt_rand(&mut hold);
                    let rx = msvcrt_rand(&mut hold);
                    let cube = fx_spark_fountain_isotropic_dir([rx, ry, rz]);
                    (
                        fraction,
                        fx_spark_fountain_cone_dir(normal, cube, bounce_rand),
                    )
                }
            },
        );
        host.spark_fountain_meshes[mesh as usize].cells[i] = FxSparkFountainCell {
            times,
            origins,
            vels,
        };
        i = i.saturating_add(1);
    }
    host.spark_fountain_holdrand = hold;
    let (write, keyframe) = fx_spark_fountain_update_keyframe_cursor(write, keyframe, spark_n);
    host.spark_fountains[dense].write_spark = write;
    host.spark_fountains[dense].keyframe = keyframe;
}
