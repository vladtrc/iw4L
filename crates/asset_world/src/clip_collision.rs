use std::collections::HashMap;
use std::sync::Arc;

use asset_iw4::size as sz;
use fastfile_iw4::{ClipMapGeometry, Ptr, ZonePtr, ZoneStream};

#[derive(Clone, Debug)]
pub struct ClipBrush {
    pub planes: Vec<[f32; 4]>,
    pub contents: u32,

    pub plane_surface_flags: Vec<u32>,

    pub glass_encoded: u16,
}

pub use clipmap_iw4::{ClipLeaf as ClipBspLeaf, ClipNode as ClipBspNode};

#[derive(Clone, Copy, Debug)]
pub struct ClipCmodel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub radius: f32,
    pub first_brush: u32,
    pub num_brushes: u16,
}

#[derive(Clone, Debug, Default)]
pub struct ClipMapMaterial {
    pub name: String,
    pub surface_flags: u32,
    pub content_flags: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ClipCollision {
    pub brushes: Vec<ClipBrush>,

    pub nodes: Vec<ClipBspNode>,
    pub leaves: Vec<ClipBspLeaf>,
    pub leafbrushes: Vec<u16>,

    /// The mesh tables, shared with every other owner of this collision
    /// instead of copied into each of them.
    pub mesh: std::sync::Arc<clipmap_iw4::ClipMeshTables>,

    pub tri_material_index: Vec<u16>,

    pub materials: Vec<ClipMapMaterial>,

    pub cmodels: Vec<ClipCmodel>,

    pub static_models: Vec<ClipPlacedStaticModel>,
}

#[derive(Clone, Debug)]
pub struct ClipPlacedStaticModel {
    pub index: u32,
    pub name: String,
    pub model: clipmap_iw4::ClipStaticModel,
}

#[derive(Clone, Debug, Default)]
pub struct XModelCollCatalog {
    models: Vec<Arc<CapturedXModelColl>>,
    by_slot: HashMap<(u8, u32), usize>,
}

#[derive(Clone, Debug)]
struct CapturedXModelColl {
    name: String,
    coll: clipmap_iw4::XModelColl,
}

impl XModelCollCatalog {
    pub fn insert(&mut self, stream: &ZoneStream<'_>, slot: Ptr, insert_slot: Option<Ptr>) {
        let Some(g) = stream.xmodel() else {
            return;
        };
        let name = g
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("")
            .to_owned();
        let surfs = match g.coll_surfs {
            Some(arr) => read_xmodel_coll_surfs(stream, arr, g.num_coll_surfs.max(0) as usize),
            None => Vec::new(),
        };
        let captured = Arc::new(CapturedXModelColl {
            name,
            coll: clipmap_iw4::XModelColl {
                coll_lod: g.coll_lod,
                contents: g.contents,
                surfs,
            },
        });
        let idx = self.models.len();
        self.models.push(Arc::clone(&captured));
        self.by_slot.insert((slot.block, slot.offset), idx);
        if let Some(ins) = insert_slot {
            self.by_slot.insert((ins.block, ins.offset), idx);
        }
    }

    pub fn alias(&mut self, slot: Ptr, target: Ptr) {
        if let Some(&idx) = self.by_slot.get(&(target.block, target.offset)) {
            self.by_slot.insert((slot.block, slot.offset), idx);
        }
    }

    pub fn insert_iw5(
        &mut self,
        stream: &fastfile_iw5::ZoneStream<'_>,
        slot: fastfile_iw5::Ptr,
        insert_slot: Option<fastfile_iw5::Ptr>,
    ) {
        let Some(g) = stream.latest_xmodel() else {
            return;
        };
        let name = g
            .name
            .and_then(|p| stream.cstr(p).ok())
            .unwrap_or("")
            .to_owned();
        let surfs = match g.coll_surfs {
            Some(arr) => read_iw5_xmodel_coll_surfs(stream, arr, g.num_coll_surfs.max(0) as usize),
            None => Vec::new(),
        };
        let captured = Arc::new(CapturedXModelColl {
            name,
            coll: clipmap_iw4::XModelColl {
                coll_lod: g.coll_lod,
                contents: g.contents,
                surfs,
            },
        });
        let idx = self.models.len();
        self.models.push(Arc::clone(&captured));
        self.by_slot.insert((slot.block, slot.offset), idx);
        if let Some(ins) = insert_slot {
            self.by_slot.insert((ins.block, ins.offset), idx);
        }
    }

    pub fn alias_iw5(&mut self, slot: fastfile_iw5::Ptr, target: fastfile_iw5::Ptr) {
        if let Some(&idx) = self.by_slot.get(&(target.block, target.offset)) {
            self.by_slot.insert((slot.block, slot.offset), idx);
        }
    }

    fn get(&self, slot: Ptr) -> Option<&CapturedXModelColl> {
        self.get_at(slot.block, slot.offset)
    }

    fn get_at(&self, block: u8, offset: u32) -> Option<&CapturedXModelColl> {
        let i = *self.by_slot.get(&(block, offset))?;
        self.models.get(i).map(|a| a.as_ref())
    }
}

pub const MASK_PLAYER_SOLID: u32 = 0x0281_0011;

impl clipmap_iw4::BrushView for ClipBrush {
    fn planes(&self) -> &[[f32; 4]] {
        &self.planes
    }
    fn contents(&self) -> u32 {
        self.contents
    }
    fn plane_surface_flags(&self) -> &[u32] {
        &self.plane_surface_flags
    }
    fn glass_encoded(&self) -> u16 {
        self.glass_encoded
    }
}

impl ClipCollision {
    fn map_ref(&self) -> clipmap_iw4::ClipMapRef<'_, ClipBrush> {
        clipmap_iw4::ClipMapRef {
            nodes: &self.nodes,
            leaves: &self.leaves,
            leafbrushes: &self.leafbrushes,
            brushes: &self.brushes,
        }
    }

    pub fn box_sight_clear(&self, start: [f32; 3], end: [f32; 3], mask: u32) -> bool {
        let map = self.map_ref();
        let ext = clipmap_iw4::TraceExtents::new(start, end, [0.0; 3], [0.0; 3], mask);
        let mesh = self.mesh.as_ref().as_ref();
        let trace = clipmap_iw4::trace_brush_and_mesh(&map, &mesh, &ext, &|_piece| true);
        trace.startsolid == 0 && trace.fraction >= 1.0
    }

