use core::fmt;
use dpvs_iw4::{GfxDrawSurf, fill_surface_materials};

use crate::SurfaceCastsSunShadow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialSortTrigger {
    UnreadIw4Caller,
}

impl MaterialSortTrigger {
    pub const IW4: Self = Self::UnreadIw4Caller;
}

#[derive(Clone, Debug, Default)]
pub struct WorldCapture {
    pub packed_draw_surfs: Vec<GfxDrawSurf>,
    pub casters: SurfaceCastsSunShadow,
    pub sort_trigger: Option<MaterialSortTrigger>,
}

impl WorldCapture {
    pub fn surface_count(&self) -> usize {
        self.packed_draw_surfs.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceMaterialStampError {
    CaptureLengthMismatch {
        packed: usize,
        casters: usize,
    },
    InputLengthMismatch {
        authored: usize,
        material_slots: usize,
        primary_lights: usize,
    },
}

impl fmt::Display for SurfaceMaterialStampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CaptureLengthMismatch { packed, casters } => write!(
                f,
                "world capture length mismatch: packed={packed} casters={casters}"
            ),
            Self::InputLengthMismatch {
                authored,
                material_slots,
                primary_lights,
            } => write!(
                f,
                "world surface stamp length mismatch: authored={authored} material_slots={material_slots} primary_lights={primary_lights}"
            ),
        }
    }
}

impl std::error::Error for SurfaceMaterialStampError {}

pub fn world_capture_from_casters(casters: SurfaceCastsSunShadow) -> WorldCapture {
    let n = casters.len();
    WorldCapture {
        packed_draw_surfs: vec![GfxDrawSurf::from_packed(0); n],
        casters,
        sort_trigger: Some(MaterialSortTrigger::IW4),
    }
}

pub fn stamp_packed_surface_materials(
    surface_material_slots: &[Option<usize>],
    surface_primary_lights: &[u8],
    baked_by_material_slot: &[Option<GfxDrawSurf>],
    capture: &mut WorldCapture,
) -> Result<(), SurfaceMaterialStampError> {
    let authored = capture.surface_count();
    let casters = capture.casters.len();
    if authored != casters {
        return Err(SurfaceMaterialStampError::CaptureLengthMismatch {
            packed: authored,
            casters,
        });
    }
    if surface_material_slots.len() != authored || surface_primary_lights.len() != authored {
        return Err(SurfaceMaterialStampError::InputLengthMismatch {
            authored,
            material_slots: surface_material_slots.len(),
            primary_lights: surface_primary_lights.len(),
        });
    }
    fill_surface_materials(
        surface_material_slots,
        surface_primary_lights,
        baked_by_material_slot,
        &mut capture.packed_draw_surfs,
    );
    Ok(())
}
