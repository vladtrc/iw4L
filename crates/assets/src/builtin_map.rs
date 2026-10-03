//! Built-in, asset-free map content.
//!
//! Zone keys of the form `iw4l:<name>` never reach `find_zone_file`: they name
//! content this crate synthesizes in memory instead of walking a `.ff` zone.
//! `iw4l:field` is a walled field arena — a ground slab plus a table of
//! axis-aligned boxes — that produces a complete [`PreparedMatch`]: visible
//! world geometry, collision, spawn entities, and just enough GSC for the
//! stock startup flow (`dm::main`, `codecallback_startgametype`,
//! `codecallback_playerconnect`) to install and spawn a player.
//!
//! The surfaces carry no material: their `packed_draw_surfs` stay zeroed so
//! the extract path routes them to the diagnostic geometry overlay, which
//! shades by normal/color with no SM3 technique set.

use std::sync::Arc;

use asset_material::MaterialDefinitions;
use asset_world::{
    CameraRangeKind, CameraSurfRange, CameraSurfRanges, ClipBrush, ClipCmodel, ClipCollision,
    DpvsWorldData, GfxBrushModelBounds, GfxBrushModelSurfs, IntermissionView, SpawnPoint,
    SurfaceCastsSunShadow, SurfaceDrawFields, WorldBatch, WorldDraw, WorldLightmapGap,
    WorldMeshStats, WorldVertexPayload, world_capture_from_casters,
};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use dpvs_iw4::{Bounds, SurfRange};

use crate::session_load::{PreparedMatch, PreparedWorld};
use crate::{MapFacts, MatchMaterials, PreparedMap};

/// Namespace prefix for maps that ship inside the runtime instead of a zone.
pub const BUILTIN_ZONE_PREFIX: &str = "iw4l:";

const FIELD_MAP: &str = "field";

/// Whether `zone` names built-in content rather than a zone file.
pub fn is_builtin_zone(zone: &str) -> bool {
    builtin_map_stem(zone).is_some()
}

fn builtin_map_stem(zone: &str) -> Option<&'static str> {
    let stem = zone.strip_prefix(BUILTIN_ZONE_PREFIX)?;
    match stem.trim().to_ascii_lowercase().as_str() {
        FIELD_MAP => Some(FIELD_MAP),
        _ => None,
    }
}

/// One axis-aligned solid box in the field layout. `mins`/`maxs` are world
/// coordinates; every box becomes six draw surfaces and one six-plane
/// collision brush.
#[derive(Clone, Copy, Debug)]
struct FieldBox {
    mins: [f32; 3],
    maxs: [f32; 3],
    rgb: [u8; 3],
}

const fn fb(x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, rgb: [u8; 3]) -> FieldBox {
    FieldBox {
        mins: [x0, y0, z0],
        maxs: [x1, y1, z1],
        rgb,
    }
}

const GRASS: [u8; 3] = [74, 112, 54];
const NET: [u8; 3] = [28, 32, 28];
const WALL: [u8; 3] = [206, 208, 200];
const DARK: [u8; 3] = [46, 48, 52];
const CRATE: [u8; 3] = [196, 132, 60];
const PIPE: [u8; 3] = [150, 150, 148];
const BARREL: [u8; 3] = [60, 110, 180];