    pub fn sweep_box(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        mask: u32,
    ) -> ClipSweepHit {
        let map = self.map_ref();
        let ext = clipmap_iw4::TraceExtents::new(start, end, mins, maxs, mask);
        let mesh = self.mesh.as_ref().as_ref();
        let trace = clipmap_iw4::trace_brush_and_mesh(&map, &mesh, &ext, &|_piece| true);
        ClipSweepHit {
            fraction: trace.fraction,
            normal: trace.normal,
            endpos: trace.endpos,
            startsolid: trace.startsolid != 0,
            allsolid: trace.allsolid != 0,
            surface_flags: trace.surface_flags,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClipSweepHit {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub endpos: [f32; 3],
    pub startsolid: bool,
    pub allsolid: bool,

    pub surface_flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipCollisionError {
    MissingTables,
    Truncated,

    LeafbrushIndexOverflow,
}

impl core::fmt::Display for ClipCollisionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingTables => write!(f, "clipmap brush tables were not retained"),
            Self::Truncated => write!(f, "clipmap brush bytes truncated in arena"),
            Self::LeafbrushIndexOverflow => {
                write!(
                    f,
                    "leafbrush table length does not fit first_brush/num_brushes index types"
                )
            }
        }
    }
}

fn leafbrush_range(first_len: usize, after_len: usize) -> Result<(u32, u16), ClipCollisionError> {
    let first = u32::try_from(first_len).map_err(|_| ClipCollisionError::LeafbrushIndexOverflow)?;
    let count = after_len
        .checked_sub(first_len)
        .ok_or(ClipCollisionError::LeafbrushIndexOverflow)?;
    let num = u16::try_from(count).map_err(|_| ClipCollisionError::LeafbrushIndexOverflow)?;
    Ok((first, num))
}

pub fn gate_leafbrush_index_fits(leafbrush_count: usize) -> Result<(), ClipCollisionError> {
    if leafbrush_count > u32::MAX as usize {
        return Err(ClipCollisionError::LeafbrushIndexOverflow);
    }
    Ok(())
}

pub fn build_clip_collision(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
) -> Result<ClipCollision, ClipCollisionError> {
    let brushes_ptr = g.brushes.ok_or(ClipCollisionError::MissingTables)?;
    let bounds_ptr = g.brush_bounds.ok_or(ClipCollisionError::MissingTables)?;
    let contents_ptr = g.brush_contents.ok_or(ClipCollisionError::MissingTables)?;
    let sides_base = g.brush_sides;
    let materials = g.materials;

    let mut out = ClipCollision {
        brushes: Vec::with_capacity(g.brush_count),
        ..ClipCollision::default()
    };

    for i in 0..g.brush_count {
        let brush = brushes_ptr.at(i * s.layout(sz::CBRUSH, 48));
        let bound = bounds_ptr.at(i * 24);
        let contents = s
            .u32_at(contents_ptr, i * 4)
            .map_err(|_| ClipCollisionError::Truncated)?;

        let mid = [
            s.f32_at(bound, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let half = [
            s.f32_at(bound, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 16)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 20)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let mins = [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]];
        let maxs = [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]];

        let mut planes = Vec::with_capacity(6);
        planes.push([1.0, 0.0, 0.0, maxs[0]]);
        planes.push([-1.0, 0.0, 0.0, -mins[0]]);
        planes.push([0.0, 1.0, 0.0, maxs[1]]);
        planes.push([0.0, -1.0, 0.0, -mins[1]]);
        planes.push([0.0, 0.0, 1.0, maxs[2]]);
        planes.push([0.0, 0.0, -1.0, -mins[2]]);

        let mut axial_mat = [0u16; 6];
        for i in 0..6 {
            axial_mat[i] = s
                .u16_at(brush, s.layout(12, 24) + i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
        }
        let mut plane_surface_flags = Vec::with_capacity(6);
        plane_surface_flags.extend_from_slice(&axial_plane_flags(axial_mat, |idx| {
            material_flags(s, materials, idx)
        }));

        let numsides = s
            .u16_at(brush, 0)
            .map_err(|_| ClipCollisionError::Truncated)? as usize;
        let glass_encoded = s
            .u16_at(brush, 2)
            .map_err(|_| ClipCollisionError::Truncated)?;
        if let (Some(sides_all), Some(first)) = (sides_base, side_first(s, brush, sides_base)?) {
            for j in 0..numsides {
                let side = sides_all.at((first + j) * s.layout(sz::CBRUSH_SIDE, 16));
                if let Some(plane) = read_side_plane(s, side)? {
                    planes.push(plane);
                    let mat = s
                        .u16_at(side, s.layout(4, 8))
                        .map_err(|_| ClipCollisionError::Truncated)?
                        as usize;
                    plane_surface_flags.push(material_flags(s, materials, mat));
                }
            }
        }

        debug_assert_eq!(planes.len(), plane_surface_flags.len());
        out.brushes.push(ClipBrush {
            planes,
            contents,
            plane_surface_flags,
            glass_encoded,
        });
    }

    extract_bsp_tables(s, g, &mut out)?;
    extract_mesh_tables(s, g, &mut out)?;
    extract_cmodels(s, g, &mut out)?;
    gate_leafbrush_index_fits(out.leafbrushes.len())?;

    Ok(out)
}

pub fn attach_static_models(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    catalog: &XModelCollCatalog,
    out: &mut ClipCollision,
) {
    extract_static_models(s, g, catalog, out);
}

pub fn attach_iw5_static_models(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    catalog: &XModelCollCatalog,
    out: &mut ClipCollision,
) {
    extract_iw5_static_models(s, g, catalog, out);
}

fn extract_mesh_tables(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    let Some(verts_ptr) = g.verts else {
        return Ok(());
    };
    mesh.verts.reserve(g.vert_count);
    for i in 0..g.vert_count {
        let v = verts_ptr.at(i * 12);
        mesh.verts.push([
            s.f32_at(v, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ]);
    }
    if let Some(idx_ptr) = g.tri_indices {
        let n = g.tri_count.saturating_mul(3);
        mesh.tri_indices.reserve(n);
        for i in 0..n {
            let id = s
                .u16_at(idx_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_indices.push(id);
        }
    }
    if let Some(walk_ptr) = g.tri_edge_is_walkable {
        let n = g.tri_count.saturating_mul(3).div_ceil(32) * 4;
        mesh.tri_edge_is_walkable.reserve(n);
        for i in 0..n {
            let b = s
                .u8_at(walk_ptr, i)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_edge_is_walkable.push(b);
        }
    }
    extract_mesh_materials(s, g, out)?;
    extract_aabb_forest(s, g, out)?;
    Ok(())
}

fn extract_aabb_forest(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    if let Some(parts) = g.collision_partitions {
        mesh.partitions.reserve(g.partition_count);
        for i in 0..g.partition_count {
            let p = parts.at(i * s.layout(sz::COLLISION_PARTITION, 16));
            let tri_n = s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
            let seg = s.u8_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
            let first = s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?;
            mesh.partitions.push(clipmap_iw4::ClipPartition {
                tri_count: tri_n,
                first_tri: first,
                first_vert_segment: seg,
            });
        }
    }
    if let Some(trees) = g.collision_aabb_trees {
        mesh.aabb_trees.reserve(g.aabb_tree_count);
        for i in 0..g.aabb_tree_count {
            let node = trees.at(i * sz::COLLISION_AABB_TREE);
            let origin = [
                s.f32_at(node, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 4)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 8)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ];
            let material_index = s
                .u16_at(node, 12)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let child_count = s
                .u16_at(node, 14)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let half_size = [
                s.f32_at(node, 16)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 20)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 24)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ];
            let u = s
                .i32_at(node, 28)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.aabb_trees.push(clipmap_iw4::ClipAabbNode {
                origin,
                half_size,
                material_index,
                child_count,
                u,
            });
        }
    }
    mesh.aabb_roots = clipmap_iw4::aabb_forest_roots(&mesh.aabb_trees);
    Ok(())
}

fn extract_mesh_materials(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    out.materials = extract_clip_materials_iw4(s, g)?;
    if g.tri_count == 0 {
        return Ok(());
    }
    let mut material_sflags = vec![0u32; g.material_count];
    let mut material_cflags = vec![0u32; g.material_count];
    for (i, mat) in out.materials.iter().enumerate() {
        if let Some(slot) = material_sflags.get_mut(i) {
            *slot = mat.surface_flags;
        }
        if let Some(slot) = material_cflags.get_mut(i) {
            *slot = mat.content_flags;
        }
    }
    let (leaves, partitions) = collect_aabb_leaves_iw4(s, g)?;
    mesh.tri_surface_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &material_sflags, g.tri_count);
    mesh.tri_content_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &material_cflags, g.tri_count);
    out.tri_material_index =
        clipmap_iw4::flatten_tri_material_index(&leaves, &partitions, g.tri_count);
    Ok(())
}

fn extract_clip_materials_iw4(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
) -> Result<Vec<ClipMapMaterial>, ClipCollisionError> {
    let mut out = Vec::with_capacity(g.material_count);
    let Some(mats) = g.materials else {
        out.resize(g.material_count, ClipMapMaterial::default());
        return Ok(out);
    };
    for i in 0..g.material_count {
        let slot = mats.at(i * s.layout(sz::CLIP_MATERIAL, 16));
        let name = match s
            .ptr_at(slot, 0)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).unwrap_or("").to_owned(),
            _ => String::new(),
        };
        let surface_flags = s
            .u32_at(slot, s.layout(4, 8))
            .map_err(|_| ClipCollisionError::Truncated)?;
        let content_flags = s
            .u32_at(slot, s.layout(8, 12))
            .map_err(|_| ClipCollisionError::Truncated)?;
        out.push(ClipMapMaterial {
            name,
            surface_flags,
            content_flags,
        });
    }
    Ok(out)
}

