pub const MAX_DRAWSURFS: usize = 16384;

pub const SF_XMODEL_RIGID: u8 = 7;

pub const SF_XMODEL_RIGID_SKINNED: u8 = 8;

pub const SF_CODE_MESH: u8 = 10;

pub const SF_PARTICLE_CLOUD: u8 = 0xf;

pub const SF_MARK_MESH: u8 = 0xc;

pub const SF_GLASS_MESH: u8 = 0xb;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct GfxDrawSurf {
    pub packed: u64,
}

impl GfxDrawSurf {
    pub const fn from_packed(packed: u64) -> Self {
        Self { packed }
    }

    pub const fn object_id(self) -> u16 {
        (self.packed & 0xffff) as u16
    }

    pub const fn reflection_probe_index(self) -> u8 {
        ((self.packed >> 16) & 0xff) as u8
    }

    pub const fn has_gfx_ent_index(self) -> bool {
        ((self.packed >> 24) & 1) != 0
    }

    pub const fn custom_index(self) -> u8 {
        ((self.packed >> 25) & 0x1f) as u8
    }

    pub const fn material_sorted_index(self) -> u16 {
        ((self.packed >> 30) & 0xfff) as u16
    }

    pub const fn prepass(self) -> u8 {
        ((self.packed >> 42) & 0x3) as u8
    }

    pub const fn use_hero_lighting(self) -> bool {
        ((self.packed >> 44) & 1) != 0
    }

    pub const fn scene_light_index(self) -> u8 {
        ((self.packed >> 45) & 0xff) as u8
    }

    pub const fn surf_type(self) -> u8 {
        ((self.packed >> 53) & 0xf) as u8
    }