/// The field layout, transcribed from the aerial reference: a fenced grass
/// field with two mirrored L-walls at the north end, two mirrored shelter
/// walls at the south end, a tall double bunker on the centerline, pipe
/// clusters, crates and scattered low cover.
const FIELD_BOXES: &[FieldBox] = &[
    fb(-896.0, -636.0, -32.0, 896.0, 636.0, 0.0, GRASS),
    fb(-896.0, 604.0, 0.0, 896.0, 636.0, 116.0, NET),
    fb(-896.0, -636.0, 0.0, 896.0, -604.0, 116.0, NET),
    fb(864.0, -636.0, 0.0, 896.0, 636.0, 116.0, NET),
    fb(-896.0, -636.0, 0.0, -864.0, 636.0, 116.0, NET),
    // North L-pair: long wall plus a leg heading north at the inner end and a
    // short hook dropping south at the outer end.
    fb(-660.0, 250.0, 0.0, -200.0, 290.0, 116.0, WALL),
    fb(-232.0, 290.0, 0.0, -200.0, 400.0, 116.0, WALL),
    fb(-660.0, 140.0, 0.0, -628.0, 290.0, 116.0, WALL),
    fb(200.0, 250.0, 0.0, 660.0, 290.0, 116.0, WALL),
    fb(200.0, 290.0, 0.0, 232.0, 400.0, 116.0, WALL),
    fb(628.0, 140.0, 0.0, 660.0, 290.0, 116.0, WALL),
    // South shelters: wide brackets open toward the field.
    fb(-640.0, -560.0, 0.0, -240.0, -520.0, 116.0, WALL),
    fb(-640.0, -560.0, 0.0, -608.0, -460.0, 116.0, WALL),
    fb(-272.0, -560.0, 0.0, -240.0, -460.0, 116.0, WALL),
    fb(240.0, -560.0, 0.0, 640.0, -520.0, 116.0, WALL),
    fb(240.0, -560.0, 0.0, 272.0, -460.0, 116.0, WALL),
    fb(608.0, -560.0, 0.0, 640.0, -460.0, 116.0, WALL),
    // Center double bunker: two wide drums joined by a narrow spine — the
    // tallest thing on the field.
    fb(-110.0, 40.0, 0.0, 110.0, 100.0, 148.0, DARK),
    fb(-110.0, -100.0, 0.0, 110.0, -40.0, 148.0, DARK),
    fb(-36.0, -40.0, 0.0, 36.0, 40.0, 148.0, DARK),
    // Pipe clusters north of center (2x2 stacks of upright pipes).
    fb(-190.0, 150.0, 0.0, -164.0, 176.0, 64.0, PIPE),
    fb(-156.0, 150.0, 0.0, -130.0, 176.0, 64.0, PIPE),
    fb(-190.0, 184.0, 0.0, -164.0, 210.0, 64.0, PIPE),
    fb(-156.0, 184.0, 0.0, -130.0, 210.0, 64.0, PIPE),
    fb(60.0, 150.0, 0.0, 86.0, 176.0, 64.0, PIPE),
    fb(94.0, 150.0, 0.0, 120.0, 176.0, 64.0, PIPE),
    fb(60.0, 184.0, 0.0, 86.0, 210.0, 64.0, PIPE),
    fb(94.0, 184.0, 0.0, 120.0, 210.0, 64.0, PIPE),
    // Twin round bunkers south of center.
    fb(-178.0, -478.0, 0.0, -122.0, -422.0, 84.0, DARK),
    fb(8.0, -478.0, 0.0, 64.0, -422.0, 84.0, DARK),
    // Crates.
    fb(-200.0, 480.0, 0.0, -148.0, 532.0, 52.0, CRATE),
    fb(380.0, 430.0, 0.0, 432.0, 482.0, 52.0, CRATE),
    fb(-610.0, -540.0, 0.0, -558.0, -488.0, 52.0, CRATE),
    fb(740.0, -540.0, 0.0, 792.0, -488.0, 52.0, CRATE),
    // Thin cover walls mid-field.
    fb(-520.0, -80.0, 0.0, -490.0, -10.0, 108.0, WALL),
    fb(490.0, 0.0, 0.0, 520.0, 70.0, 108.0, WALL),
    fb(-560.0, -390.0, 0.0, -500.0, -360.0, 108.0, WALL),
    fb(500.0, -390.0, 0.0, 560.0, -360.0, 108.0, WALL),
    // Low side platforms.
    fb(-800.0, -190.0, 0.0, -720.0, -130.0, 26.0, WALL),
    fb(720.0, -190.0, 0.0, 800.0, -130.0, 26.0, WALL),
    // Barrel pairs near the south corners.
    fb(-840.0, -560.0, 0.0, -782.0, -502.0, 56.0, BARREL),
    fb(782.0, -560.0, 0.0, 840.0, -502.0, 56.0, BARREL),
];