fn collect_aabb_leaves_iw4(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
) -> Result<(Vec<(u16, i32)>, Vec<(u8, i32)>), ClipCollisionError> {
    let mut partitions = vec![(0u8, 0i32); g.partition_count];
    if let Some(parts) = g.collision_partitions {
        for (i, slot) in partitions.iter_mut().enumerate() {
            let p = parts.at(i * s.layout(sz::COLLISION_PARTITION, 16));
            let tri_n = s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
            let first = s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?;
            *slot = (tri_n, first);
        }
    }
    let mut leaves = Vec::new();
    if let Some(trees) = g.collision_aabb_trees {
        for i in 0..g.aabb_tree_count {
            let node = trees.at(i * sz::COLLISION_AABB_TREE);
            let child_count = s
                .u16_at(node, 14)
                .map_err(|_| ClipCollisionError::Truncated)?;
            if child_count != 0 {
                continue;
            }
            let mat = s
                .u16_at(node, 12)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let part = s
                .i32_at(node, 28)
                .map_err(|_| ClipCollisionError::Truncated)?;
            leaves.push((mat, part));
        }
    }
    Ok((leaves, partitions))
}

fn extract_cmodels(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let Some(cmodels_ptr) = g.cmodels else {
        return Ok(());
    };
    let lb_nodes = g.leafbrush_nodes;
    out.cmodels.reserve(g.cmodel_count);
    for i in 0..g.cmodel_count {
        let cm = cmodels_ptr.at(i * sz::CMODEL);
        let mins = [
            s.f32_at(cm, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let maxs = [
            s.f32_at(cm, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 16)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 20)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let radius = s
            .f32_at(cm, 24)
            .map_err(|_| ClipCollisionError::Truncated)?;

        let lb_index = s
            .i32_at(cm, 64)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let first_len = out.leafbrushes.len();
        if lb_index >= 0 {
            if let Some(lb_base) = lb_nodes {
                append_leafbrush_node(s, lb_base, lb_index as usize, &mut out.leafbrushes)?;
            }
        }
        let (first, num) = leafbrush_range(first_len, out.leafbrushes.len())?;
        out.cmodels.push(ClipCmodel {
            mins,
            maxs,
            radius,
            first_brush: first,
            num_brushes: num,
        });
    }
    Ok(())
}

fn extract_static_models(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    catalog: &XModelCollCatalog,
    out: &mut ClipCollision,
) {
    let Some(arr) = g.static_models else {
        return;
    };
    out.static_models.reserve(g.static_model_count);
    for i in 0..g.static_model_count {
        let sm = arr.at(i * s.layout(sz::C_STATIC_MODEL, 80));
        if let Some(placed) = read_placed_static_model(s, sm, i as u32, catalog) {
            out.static_models.push(placed);
        }
    }
}

fn read_placed_static_model(
    s: &ZoneStream<'_>,
    sm: Ptr,
    index: u32,
    catalog: &XModelCollCatalog,
) -> Option<ClipPlacedStaticModel> {
    let body = match s.ptr_at(sm, 0).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return None,
    };
    let captured = catalog.get(body)?;
    let origin = [
        s.f32_at(sm, s.layout(4, 8)).ok()?,
        s.f32_at(sm, s.layout(8, 12)).ok()?,
        s.f32_at(sm, s.layout(12, 16)).ok()?,
    ];
    let mut inv_scaled_axis = [[0.0_f32; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            inv_scaled_axis[row][col] = s
                .f32_at(sm, s.layout(0x10, 20) + (row * 3 + col) * 4)
                .ok()?;
        }
    }
    let bounds_mid = [
        s.f32_at(sm, s.layout(0x34, 56)).ok()?,
        s.f32_at(sm, s.layout(0x38, 60)).ok()?,
        s.f32_at(sm, s.layout(0x3c, 64)).ok()?,
    ];
    let bounds_half = [
        s.f32_at(sm, s.layout(0x40, 68)).ok()?,
        s.f32_at(sm, s.layout(0x44, 72)).ok()?,
        s.f32_at(sm, s.layout(0x48, 76)).ok()?,
    ];
    if !origin.iter().all(|v| v.is_finite())
        || !bounds_mid.iter().all(|v| v.is_finite())
        || !bounds_half.iter().all(|v| v.is_finite() && *v >= 0.0)
    {
        return None;
    }
    Some(ClipPlacedStaticModel {
        index,
        name: captured.name.clone(),
        model: clipmap_iw4::ClipStaticModel {
            origin,
            inv_scaled_axis,
            bounds_mid,
            bounds_half,
            coll: captured.coll.clone(),
        },
    })
}

fn read_xmodel_coll_surfs(
    s: &ZoneStream<'_>,
    arr: Ptr,
    num: usize,
) -> Vec<clipmap_iw4::XModelCollSurf> {
    (0..num)
        .filter_map(|i| {
            let row = arr.at(i * s.layout(sz::XMODEL_COLL_SURF, 48));
            let midpoint = [
                s.f32_at(row, s.layout(8, 12)).ok()?,
                s.f32_at(row, s.layout(12, 16)).ok()?,
                s.f32_at(row, s.layout(16, 20)).ok()?,
            ];
            let half_size = [
                s.f32_at(row, s.layout(20, 24)).ok()?,
                s.f32_at(row, s.layout(24, 28)).ok()?,
                s.f32_at(row, s.layout(28, 32)).ok()?,
            ];
            let bone_idx = s.i32_at(row, s.layout(32, 36)).ok()?;
            let contents = s.u32_at(row, s.layout(36, 40)).ok()?;
            let surf_flags = s.u32_at(row, s.layout(40, 44)).ok()?;
            let num_tris = s.i32_at(row, s.layout(4, 8)).ok()?.max(0) as usize;
            if num_tris > 1_000_000 {
                return None;
            }
            let tris = match s.ptr_at(row, 0).ok()? {
                ZonePtr::Offset(p) => read_xmodel_coll_tris(s, s.resolve_alias(p), num_tris),
                _ => Vec::new(),
            };
            Some(clipmap_iw4::XModelCollSurf {
                tris,
                midpoint,
                half_size,
                bone_idx,
                contents,
                surf_flags,
            })
        })
        .collect()
}

fn read_xmodel_coll_tris(
    s: &ZoneStream<'_>,
    arr: Ptr,
    num: usize,
) -> Vec<clipmap_iw4::XModelCollTri> {
    (0..num)
        .filter_map(|i| {
            let t = arr.at(i * sz::XMODEL_COLL_TRI);
            let f = |off: usize| s.f32_at(t, off).ok();
            Some(clipmap_iw4::XModelCollTri {
                plane: [f(0)?, f(4)?, f(8)?, f(12)?],
                svec: [f(16)?, f(20)?, f(24)?, f(28)?],
                tvec: [f(32)?, f(36)?, f(40)?, f(44)?],
            })
        })
        .collect()
}

fn read_iw5_xmodel_coll_surfs(
    s: &fastfile_iw5::ZoneStream<'_>,
    arr: fastfile_iw5::Ptr,
    num: usize,
) -> Vec<clipmap_iw4::XModelCollSurf> {
    use fastfile_iw5::ZonePtr;
    use fastfile_iw5::size as iw5sz;
    (0..num)
        .filter_map(|i| {
            let row = arr.at(i * s.layout(iw5sz::XMODEL_COLL_SURF, 48));
            let bounds = s.layout(8, 12);
            let midpoint = [
                s.f32_at(row, bounds).ok()?,
                s.f32_at(row, bounds + 4).ok()?,
                s.f32_at(row, bounds + 8).ok()?,
            ];
            let half_size = [
                s.f32_at(row, bounds + 12).ok()?,
                s.f32_at(row, bounds + 16).ok()?,
                s.f32_at(row, bounds + 20).ok()?,
            ];
            let bone_idx = s.i32_at(row, s.layout(32, 36)).ok()?;
            let contents = s.u32_at(row, s.layout(36, 40)).ok()?;
            let surf_flags = s.u32_at(row, s.layout(40, 44)).ok()?;
            let num_tris = s.i32_at(row, s.layout(4, 8)).ok()?.max(0) as usize;
            if num_tris > 1_000_000 {
                return None;
            }
            let tris = match s.ptr_at(row, 0).ok()? {
                ZonePtr::Offset(p) => read_iw5_xmodel_coll_tris(s, s.resolve_alias(p), num_tris),
                _ => Vec::new(),
            };
            Some(clipmap_iw4::XModelCollSurf {
                tris,
                midpoint,
                half_size,
                bone_idx,
                contents,
                surf_flags,
            })
        })
        .collect()
}

fn read_iw5_xmodel_coll_tris(
    s: &fastfile_iw5::ZoneStream<'_>,
    arr: fastfile_iw5::Ptr,
    num: usize,
) -> Vec<clipmap_iw4::XModelCollTri> {
    use fastfile_iw5::size as iw5sz;
    (0..num)
        .filter_map(|i| {
            let t = arr.at(i * iw5sz::XMODEL_COLL_TRI);
            let f = |off: usize| s.f32_at(t, off).ok();
            Some(clipmap_iw4::XModelCollTri {
                plane: [f(0)?, f(4)?, f(8)?, f(12)?],
                svec: [f(16)?, f(20)?, f(24)?, f(28)?],
                tvec: [f(32)?, f(36)?, f(40)?, f(44)?],
            })
        })
        .collect()
}

fn extract_bsp_tables(
    s: &ZoneStream<'_>,
    g: ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let Some(nodes_ptr) = g.nodes else {
        return Ok(());
    };
    let Some(leaves_ptr) = g.leaves else {
        return Ok(());
    };
    let Some(leafbrushes_ptr) = g.leafbrushes else {
        return Ok(());
    };

    out.nodes.reserve(g.node_count);
    for i in 0..g.node_count {
        let node = nodes_ptr.at(i * s.layout(sz::C_NODE, 16));
        let plane = match s
            .ptr_at(node, 0)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            ZonePtr::Offset(p) => [
                s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
            ],
            _ => [0.0, 0.0, 1.0, 0.0],
        };
        let c0 = s
            .i16_at(node, s.layout(4, 8))
            .map_err(|_| ClipCollisionError::Truncated)? as i32;
        let c1 = s
            .i16_at(node, s.layout(6, 10))
            .map_err(|_| ClipCollisionError::Truncated)? as i32;
        out.nodes.push(ClipBspNode {
            plane,
            children: [c0, c1],
        });
    }

    let lb_nodes = g.leafbrush_nodes;
    out.leaves.reserve(g.leaf_count);
    for i in 0..g.leaf_count {
        let leaf = leaves_ptr.at(i * sz::C_LEAF);
        let lb_index = s
            .i32_at(leaf, 36)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let first_len = out.leafbrushes.len();
        if lb_index >= 0 {
            if let Some(lb_base) = lb_nodes {
                append_leafbrush_node(s, lb_base, lb_index as usize, &mut out.leafbrushes)?;
            }
        }
        let (first, num) = leafbrush_range(first_len, out.leafbrushes.len())?;
        let first_coll_aabb_index = s
            .u16_at(leaf, 0)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let coll_aabb_count = s
            .u16_at(leaf, 2)
            .map_err(|_| ClipCollisionError::Truncated)?;
        out.leaves.push(ClipBspLeaf {
            first_brush: first,
            num_brushes: num,
            first_coll_aabb_index,
            coll_aabb_count,
        });
    }

    if out.leafbrushes.is_empty() && g.leafbrush_count > 0 {
        for i in 0..g.leafbrush_count {
            let id = s
                .u16_at(leafbrushes_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            out.leafbrushes.push(id);
        }
    }

    Ok(())
}

const LEAFBRUSH_NODE_MAX_DEPTH: usize = 64;

fn append_leafbrush_node(
    s: &ZoneStream<'_>,
    lb_base: Ptr,
    index: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    append_leafbrush_node_at(s, lb_base, index, 0, out)
}

fn append_leafbrush_node_at(
    s: &ZoneStream<'_>,
    lb_base: Ptr,
    index: usize,
    depth: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    if depth > LEAFBRUSH_NODE_MAX_DEPTH {
        return Ok(());
    }
    let node = lb_base.at(index * s.layout(sz::C_LEAF_BRUSH_NODE, 24));
    let count = s
        .i16_at(node, 2)
        .map_err(|_| ClipCollisionError::Truncated)?;
    if count > 0 {
        if let ZonePtr::Offset(brushes) = s
            .ptr_at(node, 8)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            for j in 0..count as usize {
                out.push(
                    s.u16_at(brushes, j * 2)
                        .map_err(|_| ClipCollisionError::Truncated)?,
                );
            }
        }
        return Ok(());
    }

    let off0 = s
        .u16_at(node, 16)
        .map_err(|_| ClipCollisionError::Truncated)? as usize;
    let off1 = s
        .u16_at(node, 18)
        .map_err(|_| ClipCollisionError::Truncated)? as usize;
    for next in leafbrush_node_visits(index, count, off0, off1) {
        append_leafbrush_node_at(s, lb_base, next, depth + 1, out)?;
    }
    Ok(())
}

