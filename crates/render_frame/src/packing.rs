use crate::{
    GfxSmodelRigidEntry, GfxTrianglesListEntry, GfxXModelRigidEntry, LIST_TOKEN,
    PackedFrontendLists, SMODEL_RIGID_ENTRY_STRIDE, SRC_SMODEL_CACHED_BYTES, SRC_SMODEL_CACHED_PTR,
    SRC_SMODEL_PRETESS_BYTES, SRC_SMODEL_PRETESS_PTR, SRC_SMODEL_RIGID_COUNT, SRC_SMODEL_RIGID_PTR,
    SRC_SMODEL_SKINNED_BYTES, SRC_SMODEL_SKINNED_PTR, SRC_WORLD_COUNT, SRC_WORLD_PTR,
    SRC_XMODEL_RIGID_COUNT, SRC_XMODEL_RIGID_PTR, SmodelPretessRange,
};
use dpvs_iw4::GfxDrawSurf;

pub const HOST_XMODEL_RIGID_TESS_INFO_PACKED_ARM: u8 = 1;

#[derive(Clone, Copy, Debug)]
pub enum PackKind {
    World {
        surf: u16,
        run: u16,
        run_off: u32,
    },
    SmodelRigid {
        surface: u32,
        lighting_handle: u32,
    },
    SmodelSkinned {
        surface: u32,
        lighting_handle: u32,
    },
    SmodelCached {
        lighting_handle: u32,
        dest: SmodelPretessRange,
    },
    SmodelPretess {
        lighting_handle: u32,
        dest: SmodelPretessRange,
    },
    XModel {
        surface: u32,
        lighting_handle: u32,
    },
    Skip,
}

#[derive(Clone, Copy, Debug)]
pub struct PackDraw {
    pub key: u64,

    pub material_rank: u32,
    pub kind: PackKind,
}

pub fn pack_sun_shadow_frontend(
    draws: &[PackDraw],
    world_run_surfs: &[u16],
    world_ranges: &[(u32, u32)],
    world_vertex_count: u32,
    smodel_ranges: &[(u32, u32)],
    xmodel_ranges: &[(u32, u32)],
) -> PackedFrontendLists {
    let mut packed = PackedFrontendLists::default();
    for (draw_index, draw) in draws.iter().enumerate() {
        match draw.kind {
            PackKind::World { surf, run, run_off } => {
                if run > 1 {
                    let start = run_off as usize;
                    let end = start.saturating_add(usize::from(run));
                    if let Some(surfs) = world_run_surfs.get(start..end) {
                        if surfs.len() == usize::from(run) {
                            for &s in surfs {
                                push_world(
                                    &mut packed,
                                    draw_index as u32,
                                    draw.key,
                                    draw.material_rank,
                                    s,
                                    world_ranges,
                                    world_vertex_count,
                                );
                            }
                            continue;
                        }
                    }
                }
                push_world(
                    &mut packed,
                    draw_index as u32,
                    draw.key,
                    draw.material_rank,
                    surf,
                    world_ranges,
                    world_vertex_count,
                );
            }
            PackKind::SmodelRigid {
                surface,
                lighting_handle,
            } => push_smodel(
                &mut packed.smodel,
                &mut packed.smodel_draw_indices,
                &mut packed.skipped_empty_ib,
                draw_index as u32,
                draw.key,
                draw.material_rank,
                surface,
                lighting_handle,
                smodel_ranges,
            ),
            PackKind::SmodelSkinned {
                surface,
                lighting_handle,
            } => push_smodel(
                &mut packed.smodel_skinned,
                &mut packed.smodel_skinned_draw_indices,
                &mut packed.skipped_empty_ib,
                draw_index as u32,
                draw.key,
                draw.material_rank,
                surface,
                lighting_handle,
                smodel_ranges,
            ),
            PackKind::SmodelPretess {
                lighting_handle,
                dest,
            } => push_smodel_dest(
                &mut packed.smodel_pretess,
                &mut packed.smodel_pretess_draw_indices,
                draw_index as u32,
                draw.key,
                draw.material_rank,
                lighting_handle,
                dest,
            ),
            PackKind::SmodelCached {
                lighting_handle,
                dest,
            } => push_smodel_dest(
                &mut packed.smodel_cached,
                &mut packed.smodel_cached_draw_indices,
                draw_index as u32,
                draw.key,
                draw.material_rank,
                lighting_handle,
                dest,
            ),
            PackKind::XModel {
                surface,
                lighting_handle,
            } => push_xmodel(
                &mut packed,
                draw_index as u32,
                draw.key,
                draw.material_rank,
                surface,
                lighting_handle,
                xmodel_ranges,
            ),
            PackKind::Skip => packed.skipped_other += 1,
        }
    }
    fill_src_count_stride(
        &mut packed.src,
        SRC_WORLD_PTR,
        SRC_WORLD_COUNT,
        packed.world.len() as u32,
    );
    fill_src_count_stride(
        &mut packed.src,
        SRC_SMODEL_RIGID_PTR,
        SRC_SMODEL_RIGID_COUNT,
        packed.smodel.len() as u32,
    );
    fill_src_count_stride(
        &mut packed.src,
        SRC_XMODEL_RIGID_PTR,
        SRC_XMODEL_RIGID_COUNT,
        packed.xmodel.len() as u32,
    );
    fill_src_byte_span(
        &mut packed.src,
        SRC_SMODEL_CACHED_PTR,
        SRC_SMODEL_CACHED_BYTES,
        packed.smodel_cached.len(),
    );
    fill_src_byte_span(
        &mut packed.src,
        SRC_SMODEL_PRETESS_PTR,
        SRC_SMODEL_PRETESS_BYTES,
        packed.smodel_pretess.len(),
    );
    fill_src_byte_span(
        &mut packed.src,
        SRC_SMODEL_SKINNED_PTR,
        SRC_SMODEL_SKINNED_BYTES,
        packed.smodel_skinned.len(),
    );
    packed
}