const CONTENTS_SOLID: u32 = 1;

/// Cube corner indices in `+x +y +z` order.
fn corners(mins: [f32; 3], maxs: [f32; 3]) -> [[f32; 3]; 8] {
    [
        [mins[0], mins[1], mins[2]],
        [maxs[0], mins[1], mins[2]],
        [maxs[0], maxs[1], mins[2]],
        [mins[0], maxs[1], mins[2]],
        [mins[0], mins[1], maxs[2]],
        [maxs[0], mins[1], maxs[2]],
        [maxs[0], maxs[1], maxs[2]],
        [mins[0], maxs[1], maxs[2]],
    ]
}

/// (normal, quad corner order). Quad order is clockwise seen from outside the
/// box so the geometric normal points inward — the front face under
/// `FrontFace::Cw` is the one looking at the box.
const FACES: [([f32; 3], [usize; 4]); 6] = [
    ([0.0, 0.0, 1.0], [4, 7, 6, 5]),
    ([0.0, 0.0, -1.0], [0, 1, 2, 3]),
    ([1.0, 0.0, 0.0], [1, 5, 6, 2]),
    ([-1.0, 0.0, 0.0], [0, 3, 7, 4]),
    ([0.0, 1.0, 0.0], [3, 2, 6, 7]),
    ([0.0, -1.0, 0.0], [0, 4, 5, 1]),
];

fn pack_unit_vec(n: [f32; 3]) -> u32 {
    let component = |v: f32| ((v * 127.0 + 127.5) as i32).clamp(0, 255) as u8;
    u32::from_le_bytes([component(n[0]), component(n[1]), component(n[2]), 63])
}

fn pack_color(rgba: [f32; 4]) -> u32 {
    let component = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    u32::from_le_bytes([
        component(rgba[2]),
        component(rgba[1]),
        component(rgba[0]),
        component(rgba[3]),
    ])
}

/// Packed IW4 world vertex: pos(12) tangent.w(4) color(4) tex-uv(8)
/// lightmap-uv(8) normal(4) tangent(4).
fn pack_world_vertex(
    packed: &mut [u8; asset_iw4::size::GFX_WORLD_VERTEX],
    position: [f32; 3],
    tangent_w: f32,
    color: u32,
    uv: [f32; 2],
    normal: u32,
    tangent: u32,
) {
    packed[0..4].copy_from_slice(&position[0].to_le_bytes());
    packed[4..8].copy_from_slice(&position[1].to_le_bytes());
    packed[8..12].copy_from_slice(&position[2].to_le_bytes());
    packed[12..16].copy_from_slice(&tangent_w.to_le_bytes());
    packed[16..20].copy_from_slice(&color.to_le_bytes());
    packed[20..24].copy_from_slice(&uv[0].to_le_bytes());
    packed[24..28].copy_from_slice(&uv[1].to_le_bytes());
    // Lightmap UVs stay zeroed — the surfaces are not lightmapped.
    packed[36..40].copy_from_slice(&normal.to_le_bytes());
    packed[40..44].copy_from_slice(&tangent.to_le_bytes());
}

fn face_uv(position: [f32; 3], normal: [f32; 3]) -> [f32; 2] {
    // Project onto the face plane so the checker diagnostic stays axis aligned.
    let (u, v) = if normal[2] != 0.0 {
        (position[0], position[1])
    } else if normal[0] != 0.0 {
        (position[1], position[2])
    } else {
        (position[0], position[2])
    };
    [u / 64.0, v / 64.0]
}