    pub const fn primary_sort_key(self) -> u8 {
        ((self.packed >> 57) & 0x3f) as u8
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxDrawSurfFields {
    pub object_id: u16,
    pub reflection_probe_index: u8,
    pub has_gfx_ent_index: bool,
    pub custom_index: u8,
    pub material_sorted_index: u16,
    pub prepass: u8,
    pub use_hero_lighting: bool,
    pub scene_light_index: u8,
    pub surf_type: u8,
    pub primary_sort_key: u8,
}

pub fn pack(fields: GfxDrawSurfFields) -> GfxDrawSurf {
    let mut p = 0u64;
    p |= u64::from(fields.object_id);
    p |= u64::from(fields.reflection_probe_index) << 16;
    p |= u64::from(fields.has_gfx_ent_index) << 24;
    p |= u64::from(fields.custom_index & 0x1f) << 25;
    p |= u64::from(fields.material_sorted_index & 0xfff) << 30;
    p |= u64::from(fields.prepass & 0x3) << 42;
    p |= u64::from(fields.use_hero_lighting) << 44;
    p |= u64::from(fields.scene_light_index) << 45;
    p |= u64::from(fields.surf_type & 0xf) << 53;
    p |= u64::from(fields.primary_sort_key & 0x3f) << 57;
    GfxDrawSurf { packed: p }
}

pub fn unpack(surf: GfxDrawSurf) -> GfxDrawSurfFields {
    GfxDrawSurfFields {
        object_id: surf.object_id(),
        reflection_probe_index: surf.reflection_probe_index(),
        has_gfx_ent_index: surf.has_gfx_ent_index(),
        custom_index: surf.custom_index(),
        material_sorted_index: surf.material_sorted_index(),
        prepass: surf.prepass(),
        use_hero_lighting: surf.use_hero_lighting(),
        scene_light_index: surf.scene_light_index(),
        surf_type: surf.surf_type(),
        primary_sort_key: surf.primary_sort_key(),
    }
}

pub fn with_object_id(base: GfxDrawSurf, object_id: u16) -> GfxDrawSurf {
    GfxDrawSurf {
        packed: (base.packed & !0xffff) | u64::from(object_id),
    }
}

pub fn with_primary_light_index(base: GfxDrawSurf, primary_light_index: u8) -> GfxDrawSurf {
    GfxDrawSurf {
        packed: (base.packed & !(0xffu64 << 45)) | (u64::from(primary_light_index) << 45),
    }
}

pub fn with_reflection_probe_index(base: GfxDrawSurf, reflection_probe_index: u8) -> GfxDrawSurf {
    GfxDrawSurf {
        packed: (base.packed & !(0xffu64 << 16)) | (u64::from(reflection_probe_index) << 16),
    }
}

pub fn pack_xmodel_rigid_draw_surf(material_baked: GfxDrawSurf, object_id: u16) -> GfxDrawSurf {
    pack_xmodel_draw_surf(material_baked, object_id, SF_XMODEL_RIGID)
}

pub fn pack_xmodel_rigid_skinned_draw_surf(
    material_baked: GfxDrawSurf,
    object_id: u16,
) -> GfxDrawSurf {
    pack_xmodel_draw_surf(material_baked, object_id, SF_XMODEL_RIGID_SKINNED)
}

fn pack_xmodel_draw_surf(
    material_baked: GfxDrawSurf,
    object_id: u16,
    surf_type: u8,
) -> GfxDrawSurf {
    const LOW_RETAIL_KEEP: u64 = 0xfeff_0000;
    const HIGH_RETAIL_KEEP: u64 = 0xfe1f_ffff;
    let low = (material_baked.packed & LOW_RETAIL_KEEP) | u64::from(object_id);
    let high =
        ((material_baked.packed >> 32) & HIGH_RETAIL_KEEP) | (u64::from(surf_type) << (53 - 32));
    GfxDrawSurf {
        packed: low | (high << 32),
    }
}

pub fn pack_code_mesh_draw_surf(
    sort_key: u8,
    material_sorted_index: u16,
    code_mesh_index: u16,
) -> GfxDrawSurf {
    pack(GfxDrawSurfFields {
        object_id: code_mesh_index,
        material_sorted_index: material_sorted_index & 0xfff,
        primary_sort_key: crate::material_sort_key_row(sort_key),
        surf_type: SF_CODE_MESH,
        ..Default::default()
    })
}

pub fn pack_particle_cloud_draw_surf(
    sort_key: u8,
    material_sorted_index: u16,
    cloud_index: u16,
) -> GfxDrawSurf {
    pack(GfxDrawSurfFields {
        object_id: cloud_index,
        material_sorted_index: material_sorted_index & 0xfff,
        primary_sort_key: crate::material_sort_key_row(sort_key),
        surf_type: SF_PARTICLE_CLOUD,
        ..Default::default()
    })
}

pub fn pack_mark_mesh_draw_surf(
    sort_key: u8,
    material_sorted_index: u16,
    mark_mesh_index: u16,
    lmap_index: u8,
    primary_light_index: u8,
    reflection_probe_index: u8,
) -> GfxDrawSurf {
    pack(GfxDrawSurfFields {
        object_id: mark_mesh_index,
        material_sorted_index: material_sorted_index & 0xfff,
        primary_sort_key: crate::material_sort_key_row(sort_key),
        surf_type: SF_MARK_MESH,
        custom_index: lmap_index,
        scene_light_index: primary_light_index,
        reflection_probe_index,
        ..Default::default()
    })
}

pub fn pack_glass_mesh_draw_surf(
    sort_key: u8,
    material_sorted_index: u16,
    glass_surf_index: u16,
    reflection_probe_index: u8,
) -> GfxDrawSurf {
    pack(GfxDrawSurfFields {
        object_id: glass_surf_index,
        reflection_probe_index,
        material_sorted_index: material_sorted_index & 0xfff,
        primary_sort_key: crate::material_sort_key_row(sort_key),
        surf_type: SF_GLASS_MESH,
        ..Default::default()
    })
}

pub fn sort_keys(keys: &mut [GfxDrawSurf]) {
    keys.sort_unstable_by_key(|k| k.packed);
}

pub fn material_rebind_count(sorted: &[GfxDrawSurf]) -> usize {
    let mut count = 0usize;
    let mut prev: Option<u16> = None;
    for k in sorted {
        let m = k.material_sorted_index();
        if prev != Some(m) {
            count += 1;
            prev = Some(m);
        }
    }
    count
}
