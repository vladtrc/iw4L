use std::sync::Arc;

use asset_iw4::size::GFX_PACKED_VERTEX;
use bevy::prelude::*;
use fx_iw4::{
    FX_SPRITE_QUAD_LOCAL_XY, FxSpriteAtlasUv, fx_pack_code_mesh_vertex, fx_sprite_quad_indices,
    fx_trail_pack_normal, fx_trail_pack_texcoord,
};

use render_frame::{
    GfxMeshData, r_reserve_code_mesh, r_reserve_code_mesh_indices, r_reserve_code_mesh_verts,
    r_shrink_code_mesh_verts,
};

use render_frame::RetailPackedVertexRefusal;

#[derive(Clone, Debug)]
pub struct FxPassMaterial {
    pub color: Option<Handle<Image>>,

    pub sort_key: u8,

    pub material_sorted_index: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct FxSurfaceDraw {
    pub viewmodel: bool,
    pub material: u32,
    pub index_start: u32,
    pub index_count: u32,

    pub arg_start: u32,

    pub arg_count: u16,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct FxCodeMeshPlan {
    pub vertices: Arc<Vec<[u8; GFX_PACKED_VERTEX]>>,
    pub indices: Arc<Vec<u32>>,

    pub args: Vec<[f32; 4]>,
    pub materials: Vec<FxPassMaterial>,
    pub draws: Vec<FxSurfaceDraw>,
    pub revision: u64,
    pub miss_material: u32,

    pub mesh: GfxMeshData,

    pub overflow_n: u32,

    pub range_share: Option<Arc<Vec<(u32, u32)>>>,
}

impl FxCodeMeshPlan {
    fn verts_mut(&mut self) -> &mut Vec<[u8; GFX_PACKED_VERTEX]> {
        Arc::make_mut(&mut self.vertices)
    }

    fn inds_mut(&mut self) -> &mut Vec<u32> {
        Arc::make_mut(&mut self.indices)
    }

    pub fn clear(&mut self) {
        super::reset_rows(&mut self.vertices);
        super::reset_rows(&mut self.indices);
        self.range_share = None;
        self.args.clear();
        self.materials.clear();
        self.draws.clear();
        self.miss_material = 0;
        self.mesh = GfxMeshData::code_mesh();
        self.overflow_n = 0;
    }

    pub fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn publish_share(&mut self) {
        self.range_share = Some(super::publish_index_ranges(
            self.draws
                .iter()
                .map(|draw| (draw.index_start, draw.index_count)),
        ));
    }

    pub fn packed_rows(&self) -> &[[u8; GFX_PACKED_VERTEX]] {
        self.vertices.as_slice()
    }

    pub fn index_rows(&self) -> &[u32] {
        self.indices.as_slice()
    }

    pub fn exact_packed_vertices(
        &self,
    ) -> Result<&[[u8; GFX_PACKED_VERTEX]], RetailPackedVertexRefusal> {
        let table_stride =
            asset_iw4::vertex_decl::stream_extent(asset_iw4::vertex_decl::PACKED_VERTEX_TYPE, 0);
        if table_stride != Some(GFX_PACKED_VERTEX as u16) {
            return Err(RetailPackedVertexRefusal::RetailStrideMismatch { table_stride });
        }
        Ok(self.packed_rows())
    }

    pub fn push_quad(
        &mut self,
        transform: Transform,
        color_rgba: [u8; 4],
        uv: FxSpriteAtlasUv,
    ) -> bool {
        let Some(base) = r_reserve_code_mesh_verts(&mut self.mesh, 4) else {
            self.overflow_n = self.overflow_n.saturating_add(1);
            return false;
        };
        if r_reserve_code_mesh_indices(&mut self.mesh, 6).is_none() {
            r_shrink_code_mesh_verts(&mut self.mesh, 4);
            self.overflow_n = self.overflow_n.saturating_add(1);
            return false;
        }
        let tangent = axis_or(transform.rotation * Vec3::X, Vec3::X);
        let normal = axis_or(transform.rotation * Vec3::Z, Vec3::Z);
        let texcoord = |u: f32, v: f32| fx_trail_pack_texcoord(u, v);
        let normal_packed = fx_trail_pack_normal(normal.to_array());
        let tangent_packed = fx_trail_pack_normal(tangent.to_array());
        debug_assert_eq!(u32::from(base), self.vertices.len() as u32);

        let corners = uv.corners();
        for (i, local_xy) in FX_SPRITE_QUAD_LOCAL_XY.iter().enumerate() {
            let local = Vec3::new(local_xy[0], local_xy[1], 0.0);
            let uv = corners[i];
            let world = transform.transform_point(local);
            self.verts_mut().push(fx_pack_code_mesh_vertex(
                world.to_array(),
                color_rgba,
                texcoord(uv[0], uv[1]),
                normal_packed,
                tangent_packed,
            ));
        }
        self.inds_mut()
            .extend_from_slice(&fx_sprite_quad_indices(u32::from(base)));
        true
    }

    pub fn push_trail_vert(
        &mut self,
        xyz: [f32; 3],
        color_rgba: [u8; 4],
        texcoord_packed: u32,
        normal_packed: u32,
        tangent_packed: f32,
    ) -> bool {
        if r_reserve_code_mesh_verts(&mut self.mesh, 1).is_none() {
            self.overflow_n = self.overflow_n.saturating_add(1);
            return false;
        }
        self.verts_mut().push(fx_pack_code_mesh_vertex(
            xyz,
            color_rgba,
            texcoord_packed,
            normal_packed,
            tangent_packed.to_bits(),
        ));
        true
    }

    pub fn push_post_light(&mut self, tess: &fx_iw4::FxPostLightTess) -> bool {
        let Some((base, _, arg_base)) = r_reserve_code_mesh(&mut self.mesh, 16, 84, 2) else {
            self.overflow_n = self.overflow_n.saturating_add(1);
            return false;
        };
        debug_assert_eq!(u32::from(base), self.vertices.len() as u32);
        for xyz in tess.verts {
            self.verts_mut()
                .push(fx_iw4::fx_post_light_pack_vert(xyz, tess.color_packed));
        }
        let b = u32::from(base);
        for idx in tess.indices {
            self.inds_mut().push(b.wrapping_add(u32::from(idx)));
        }
        debug_assert_eq!(arg_base, Some(self.args.len() as u32));
        self.args.extend_from_slice(&tess.args);
        true
    }

    pub fn extend_indices(&mut self, inds: &[u32]) -> bool {
        if r_reserve_code_mesh_indices(&mut self.mesh, inds.len() as u32).is_none() {
            self.overflow_n = self.overflow_n.saturating_add(1);
            return false;
        }
        self.inds_mut().extend_from_slice(inds);
        true
    }

    pub fn shrink_verts_to(&mut self, vert_used: u32) {
        let n = self.mesh.vert_used.wrapping_sub(vert_used);
        r_shrink_code_mesh_verts(&mut self.mesh, n);
        let vert_used = self.mesh.vert_used as usize;
        self.verts_mut().truncate(vert_used);
    }

    pub fn shrink_indices_to(&mut self, index_used: u32) {
        self.mesh.index_used = index_used;
        self.inds_mut().truncate(index_used as usize);
    }

    pub fn begin_material_draw(
        &mut self,
        color: Option<Handle<Image>>,
        sort_key: u8,
        material_sorted_index: Option<u32>,
    ) -> u32 {
        let material = self.materials.len() as u32;
        self.materials.push(FxPassMaterial {
            color,
            sort_key,
            material_sorted_index,
        });
        let index_start = self.indices.len() as u32;
        self.draws.push(FxSurfaceDraw {
            viewmodel: false,
            material,
            index_start,
            index_count: 0,
            arg_start: self.args.len() as u32,
            arg_count: 0,
        });
        self.draws.len() as u32 - 1
    }

    pub fn end_material_draw(&mut self, draw_slot: u32) {
        let Some(draw) = self.draws.get_mut(draw_slot as usize) else {
            return;
        };
        draw.index_count = (self.indices.len() as u32).saturating_sub(draw.index_start);
        draw.arg_count = (self.args.len() as u32)
            .saturating_sub(draw.arg_start)
            .min(u32::from(u16::MAX)) as u16;
    }
}

fn axis_or(v: Vec3, fallback: Vec3) -> Vec3 {
    let n = v.normalize_or_zero();
    if n == Vec3::ZERO { fallback } else { n }
}