fn face_bounds(mins: [f32; 3], maxs: [f32; 3], normal: [f32; 3]) -> Bounds {
    let mid = [
        (mins[0] + maxs[0]) * 0.5,
        (mins[1] + maxs[1]) * 0.5,
        (mins[2] + maxs[2]) * 0.5,
    ];
    let half = [
        (maxs[0] - mins[0]) * 0.5,
        (maxs[1] - mins[1]) * 0.5,
        (maxs[2] - mins[2]) * 0.5,
    ];
    let face_min = [
        mid[0] - half[0] * (1.0 - normal[0].abs()),
        mid[1] - half[1] * (1.0 - normal[1].abs()),
        mid[2] - half[2] * (1.0 - normal[2].abs()),
    ];
    let face_max = [
        mid[0] + half[0] * (1.0 - normal[0].abs()),
        mid[1] + half[1] * (1.0 - normal[1].abs()),
        mid[2] + half[2] * (1.0 - normal[2].abs()),
    ];
    Bounds::from_mins_maxs(face_min, face_max)
}

struct FieldGeometry {
    draw: WorldDraw,
    clip: ClipCollision,
    min: [f32; 3],
    max: [f32; 3],
}

fn build_field_geometry() -> FieldGeometry {
    const VERTS_PER_FACE: usize = 4;
    const INDICES_PER_FACE: usize = 6;
    const FACES_PER_BOX: usize = 6;

    let surface_count = FIELD_BOXES.len() * FACES_PER_BOX;
    let vertex_total = FIELD_BOXES.len() * FACES_PER_BOX * VERTS_PER_FACE;

    let mut packed_vertices = Vec::with_capacity(vertex_total);
    let mut positions = Vec::with_capacity(vertex_total);
    let mut normals = Vec::with_capacity(vertex_total);
    let mut tangents = Vec::with_capacity(vertex_total);
    let mut colors = Vec::with_capacity(vertex_total);
    let mut texture_uvs = Vec::with_capacity(vertex_total);
    let lightmap_uvs = vec![[0.0f32; 2]; vertex_total];
    let mut packed_indices = Vec::with_capacity(surface_count * INDICES_PER_FACE);
    let mut surface_index_ranges = Vec::with_capacity(surface_count);
    let mut surface_batch_ranges = Vec::with_capacity(surface_count);
    let mut surface_draw_fields = Vec::with_capacity(surface_count);
    let mut surface_first_vertex = Vec::with_capacity(surface_count);
    let mut surface_bounds = Vec::with_capacity(surface_count);
    let mut brushes = Vec::with_capacity(FIELD_BOXES.len());
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for field_box in FIELD_BOXES {
        let c = corners(field_box.mins, field_box.maxs);
        let color = [
            f32::from(field_box.rgb[0]) / 255.0,
            f32::from(field_box.rgb[1]) / 255.0,
            f32::from(field_box.rgb[2]) / 255.0,
            1.0,
        ];
        let packed_color = pack_color(color);
        brushes.push(ClipBrush {
            planes: vec![
                [1.0, 0.0, 0.0, field_box.maxs[0]],
                [-1.0, 0.0, 0.0, -field_box.mins[0]],
                [0.0, 1.0, 0.0, field_box.maxs[1]],
                [0.0, -1.0, 0.0, -field_box.mins[1]],
                [0.0, 0.0, 1.0, field_box.maxs[2]],
                [0.0, 0.0, -1.0, -field_box.mins[2]],
            ],
            contents: CONTENTS_SOLID,
            plane_surface_flags: vec![0; 6],
            glass_encoded: 0,
        });
        for axis in 0..3 {
            min[axis] = min[axis].min(field_box.mins[axis]);
            max[axis] = max[axis].max(field_box.maxs[axis]);
        }

        for (normal, quad) in FACES {
            let first_vertex = positions.len() as u32;
            let index_start = packed_indices.len() as u32;
            surface_first_vertex.push(first_vertex);
            surface_draw_fields.push(SurfaceDrawFields {
                first_vertex,
                tri_count: 2,
                base_index: index_start,
                lightmap_index: 0,
                reflection_probe_index: 0,
                primary_light_index: 0,
            });
            surface_index_ranges.push((index_start, INDICES_PER_FACE as u32));
            surface_batch_ranges.push((0, index_start, INDICES_PER_FACE as u32));
            surface_bounds.push(face_bounds(field_box.mins, field_box.maxs, normal));
            packed_indices.extend_from_slice(&[
                first_vertex,
                first_vertex + 1,
                first_vertex + 2,
                first_vertex,
                first_vertex + 2,
                first_vertex + 3,
            ]);
            // Any in-face unit vector works for the tangent; the diagnostic
            // overlay only needs a normal and color.
            let tangent = if normal[2] != 0.0 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 0.0, 1.0, 1.0]
            };
            for &corner in &quad {
                let position = c[corner];
                positions.push(position);
                normals.push(normal);
                tangents.push(tangent);
                colors.push(color);
                texture_uvs.push(face_uv(position, normal));
                let mut packed = [0u8; asset_iw4::size::GFX_WORLD_VERTEX];
                pack_world_vertex(
                    &mut packed,
                    position,
                    tangent[3],
                    packed_color,
                    face_uv(position, normal),
                    pack_unit_vec(normal),
                    pack_unit_vec([tangent[0], tangent[1], tangent[2]]),
                );
                packed_vertices.push(packed);
            }
        }
    }

    let n = surface_count;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_uvs.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, lightmap_uvs.clone());
    mesh.insert_indices(Indices::U32(packed_indices.clone()));

    let batch = WorldBatch {
        mesh,
        packed_indices: packed_indices.clone(),
        material: None,
        lightmapped: false,
        lightmap_index: 0,
        primary_light_index: 0,
        reflection_probe_index: 0,
    };

    // One DPVS cell whose root span admits every surface. With no BSP nodes
    // `cell_for_point` returns `None` and the culler treats the cell as
    // always visible.
    let mut dpvs = DpvsWorldData::new(CameraSurfRanges::new(
        CameraSurfRange {
            kind: CameraRangeKind::LitOpaque,
            begin: 0,
            end: n as u32,
        },
        Vec::new(),
    ));
    dpvs.cell_count = 1;
    dpvs.cell_roots = vec![SurfRange {
        start: 0,
        count: n as u16,
    }];
    dpvs.aabb_trees = vec![Vec::new()];
    dpvs.aabb_smodel_indices = vec![Vec::new()];
    dpvs.sorted_surf_index = (0..n as u16).collect();
    dpvs.static_surface_count = n;
    dpvs.static_surface_count_no_decal = n;
    dpvs.surface_bounds = surface_bounds;
    dpvs.portals_per_cell = vec![Vec::new()];
    dpvs.cell_reflection_probes = vec![Vec::new()];
    dpvs.lit_opaque_begin = 0;
    dpvs.lit_opaque_end = n as u32;

    let radius =
        ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2) + (max[2] - min[2]).powi(2)).sqrt()
            * 0.5;

    let draw = WorldDraw {
        batches: vec![batch],
        sky_model: None,
        lightmap: Err(WorldLightmapGap::Missing),
        stats: WorldMeshStats {
            vertices: positions.len(),
            triangles: packed_indices.len() / 3,
            surfaces: n,
            skipped_surfaces: 0,
            sky_surfaces: 0,
            sky_material: None,
            unrouted_surfaces: 0,
            undecided_state_bits_surfaces: 0,
            min,
            max,
            bounds: Some([min[0], min[1], min[2], max[0], max[1], max[2]]),
        },
        packed_vertices: WorldVertexPayload::Iw4(packed_vertices),
        vertex_layer: Vec::new(),
        surface_vertex_layer: Vec::new(),
        surface_first_vertex,
        surface_draw_fields,
        positions,
        normals,
        tangents,
        colors,
        texture_uvs,
        lightmap_uvs,
        packed_indices,
        surface_index_ranges,
        surface_batch_ranges,
        surface_lightmapped: vec![false; n],
        surface_lightmap_indices: vec![0; n],
        surface_reflection_probes: vec![0; n],
        surface_primary_lights: vec![0; n],
        sort_key_distortion: None,
        capture: world_capture_from_casters(SurfaceCastsSunShadow::with_len(n)),
        brush_models: vec![GfxBrushModelSurfs {
            start_surf: 0,
            surface_count: n as u16,
            surface_count_no_decal: n as u16,
        }],
        brush_model_bounds: vec![GfxBrushModelBounds {
            mid: [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ],
            half: [
                (max[0] - min[0]) * 0.5,
                (max[1] - min[1]) * 0.5,
                (max[2] - min[2]) * 0.5,
            ],
        }],
        surface_materials: vec![None; n],
        primary_lights: Vec::new(),
        light_defs: Vec::new(),
        sun_primary_light_count: 0,
        sun_stages: Vec::new(),
        light_region_hulls: None,
        shadow_geometry: Vec::new(),
        reflection_probes: Vec::new(),
        dpvs,
        outdoor_image_name: None,
        outdoor_image: None,
        outdoor_lookup: [0; 16],
        sun_effects: None,
        t5_sun_parse_exposure: None,
        t5_sky_dynamic_intensity: None,
        t5_sun_light: None,
        t5_tree_scatter_intensity: None,
        t5_tree_scatter_amount: None,
        t5_exposure_volume_count: 0,
    };

    let clip = ClipCollision {
        brushes,
        nodes: Vec::new(),
        leaves: Vec::new(),
        leafbrushes: Vec::new(),
        mesh: Arc::new(clipmap_iw4::ClipMeshTables::default()),
        tri_material_index: Vec::new(),
        materials: Vec::new(),
        cmodels: vec![ClipCmodel {
            mins: min,
            maxs: max,
            radius,
            first_brush: 0,
            num_brushes: FIELD_BOXES.len() as u16,
        }],
        trigger_models: Vec::new(),
        static_models: Vec::new(),
    };

    FieldGeometry {
        draw,
        clip,
        min,
        max,
    }
}