fn leafbrush_node_visits(index: usize, count: i16, off0: usize, off1: usize) -> Vec<usize> {
    let mut visits = Vec::with_capacity(3);
    if count < 0 {
        visits.push(index + 1);
    }

    if off0 != 0 {
        visits.push(index + off0);
    }
    if off1 != 0 {
        visits.push(index + off1);
    }
    visits
}

fn side_first(
    s: &ZoneStream<'_>,
    brush: Ptr,
    sides_all: Option<Ptr>,
) -> Result<Option<usize>, ClipCollisionError> {
    let Some(sides_all) = sides_all else {
        return Ok(None);
    };
    match s
        .ptr_at(brush, s.layout(4, 8))
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ZonePtr::Offset(p) if p.block == sides_all.block && p.offset >= sides_all.offset => Ok(
            Some((p.offset - sides_all.offset) as usize / s.layout(sz::CBRUSH_SIDE, 16)),
        ),
        _ => Ok(None),
    }
}

fn read_side_plane(s: &ZoneStream<'_>, side: Ptr) -> Result<Option<[f32; 4]>, ClipCollisionError> {
    match s
        .ptr_at(side, 0)
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ZonePtr::Offset(plane) => Ok(Some([
            s.f32_at(plane, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ])),
        _ => Ok(None),
    }
}

fn material_flags(s: &ZoneStream<'_>, materials: Option<Ptr>, index: usize) -> u32 {
    let Some(base) = materials else {
        return 0;
    };
    s.u32_at(
        base.at(index * s.layout(sz::CLIP_MATERIAL, 16)),
        s.layout(4, 8),
    )
    .unwrap_or(0)
}

fn axial_plane_flags(axial_mat: [u16; 6], flag_of: impl Fn(usize) -> u32) -> [u32; 6] {
    [
        flag_of(axial_mat[3] as usize),
        flag_of(axial_mat[0] as usize),
        flag_of(axial_mat[4] as usize),
        flag_of(axial_mat[1] as usize),
        flag_of(axial_mat[5] as usize),
        flag_of(axial_mat[2] as usize),
    ]
}