pub fn pack_spot_shadow_frontend(
    draws: &[PackDraw],
    world_run_surfs: &[u16],
    world_ranges: &[(u32, u32)],
    world_vertex_count: u32,
    smodel_ranges: &[(u32, u32)],
    xmodel_ranges: &[(u32, u32)],
) -> PackedFrontendLists {
    pack_sun_shadow_frontend(
        draws,
        world_run_surfs,
        world_ranges,
        world_vertex_count,
        smodel_ranges,
        xmodel_ranges,
    )
}

fn fill_src_count_stride(src: &mut [u32], ptr: usize, count: usize, n: u32) {
    if n == 0 {
        return;
    }
    src[ptr] = LIST_TOKEN;
    src[count] = n;
}

fn fill_src_byte_span(src: &mut [u32], ptr: usize, bytes: usize, n: usize) {
    if n == 0 {
        return;
    }
    src[ptr] = LIST_TOKEN;
    src[bytes] = (n * SMODEL_RIGID_ENTRY_STRIDE) as u32;
}

fn push_world(
    packed: &mut PackedFrontendLists,
    draw_index: u32,
    key: u64,
    material_rank: u32,
    surf: u16,
    world_ranges: &[(u32, u32)],
    world_vertex_count: u32,
) {
    let Some(&(index_start, index_count)) = world_ranges.get(usize::from(surf)) else {
        packed.skipped_empty_ib += 1;
        return;
    };
    if index_count < 3 {
        packed.skipped_empty_ib += 1;
        return;
    }
    packed.world.push(GfxTrianglesListEntry {
        sort_key: backend_setup_key(key, material_rank),
        tri_count: (index_count / 3) as u16,
        base_index: index_start,
        first_vertex: 0,
        vertex_count: world_vertex_count,
    });
    packed.world_draw_indices.push(draw_index);
}

fn push_smodel(
    dest: &mut Vec<GfxSmodelRigidEntry>,
    draw_indices: &mut Vec<u32>,
    skipped_empty_ib: &mut u32,
    draw_index: u32,
    key: u64,
    material_rank: u32,
    surface: u32,
    lighting_handle: u32,
    smodel_ranges: &[(u32, u32)],
) {
    let Some(&(index_start, index_count)) = smodel_ranges.get(surface as usize) else {
        *skipped_empty_ib += 1;
        return;
    };
    if index_count < 3 {
        *skipped_empty_ib += 1;
        return;
    }
    dest.push(GfxSmodelRigidEntry {
        packed_key: backend_setup_key(key, material_rank),
        tri_count: (index_count / 3) as u16,
        index_byte_offset: index_start.saturating_mul(2),
        lighting_handle: lighting_handle as u16,
    });
    draw_indices.push(draw_index);
}

fn push_smodel_dest(
    dest: &mut Vec<GfxSmodelRigidEntry>,
    draw_indices: &mut Vec<u32>,
    draw_index: u32,
    key: u64,
    material_rank: u32,
    lighting_handle: u32,
    range: SmodelPretessRange,
) {
    if range.count < 3 {
        return;
    }
    dest.push(GfxSmodelRigidEntry {
        packed_key: backend_setup_key(key, material_rank),
        tri_count: (range.count / 3) as u16,
        index_byte_offset: range.start.saturating_mul(2),
        lighting_handle: lighting_handle as u16,
    });
    draw_indices.push(draw_index);
}

fn push_xmodel(
    packed: &mut PackedFrontendLists,
    draw_index: u32,
    key: u64,
    material_rank: u32,
    surface: u32,
    lighting_handle: u32,
    xmodel_ranges: &[(u32, u32)],
) {
    let Some(&(index_start, index_count)) = xmodel_ranges.get(surface as usize) else {
        packed.skipped_empty_ib += 1;
        return;
    };
    if index_count < 3 {
        packed.skipped_empty_ib += 1;
        return;
    }
    packed.xmodel.push(GfxXModelRigidEntry::new(
        backend_setup_key(key, material_rank),
        index_start.saturating_mul(2),
        (index_count / 3) as u16,
        HOST_XMODEL_RIGID_TESS_INFO_PACKED_ARM,
        lighting_handle,
    ));
    packed.xmodel_draw_indices.push(draw_index);
}

fn backend_setup_key(key: u64, material_rank: u32) -> u32 {
    let draw = GfxDrawSurf { packed: key };

    (material_rank & 0x7fff)
        | (u32::from(draw.scene_light_index()) << 16)
        | (u32::from(draw.surf_type()) << 24)
}