/// `(origin, yaw)` — yaw faces field center. Every point keeps the player
/// hull (±15 xy) clear of every box in `FIELD_BOXES`: the centre-spot in the
/// twin-bunker alcove sits on its axis, and nothing hugs a wall face.
const SPAWN_STARTS: &[([f32; 3], f32)] = &[
    ([-560.0, 340.0, 8.0], -31.0),
    ([560.0, 340.0, 8.0], -149.0),
    ([-560.0, -320.0, 8.0], 31.0),
    ([560.0, -320.0, 8.0], 149.0),
    ([0.0, 440.0, 8.0], -90.0),
    ([-57.0, -440.0, 8.0], 90.0),
    ([-700.0, 60.0, 8.0], -5.0),
    ([700.0, 60.0, 8.0], 175.0),
];

const SPAWNS: &[([f32; 3], f32)] = &[
    ([-350.0, 160.0, 8.0], -25.0),
    ([350.0, 160.0, 8.0], -155.0),
    ([-350.0, -200.0, 8.0], 30.0),
    ([350.0, -200.0, 8.0], 150.0),
    ([0.0, -260.0, 8.0], 90.0),
    ([0.0, 330.0, 8.0], -90.0),
    ([-700.0, -300.0, 8.0], 23.0),
    ([700.0, -300.0, 8.0], 157.0),
];