pub fn build_iw5_clip_collision(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
) -> Result<ClipCollision, ClipCollisionError> {
    use fastfile_iw5::size as iw5sz;

    let brushes_ptr = g.brushes.ok_or(ClipCollisionError::MissingTables)?;
    let bounds_ptr = g.brush_bounds.ok_or(ClipCollisionError::MissingTables)?;
    let contents_ptr = g.brush_contents.ok_or(ClipCollisionError::MissingTables)?;
    let sides_base = g.brush_sides;
    let materials = g.materials;

    let mut out = ClipCollision {
        brushes: Vec::with_capacity(g.brush_count),
        ..ClipCollision::default()
    };

    for i in 0..g.brush_count {
        let brush = brushes_ptr.at(i * s.layout(iw5sz::CBRUSH, 48));
        let bound = bounds_ptr.at(i * iw5sz::BOUNDS);
        let contents = s
            .u32_at(contents_ptr, i * 4)
            .map_err(|_| ClipCollisionError::Truncated)?;

        let mid = [
            s.f32_at(bound, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let half = [
            s.f32_at(bound, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 16)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(bound, 20)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let mins = [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]];
        let maxs = [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]];

        let mut planes = Vec::with_capacity(6);
        planes.push([1.0, 0.0, 0.0, maxs[0]]);
        planes.push([-1.0, 0.0, 0.0, -mins[0]]);
        planes.push([0.0, 1.0, 0.0, maxs[1]]);
        planes.push([0.0, -1.0, 0.0, -mins[1]]);
        planes.push([0.0, 0.0, 1.0, maxs[2]]);
        planes.push([0.0, 0.0, -1.0, -mins[2]]);

        let mut axial_mat = [0u16; 6];
        for i in 0..6 {
            axial_mat[i] = s
                .u16_at(brush, s.layout(12, 24) + i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
        }
        let mut plane_surface_flags = Vec::with_capacity(6);
        plane_surface_flags.extend_from_slice(&axial_plane_flags(axial_mat, |idx| {
            iw5_material_flags(s, materials, idx)
        }));

        let numsides = s
            .u16_at(brush, 0)
            .map_err(|_| ClipCollisionError::Truncated)? as usize;

        let glass_encoded = s
            .u16_at(brush, 2)
            .map_err(|_| ClipCollisionError::Truncated)?;
        if let (Some(sides_all), Some(first)) = (sides_base, iw5_side_first(s, brush, sides_base)?)
        {
            for j in 0..numsides {
                let side = sides_all.at((first + j) * s.layout(iw5sz::CBRUSH_SIDE, 16));
                if let Some(plane) = iw5_read_side_plane(s, side)? {
                    planes.push(plane);
                    let mat = s
                        .u16_at(side, s.layout(4, 8))
                        .map_err(|_| ClipCollisionError::Truncated)?
                        as usize;
                    plane_surface_flags.push(iw5_material_flags(s, materials, mat));
                }
            }
        }

        debug_assert_eq!(planes.len(), plane_surface_flags.len());
        out.brushes.push(ClipBrush {
            planes,
            contents,
            plane_surface_flags,
            glass_encoded,
        });
    }

    extract_iw5_bsp_tables(s, g, &mut out)?;
    extract_iw5_mesh_tables(s, g, &mut out)?;
    extract_iw5_cmodels(s, g, &mut out)?;
    gate_leafbrush_index_fits(out.leafbrushes.len())?;

    Ok(out)
}

fn extract_iw5_bsp_tables(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    use fastfile_iw5::ZonePtr;
    use fastfile_iw5::size as iw5sz;

    let Some(nodes_ptr) = g.nodes else {
        return Ok(());
    };
    let Some(leaves_ptr) = g.leaves else {
        return Ok(());
    };
    let Some(leafbrushes_ptr) = g.leafbrushes else {
        return Ok(());
    };

    out.nodes.reserve(g.node_count);
    for i in 0..g.node_count {
        let node = nodes_ptr.at(i * s.layout(iw5sz::C_NODE, 16));
        let plane = match s
            .ptr_at(node, 0)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            ZonePtr::Offset(p) => [
                s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
            ],
            _ => [0.0, 0.0, 1.0, 0.0],
        };
        let children = s.layout(4, 8);
        let c0 = s
            .i16_at(node, children)
            .map_err(|_| ClipCollisionError::Truncated)? as i32;
        let c1 = s
            .i16_at(node, children + 2)
            .map_err(|_| ClipCollisionError::Truncated)? as i32;
        out.nodes.push(ClipBspNode {
            plane,
            children: [c0, c1],
        });
    }

    let lb_nodes = g.leafbrush_nodes;
    out.leaves.reserve(g.leaf_count);
    for i in 0..g.leaf_count {
        let leaf = leaves_ptr.at(i * iw5sz::C_LEAF);
        let lb_index = s
            .i32_at(leaf, 36)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let first_len = out.leafbrushes.len();
        if lb_index >= 0 {
            if let Some(lb_base) = lb_nodes {
                append_iw5_leafbrush_node(s, lb_base, lb_index as usize, &mut out.leafbrushes)?;
            }
        }
        let (first, num) = leafbrush_range(first_len, out.leafbrushes.len())?;
        let first_coll_aabb_index = s
            .u16_at(leaf, 0)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let coll_aabb_count = s
            .u16_at(leaf, 2)
            .map_err(|_| ClipCollisionError::Truncated)?;
        out.leaves.push(ClipBspLeaf {
            first_brush: first,
            num_brushes: num,
            first_coll_aabb_index,
            coll_aabb_count,
        });
    }

    if out.leafbrushes.is_empty() && g.leafbrush_count > 0 {
        for i in 0..g.leafbrush_count {
            let id = s
                .u16_at(leafbrushes_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            out.leafbrushes.push(id);
        }
    }

    Ok(())
}

fn append_iw5_leafbrush_node(
    s: &fastfile_iw5::ZoneStream<'_>,
    lb_base: fastfile_iw5::Ptr,
    index: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    append_iw5_leafbrush_node_at(s, lb_base, index, 0, out)
}

fn append_iw5_leafbrush_node_at(
    s: &fastfile_iw5::ZoneStream<'_>,
    lb_base: fastfile_iw5::Ptr,
    index: usize,
    depth: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    use fastfile_iw5::ZonePtr;
    use fastfile_iw5::size as iw5sz;

    if depth > LEAFBRUSH_NODE_MAX_DEPTH {
        return Ok(());
    }
    let node = lb_base.at(index * s.layout(iw5sz::C_LEAF_BRUSH_NODE, 24));
    let count = s
        .i16_at(node, 2)
        .map_err(|_| ClipCollisionError::Truncated)?;
    if count > 0 {
        if let ZonePtr::Offset(brushes) = s
            .ptr_at(node, 8)
            .map_err(|_| ClipCollisionError::Truncated)?
        {
            for j in 0..count as usize {
                out.push(
                    s.u16_at(brushes, j * 2)
                        .map_err(|_| ClipCollisionError::Truncated)?,
                );
            }
        }
        return Ok(());
    }
    let off0 = s
        .u16_at(node, 16)
        .map_err(|_| ClipCollisionError::Truncated)? as usize;
    let off1 = s
        .u16_at(node, 18)
        .map_err(|_| ClipCollisionError::Truncated)? as usize;
    for next in leafbrush_node_visits(index, count, off0, off1) {
        append_iw5_leafbrush_node_at(s, lb_base, next, depth + 1, out)?;
    }
    Ok(())
}

fn extract_iw5_mesh_tables(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    let Some(verts_ptr) = g.verts else {
        return Ok(());
    };
    mesh.verts.reserve(g.vert_count);
    for i in 0..g.vert_count {
        let v = verts_ptr.at(i * 12);
        mesh.verts.push([
            s.f32_at(v, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ]);
    }
    if let Some(idx_ptr) = g.tri_indices {
        let n = g.tri_count.saturating_mul(3);
        mesh.tri_indices.reserve(n);
        for i in 0..n {
            let id = s
                .u16_at(idx_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_indices.push(id);
        }
    }
    if let Some(walk_ptr) = g.tri_edge_is_walkable {
        let n = g.tri_count.saturating_mul(3).div_ceil(32) * 4;
        mesh.tri_edge_is_walkable.reserve(n);
        for i in 0..n {
            let b = s
                .u8_at(walk_ptr, i)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_edge_is_walkable.push(b);
        }
    }
    extract_iw5_mesh_materials(s, g, out)?;
    extract_iw5_aabb_forest(s, g, out)?;
    Ok(())
}

fn extract_iw5_aabb_forest(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    use fastfile_iw5::size as iw5sz;

    if let Some(parts) = g.collision_partitions {
        mesh.partitions.reserve(g.partition_count);
        for i in 0..g.partition_count {
            let p = parts.at(i * s.layout(iw5sz::COLLISION_PARTITION, 16));
            let tri_n = s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
            let seg = s.u8_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
            let first = s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?;
            mesh.partitions.push(clipmap_iw4::ClipPartition {
                tri_count: tri_n,
                first_tri: first,
                first_vert_segment: seg,
            });
        }
    }
    if let Some(trees) = g.collision_aabb_trees {
        mesh.aabb_trees.reserve(g.aabb_tree_count);
        for i in 0..g.aabb_tree_count {
            let node = trees.at(i * iw5sz::COLLISION_AABB_TREE);
            let origin = [
                s.f32_at(node, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 4)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 8)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ];
            let material_index = s
                .u16_at(node, 12)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let child_count = s
                .u16_at(node, 14)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let half_size = [
                s.f32_at(node, 16)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 20)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(node, 24)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ];
            let u = s
                .i32_at(node, 28)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.aabb_trees.push(clipmap_iw4::ClipAabbNode {
                origin,
                half_size,
                material_index,
                child_count,
                u,
            });
        }
    }
    mesh.aabb_roots = clipmap_iw4::aabb_forest_roots(&mesh.aabb_trees);
    Ok(())
}

fn extract_iw5_mesh_materials(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    use fastfile_iw5::ZonePtr;
    use fastfile_iw5::size as iw5sz;

    out.materials = {
        let mut mats = Vec::with_capacity(g.material_count);
        if let Some(base) = g.materials {
            for i in 0..g.material_count {
                let slot = base.at(i * s.layout(iw5sz::CLIP_MATERIAL, 16));
                let name = match s
                    .ptr_at(slot, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?
                {
                    ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).unwrap_or("").to_owned(),
                    _ => String::new(),
                };
                let surface_flags = s
                    .u32_at(slot, s.layout(4, 8))
                    .map_err(|_| ClipCollisionError::Truncated)?;
                let content_flags = s
                    .u32_at(slot, s.layout(8, 12))
                    .map_err(|_| ClipCollisionError::Truncated)?;
                mats.push(ClipMapMaterial {
                    name,
                    surface_flags,
                    content_flags,
                });
            }
        } else {
            mats.resize(g.material_count, ClipMapMaterial::default());
        }
        mats
    };
    if g.tri_count == 0 {
        return Ok(());
    }
    let mut material_sflags = vec![0u32; g.material_count];
    let mut material_cflags = vec![0u32; g.material_count];
    for (i, mat) in out.materials.iter().enumerate() {
        if let Some(slot) = material_sflags.get_mut(i) {
            *slot = mat.surface_flags;
        }
        if let Some(slot) = material_cflags.get_mut(i) {
            *slot = mat.content_flags;
        }
    }
    let mut partitions = vec![(0u8, 0i32); g.partition_count];
    if let Some(parts) = g.collision_partitions {
        for (i, slot) in partitions.iter_mut().enumerate() {
            let p = parts.at(i * s.layout(iw5sz::COLLISION_PARTITION, 16));
            let tri_n = s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
            let first = s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?;
            *slot = (tri_n, first);
        }
    }
    let mut leaves = Vec::new();
    if let Some(trees) = g.collision_aabb_trees {
        for i in 0..g.aabb_tree_count {
            let node = trees.at(i * iw5sz::COLLISION_AABB_TREE);
            let child_count = s
                .u16_at(node, 14)
                .map_err(|_| ClipCollisionError::Truncated)?;
            if child_count != 0 {
                continue;
            }
            let mat = s
                .u16_at(node, 12)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let part = s
                .i32_at(node, 28)
                .map_err(|_| ClipCollisionError::Truncated)?;
            leaves.push((mat, part));
        }
    }
    mesh.tri_surface_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &material_sflags, g.tri_count);
    mesh.tri_content_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &material_cflags, g.tri_count);
    out.tri_material_index =
        clipmap_iw4::flatten_tri_material_index(&leaves, &partitions, g.tri_count);
    Ok(())
}

fn extract_iw5_cmodels(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    use fastfile_iw5::size as iw5sz;

    let Some(cmodels_ptr) = g.cmodels else {
        return Ok(());
    };
    let lb_nodes = g.leafbrush_nodes;
    out.cmodels.reserve(g.cmodel_count);
    for i in 0..g.cmodel_count {
        let cm = cmodels_ptr.at(i * s.layout(iw5sz::CMODEL, 80));
        let mid = [
            s.f32_at(cm, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let half = [
            s.f32_at(cm, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 16)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(cm, 20)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let mins = [mid[0] - half[0], mid[1] - half[1], mid[2] - half[2]];
        let maxs = [mid[0] + half[0], mid[1] + half[1], mid[2] + half[2]];
        let radius = s
            .f32_at(cm, 24)
            .map_err(|_| ClipCollisionError::Truncated)?;
        let lb_index = s
            .i32_at(cm, s.layout(0x44, 76))
            .map_err(|_| ClipCollisionError::Truncated)?;
        let first_len = out.leafbrushes.len();
        if lb_index >= 0 {
            if let Some(lb_base) = lb_nodes {
                if append_iw5_leafbrush_node(s, lb_base, lb_index as usize, &mut out.leafbrushes)
                    .is_err()
                {
                    out.leafbrushes.truncate(first_len);
                }
            }
        }
        let (first, num) = leafbrush_range(first_len, out.leafbrushes.len())?;
        out.cmodels.push(ClipCmodel {
            mins,
            maxs,
            radius,
            first_brush: first,
            num_brushes: num,
        });
    }
    Ok(())
}

fn extract_iw5_static_models(
    s: &fastfile_iw5::ZoneStream<'_>,
    g: fastfile_iw5::ClipMapGeometry,
    catalog: &XModelCollCatalog,
    out: &mut ClipCollision,
) {
    use fastfile_iw5::size as iw5sz;
    let Some(arr) = g.static_models else {
        return;
    };
    out.static_models.reserve(g.static_model_count);
    for i in 0..g.static_model_count {
        let sm = arr.at(i * s.layout(iw5sz::C_STATIC_MODEL, 80));
        if let Some(placed) = read_iw5_placed_static_model(s, sm, i as u32, catalog) {
            out.static_models.push(placed);
        }
    }
}

fn read_iw5_placed_static_model(
    s: &fastfile_iw5::ZoneStream<'_>,
    sm: fastfile_iw5::Ptr,
    index: u32,
    catalog: &XModelCollCatalog,
) -> Option<ClipPlacedStaticModel> {
    use fastfile_iw5::ZonePtr;
    let body = match s.ptr_at(sm, 0).ok()? {
        ZonePtr::Offset(p) => s.resolve_alias(p),
        _ => return None,
    };
    let captured = catalog.get_at(body.block, body.offset)?;
    let origin_off = s.layout(4, 8);
    let origin = [
        s.f32_at(sm, origin_off).ok()?,
        s.f32_at(sm, origin_off + 4).ok()?,
        s.f32_at(sm, origin_off + 8).ok()?,
    ];
    let axis_off = s.layout(0x10, 20);
    let mut inv_scaled_axis = [[0.0_f32; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            inv_scaled_axis[row][col] = s.f32_at(sm, axis_off + (row * 3 + col) * 4).ok()?;
        }
    }
    let abs_bounds = s.layout(0x34, 56);
    let bounds_mid = [
        s.f32_at(sm, abs_bounds).ok()?,
        s.f32_at(sm, abs_bounds + 4).ok()?,
        s.f32_at(sm, abs_bounds + 8).ok()?,
    ];
    let bounds_half = [
        s.f32_at(sm, abs_bounds + 12).ok()?,
        s.f32_at(sm, abs_bounds + 16).ok()?,
        s.f32_at(sm, abs_bounds + 20).ok()?,
    ];
    if !origin.iter().all(|v| v.is_finite())
        || !bounds_mid.iter().all(|v| v.is_finite())
        || !bounds_half.iter().all(|v| v.is_finite() && *v >= 0.0)
    {
        return None;
    }
    Some(ClipPlacedStaticModel {
        index,
        name: captured.name.clone(),
        model: clipmap_iw4::ClipStaticModel {
            origin,
            inv_scaled_axis,
            bounds_mid,
            bounds_half,
            coll: captured.coll.clone(),
        },
    })
}

fn iw5_side_first(
    s: &fastfile_iw5::ZoneStream<'_>,
    brush: fastfile_iw5::Ptr,
    sides_all: Option<fastfile_iw5::Ptr>,
) -> Result<Option<usize>, ClipCollisionError> {
    use fastfile_iw5::ZonePtr;
    use fastfile_iw5::size as iw5sz;

    let Some(sides_all) = sides_all else {
        return Ok(None);
    };
    match s
        .ptr_at(brush, s.layout(4, 8))
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ZonePtr::Offset(p) if p.block == sides_all.block && p.offset >= sides_all.offset => Ok(
            Some((p.offset - sides_all.offset) as usize / s.layout(iw5sz::CBRUSH_SIDE, 16)),
        ),
        _ => Ok(None),
    }
}

fn iw5_read_side_plane(
    s: &fastfile_iw5::ZoneStream<'_>,
    side: fastfile_iw5::Ptr,
) -> Result<Option<[f32; 4]>, ClipCollisionError> {
    use fastfile_iw5::ZonePtr;

    match s
        .ptr_at(side, 0)
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        ZonePtr::Offset(plane) => Ok(Some([
            s.f32_at(plane, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ])),
        _ => Ok(None),
    }
}

fn iw5_material_flags(
    s: &fastfile_iw5::ZoneStream<'_>,
    materials: Option<fastfile_iw5::Ptr>,
    index: usize,
) -> u32 {
    use fastfile_iw5::size as iw5sz;

    let Some(base) = materials else {
        return 0;
    };
    s.u32_at(
        base.at(index * s.layout(iw5sz::CLIP_MATERIAL, 16)),
        s.layout(4, 8),
    )
    .unwrap_or(0)
}

const T5_BRUSH: usize = 96;
const T5_BRUSH_MINS: usize = 0;
const T5_BRUSH_CONTENTS: usize = 12;
const T5_BRUSH_MAXS: usize = 16;
const T5_BRUSH_NUMSIDES: usize = 28;
const T5_BRUSH_SIDES: usize = 32;

const T5_BRUSH_AXIAL_SFLAGS: usize = 0x3c;
const T5_CBRUSH_SIDE: usize = 12;

const T5_CBRUSH_SIDE_SFLAGS: usize = 8;

pub fn build_t5_clip_collision(
    s: &fastfile_t5::ZoneStream<'_>,
    g: fastfile_t5::ClipMapGeometry,
) -> Result<ClipCollision, ClipCollisionError> {
    let brushes_ptr = g.brushes.ok_or(ClipCollisionError::MissingTables)?;
    let mut out = ClipCollision {
        brushes: Vec::with_capacity(g.brush_count),
        ..ClipCollision::default()
    };

    for i in 0..g.brush_count {
        let brush = brushes_ptr.at(i * T5_BRUSH);
        let mins = [
            s.f32_at(brush, T5_BRUSH_MINS)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MINS + 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MINS + 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let maxs = [
            s.f32_at(brush, T5_BRUSH_MAXS)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MAXS + 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(brush, T5_BRUSH_MAXS + 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ];
        let contents = s
            .u32_at(brush, T5_BRUSH_CONTENTS)
            .map_err(|_| ClipCollisionError::Truncated)?;

        let mut planes = Vec::with_capacity(6);
        planes.push([1.0, 0.0, 0.0, maxs[0]]);
        planes.push([-1.0, 0.0, 0.0, -mins[0]]);
        planes.push([0.0, 1.0, 0.0, maxs[1]]);
        planes.push([0.0, -1.0, 0.0, -mins[1]]);
        planes.push([0.0, 0.0, 1.0, maxs[2]]);
        planes.push([0.0, 0.0, -1.0, -mins[2]]);

        let mut axial_sflags = [0u32; 6];
        for (i, slot) in axial_sflags.iter_mut().enumerate() {
            *slot = s
                .u32_at(brush, T5_BRUSH_AXIAL_SFLAGS + i * 4)
                .map_err(|_| ClipCollisionError::Truncated)?;
        }
        let mut plane_surface_flags = Vec::with_capacity(6);
        plane_surface_flags.extend_from_slice(&t5_axial_plane_flags(axial_sflags));

        let numsides = s
            .u32_at(brush, T5_BRUSH_NUMSIDES)
            .map_err(|_| ClipCollisionError::Truncated)? as usize;
        if let Ok(fastfile_t5::ZonePtr::Offset(sides)) = s.ptr_at(brush, T5_BRUSH_SIDES) {
            for j in 0..numsides {
                let side = sides.at(j * T5_CBRUSH_SIDE);
                if let Some(plane) = read_t5_side_plane(s, side)? {
                    planes.push(plane);
                    plane_surface_flags.push(
                        s.u32_at(side, T5_CBRUSH_SIDE_SFLAGS)
                            .map_err(|_| ClipCollisionError::Truncated)?,
                    );
                }
            }
        }

        debug_assert_eq!(planes.len(), plane_surface_flags.len());
        out.brushes.push(ClipBrush {
            planes,
            contents,
            plane_surface_flags,

            glass_encoded: 0,
        });
    }

    extract_t5_mesh_tables(s, g, &mut out)?;
    extract_t5_bsp_tables(s, g, &mut out)?;

    Ok(out)
}

fn t5_axial_plane_flags(axial_sflags: [u32; 6]) -> [u32; 6] {
    [
        axial_sflags[3],
        axial_sflags[0],
        axial_sflags[4],
        axial_sflags[1],
        axial_sflags[5],
        axial_sflags[2],
    ]
}

fn extract_t5_mesh_tables(
    s: &fastfile_t5::ZoneStream<'_>,
    g: fastfile_t5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    let Some(verts_ptr) = g.verts else {
        return Ok(());
    };
    mesh.verts.reserve(g.vert_count);
    for i in 0..g.vert_count {
        let v = verts_ptr.at(i * 12);
        mesh.verts.push([
            s.f32_at(v, 0).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 4).map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(v, 8).map_err(|_| ClipCollisionError::Truncated)?,
        ]);
    }
    if let Some(idx_ptr) = g.tri_indices {
        let n = g.tri_count.saturating_mul(3);
        mesh.tri_indices.reserve(n);
        for i in 0..n {
            let id = s
                .u16_at(idx_ptr, i * 2)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_indices.push(id);
        }
    }
    if let Some(walk_ptr) = g.tri_edge_is_walkable {
        let n = g.tri_count.saturating_mul(3).div_ceil(32) * 4;
        mesh.tri_edge_is_walkable.reserve(n);
        for i in 0..n {
            let b = s
                .u8_at(walk_ptr, i)
                .map_err(|_| ClipCollisionError::Truncated)?;
            mesh.tri_edge_is_walkable.push(b);
        }
    }
    use fastfile_t5::size as t5;
    if let Some(materials) = g.materials {
        for i in 0..g.material_count {
            let p = materials.at(i * t5::DMATERIAL);
            let bytes = s
                .slice_at(p, 0, 64)
                .map_err(|_| ClipCollisionError::Truncated)?;
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            out.materials.push(ClipMapMaterial {
                name: String::from_utf8_lossy(&bytes[..end]).into_owned(),
                surface_flags: s.u32_at(p, 64).map_err(|_| ClipCollisionError::Truncated)?,
                content_flags: s.u32_at(p, 68).map_err(|_| ClipCollisionError::Truncated)?,
            });
        }
    }
    if let Some(parts) = g.collision_partitions {
        for i in 0..g.partition_count {
            let p = parts.at(i * t5::COLLISION_PARTITION);
            mesh.partitions.push(clipmap_iw4::ClipPartition {
                tri_count: s.u8_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                first_tri: s.i32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                first_vert_segment: 0,
            });
        }
    }
    if let Some(trees) = g.collision_aabb_trees {
        for i in 0..g.aabb_tree_count {
            let p = trees.at(i * t5::COLLISION_AABB_TREE);
            mesh.aabb_trees.push(clipmap_iw4::ClipAabbNode {
                origin: [
                    s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                half_size: [
                    s.f32_at(p, 16).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 20).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 24).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                material_index: s.u16_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
                child_count: s.u16_at(p, 14).map_err(|_| ClipCollisionError::Truncated)?,
                u: s.i32_at(p, 28).map_err(|_| ClipCollisionError::Truncated)?,
            });
        }
    }
    let leaves: Vec<_> = mesh
        .aabb_trees
        .iter()
        .filter(|n| n.child_count == 0)
        .map(|n| (n.material_index, n.u))
        .collect();
    let partitions: Vec<_> = mesh
        .partitions
        .iter()
        .map(|p| (p.tri_count, p.first_tri))
        .collect();
    let sflags: Vec<_> = out.materials.iter().map(|m| m.surface_flags).collect();
    let cflags: Vec<_> = out.materials.iter().map(|m| m.content_flags).collect();
    mesh.tri_surface_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &sflags, g.tri_count);
    mesh.tri_content_flags =
        clipmap_iw4::flatten_tri_surface_flags(&leaves, &partitions, &cflags, g.tri_count);
    out.tri_material_index =
        clipmap_iw4::flatten_tri_material_index(&leaves, &partitions, g.tri_count);
    Ok(())
}

fn extract_t5_bsp_tables(
    s: &fastfile_t5::ZoneStream<'_>,
    g: fastfile_t5::ClipMapGeometry,
    out: &mut ClipCollision,
) -> Result<(), ClipCollisionError> {
    let mesh = std::sync::Arc::make_mut(&mut out.mesh);
    use fastfile_t5::{ZonePtr, size as sz};
    let (Some(nodes), Some(leaves), Some(lb)) = (g.nodes, g.leaves, g.leafbrush_nodes) else {
        return Err(ClipCollisionError::MissingTables);
    };
    for i in 0..g.node_count {
        let p = nodes.at(i * sz::C_NODE);
        let ZonePtr::Offset(plane) = s.ptr_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?
        else {
            return Err(ClipCollisionError::MissingTables);
        };
        out.nodes.push(ClipBspNode {
            plane: [
                s.f32_at(plane, 0)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(plane, 4)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(plane, 8)
                    .map_err(|_| ClipCollisionError::Truncated)?,
                s.f32_at(plane, 12)
                    .map_err(|_| ClipCollisionError::Truncated)?,
            ],
            children: [
                i32::from(s.i16_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?),
                i32::from(s.i16_at(p, 6).map_err(|_| ClipCollisionError::Truncated)?),
            ],
        });
    }
    let mut roots = std::collections::BTreeSet::new();
    for i in 0..g.leaf_count {
        let p = leaves.at(i * sz::C_LEAF);
        let first_len = out.leafbrushes.len();
        let node = s.i32_at(p, 36).map_err(|_| ClipCollisionError::Truncated)?;
        if node > 0 {
            append_t5_leafbrush_node(s, lb, node as usize, &mut out.leafbrushes)?;
        }
        let (first_brush, num_brushes) = leafbrush_range(first_len, out.leafbrushes.len())?;
        let first_coll_aabb_index = s.u16_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?;
        let coll_aabb_count = s.u16_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
        for id in u32::from(first_coll_aabb_index)
            ..u32::from(first_coll_aabb_index) + u32::from(coll_aabb_count)
        {
            roots.insert(u16::try_from(id).map_err(|_| ClipCollisionError::Truncated)?);
        }
        out.leaves.push(ClipBspLeaf {
            first_brush,
            num_brushes,
            first_coll_aabb_index,
            coll_aabb_count,
        });
    }
    mesh.aabb_roots = roots.into_iter().collect();
    if let Some(models) = g.cmodels {
        for i in 0..g.cmodel_count {
            let p = models.at(i * sz::C_MODEL);
            let first_len = out.leafbrushes.len();
            let node = s.i32_at(p, 64).map_err(|_| ClipCollisionError::Truncated)?;
            if node > 0 {
                append_t5_leafbrush_node(s, lb, node as usize, &mut out.leafbrushes)?;
            }
            let (first_brush, num_brushes) = leafbrush_range(first_len, out.leafbrushes.len())?;
            out.cmodels.push(ClipCmodel {
                mins: [
                    s.f32_at(p, 0).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 4).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                maxs: [
                    s.f32_at(p, 12).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 16).map_err(|_| ClipCollisionError::Truncated)?,
                    s.f32_at(p, 20).map_err(|_| ClipCollisionError::Truncated)?,
                ],
                radius: s.f32_at(p, 24).map_err(|_| ClipCollisionError::Truncated)?,
                first_brush,
                num_brushes,
            });
        }
    }
    Ok(())
}

fn append_t5_leafbrush_node(
    s: &fastfile_t5::ZoneStream<'_>,
    base: fastfile_t5::Ptr,
    root: usize,
    out: &mut Vec<u16>,
) -> Result<(), ClipCollisionError> {
    let mut pending = vec![(root, 0)];
    while let Some((index, depth)) = pending.pop() {
        if depth > LEAFBRUSH_NODE_MAX_DEPTH {
            return Err(ClipCollisionError::Truncated);
        }
        let p = base.at(index * fastfile_t5::size::C_LEAF_BRUSH_NODE);
        let count = s.i16_at(p, 2).map_err(|_| ClipCollisionError::Truncated)?;
        if count > 0 {
            let fastfile_t5::ZonePtr::Offset(brushes) =
                s.ptr_at(p, 8).map_err(|_| ClipCollisionError::Truncated)?
            else {
                return Err(ClipCollisionError::MissingTables);
            };
            for i in 0..count as usize {
                out.push(
                    s.u16_at(brushes, i * 2)
                        .map_err(|_| ClipCollisionError::Truncated)?,
                );
            }
        } else {
            let a = s.u16_at(p, 16).map_err(|_| ClipCollisionError::Truncated)? as usize;
            let b = s.u16_at(p, 18).map_err(|_| ClipCollisionError::Truncated)? as usize;
            pending.extend(
                leafbrush_node_visits(index, count, a, b)
                    .into_iter()
                    .map(|i| (i, depth + 1)),
            );
        }
    }
    Ok(())
}

fn read_t5_side_plane(
    s: &fastfile_t5::ZoneStream<'_>,
    side: fastfile_t5::Ptr,
) -> Result<Option<[f32; 4]>, ClipCollisionError> {
    match s
        .ptr_at(side, 0)
        .map_err(|_| ClipCollisionError::Truncated)?
    {
        fastfile_t5::ZonePtr::Offset(plane) => Ok(Some([
            s.f32_at(plane, 0)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 4)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 8)
                .map_err(|_| ClipCollisionError::Truncated)?,
            s.f32_at(plane, 12)
                .map_err(|_| ClipCollisionError::Truncated)?,
        ])),
        _ => Ok(None),
    }
}