const INTERMISSION_ORIGIN: [f32; 3] = [0.0, -780.0, 620.0];
const INTERMISSION_ANGLES: [f32; 3] = [55.0, 0.0, 0.0];

fn spawn_point(classname: &str, origin: [f32; 3], yaw: f32) -> SpawnPoint {
    SpawnPoint {
        classname: classname.to_owned(),
        origin,
        angles: [0.0, yaw, 0.0],
        script_linkto: String::new(),
        script_destructable_area: String::new(),
    }
}

fn field_spawns() -> Vec<SpawnPoint> {
    let mut points = vec![spawn_point(
        "mp_global_intermission",
        INTERMISSION_ORIGIN,
        INTERMISSION_ANGLES[1],
    )];
    points.extend(
        SPAWN_STARTS
            .iter()
            .map(|&(origin, yaw)| spawn_point("mp_dm_spawn_start", origin, yaw)),
    );
    points.extend(
        SPAWNS
            .iter()
            .map(|&(origin, yaw)| spawn_point("mp_dm_spawn", origin, yaw)),
    );
    points
}

fn push_ent(text: &mut String, pairs: &[(&str, String)]) {
    text.push_str("{\n");
    for (key, value) in pairs {
        text.push_str(&format!("\"{key}\" \"{value}\"\n"));
    }
    text.push_str("}\n");
}

fn entity_string() -> String {
    let mut text = String::new();
    push_ent(
        &mut text,
        &[
            ("classname", "worldspawn".to_owned()),
            ("northYaw", "90".to_owned()),
        ],
    );
    for point in field_spawns() {
        let origin = format!(
            "{} {} {}",
            point.origin[0], point.origin[1], point.origin[2]
        );
        let angles = format!(
            "{} {} {}",
            point.angles[0], point.angles[1], point.angles[2]
        );
        push_ent(
            &mut text,
            &[
                ("classname", point.classname.clone()),
                ("origin", origin),
                ("angles", angles),
            ],
        );
    }
    for corner in [[-896.0, -636.0], [896.0, 636.0]] {
        push_ent(
            &mut text,
            &[
                ("classname", "script_origin".to_owned()),
                ("targetname", "minimap_corner".to_owned()),
                ("origin", format!("{} {} 0", corner[0], corner[1])),
            ],
        );
    }
    text
}

const CALLBACK_SETUP: &str = r#"codecallback_startgametype()
{
	level notify( "prematch_over" );
}

codecallback_playerconnect()
{
	self.sessionstate = "playing";
	self thread spawn_on_begin();
}

codecallback_playerdisconnect()
{
}

codecallback_playerdamage( eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc, psOffsetTime )
{
}

codecallback_playerkilled( eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc, psOffsetTime, deathAnimDuration )
{
}

codecallback_playerlaststand( eInflictor, eAttacker, iDamage, sMeansOfDeath, sWeapon, vDir, sHitLoc, psOffsetTime, deathAnimDuration )
{
}

codecallback_vehicledamage( eInflictor, eAttacker, iDamage, iDFlags, sMeansOfDeath, sWeapon, vPoint, vDir, sHitLoc, psOffsetTime, damageFromUnderneath )
{
}

spawn_on_begin()
{
	self endon( "disconnect" );
	self waittill( "begin" );
	for ( ;; )
	{
		point = pick_spawn();
		self.sessionstate = "playing";
		self spawn( point.origin, point.angles );
		self waittill( "death" );
		wait 1.5;
	}
}

pick_spawn()
{
	points = getentarray( "mp_dm_spawn_start", "classname" );
	if ( !isdefined( points ) || points.size == 0 )
		points = getentarray( "mp_dm_spawn", "classname" );
	return points[ randomint( points.size ) ];
}
"#;

const EMPTY_MAIN: &str = "main()\n{\n}\n";

fn field_scripts(zone: &str) -> crate::ScriptSources {
    let mut sources = crate::ScriptSources::default();
    sources.insert_source("codescripts/delete", EMPTY_MAIN.to_owned());
    sources.insert_source("codescripts/struct", EMPTY_MAIN.to_owned());
    sources.insert_source(
        "maps/mp/gametypes/_callbacksetup",
        CALLBACK_SETUP.to_owned(),
    );
    // Every gametype token resolves to a `main` stub so `IW4L_GAMETYPE` can
    // pick any of them; the connect/spawn flow lives in `_callbacksetup` and
    // is gametype-agnostic.
    for token in ["dm", "war", "dom", "dd", "sd", "ctf", "koth", "sab"] {
        sources.insert_source(&format!("maps/mp/gametypes/{token}"), EMPTY_MAIN.to_owned());
    }
    // `Iw4Startup` resolves the map module by stem (`iw4l:field` → `maps/mp/field`).
    let stem = zone.rsplit(':').next().unwrap_or(zone);
    sources.insert_source(&format!("maps/mp/{stem}"), EMPTY_MAIN.to_owned());
    // `bind_or_initialize` refuses a listen host without `mp/stats_init.cfg`
    // + a `mp/playerdata.def` schema: the local account gets a default
    // playerdata buffer on first join. An empty cfg just writes the header
    // and the ten class names.
    sources.capture("mp/stats_init.cfg", b"// built-in defaults\n", false);
    sources.capture_schema("mp/playerdata.def".to_owned(), playerdata_schema());
    sources.set_entities(entity_string());
    sources
}

/// Minimal playerdata layout: one `customClasses[10]` array of `{ name }`
/// entries after the eight-byte version/checksum header. That is all
/// `persistent_defaults::initialize` writes into the buffer.
fn playerdata_schema() -> structured_data_iw4::DefinitionSet {
    use structured_data_iw4::{DataType, Definition, IndexedArray, Property, Struct};
    structured_data_iw4::DefinitionSet {
        definitions: vec![Definition {
            version: 1,
            checksum: 0x46_49_45_4C,
            size: 1024,
            root: DataType::Struct(0),
            enums: Vec::new(),
            structs: vec![
                Struct {
                    properties: vec![Property {
                        name: "customClasses".to_owned(),
                        ty: DataType::IndexedArray(0),
                        offset: 8,
                    }],
                    size: 0,
                    bit_offset: 0,
                },
                Struct {
                    properties: vec![Property {
                        name: "name".to_owned(),
                        ty: DataType::String(32),
                        offset: 0,
                    }],
                    size: 32,
                    bit_offset: 0,
                },
            ],
            indexed_arrays: vec![IndexedArray {
                count: 10,
                element: DataType::Struct(1),
                stride: 32,
            }],
            enum_arrays: Vec::new(),
        }],
    }
}

/// Synthesize the complete prepared match for an `iw4l:` zone key.
pub fn builtin_prepared_match(zone: &str) -> PreparedMatch {
    let geometry = build_field_geometry();
    let scripts = field_scripts(zone);
    let spawns = field_spawns();
    let stem = zone.rsplit(':').next().unwrap_or(zone);
    let mut prepared = PreparedMatch {
        world: PreparedWorld {
            draw: Some(geometry.draw),
            intermission_view: Some(IntermissionView {
                origin: INTERMISSION_ORIGIN,
                angles: INTERMISSION_ANGLES,
            }),
            min: geometry.min,
            max: geometry.max,
            world_bounds: Some([
                geometry.min[0],
                geometry.min[1],
                geometry.min[2],
                geometry.max[0],
                geometry.max[1],
                geometry.max[2],
            ]),
            policy: asset_world::WorldDrawPolicy::iw4(),
            ..Default::default()
        },
        scripts,
        materials: MatchMaterials {
            population: Arc::new(MaterialDefinitions::default()),
            ..Default::default()
        },
        clip: Some(Arc::new(geometry.clip)),
        prepared_map: PreparedMap {
            // The manifest durable key is `iw4:map/<stem>` — the `iw4l:`
            // prefix is transport-only and `:` is not a legal key character.
            zone: stem.to_owned(),
            namespace: Some(asset_core::AssetNamespace::Iw4),
            spawns,
            facts: MapFacts {
                minimap_corners: Some(asset_world::MinimapCorners {
                    a: [-896.0, -636.0],
                    b: [896.0, 636.0],
                }),
                north_yaw: Some(90.0),
                ..Default::default()
            },
            ..Default::default()
        },
        report: vec![
            format!("built-in map `{zone}`: no zone file opened"),
            format!(
                "built-in world: {} solid boxes, {} draw surfaces",
                FIELD_BOXES.len(),
                FIELD_BOXES.len() * 6
            ),
        ],
        ..Default::default()
    };
    prepared
        .report
        .push("built-in scripts: _callbacksetup connect/spawn flow, dm gametype stub".to_owned());
    prepared
}
