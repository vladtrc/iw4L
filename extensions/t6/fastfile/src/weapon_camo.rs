use crate::{AssetType, LoadedAsset, Ptr, WalkError, ZoneLoad};

#[derive(Clone, Copy)]
pub struct WeaponCamoView<'a> {
    load: &'a ZoneLoad,
    asset: &'a LoadedAsset,
    sets: Option<Ptr>,
    set_count: u32,
    materials: Option<Ptr>,
    material_count: u32,
}

#[derive(Clone, Copy)]
pub struct CamoImageSet<'a> {
    pub solid: Option<&'a LoadedAsset>,
    pub pattern: Option<&'a LoadedAsset>,
    pub offset: [f32; 2],
    pub scale: f32,
}

#[derive(Clone, Copy)]
pub struct CamoMaterialView<'a> {
    load: &'a ZoneLoad,
    owner: &'a LoadedAsset,
    address: Ptr,
    bytes: &'a [u8],
}

impl<'a> WeaponCamoView<'a> {
    pub fn new(load: &'a ZoneLoad, asset: &'a LoadedAsset) -> Result<Self, WalkError> {
        if asset.ty != AssetType::WeaponCamo || asset.header.len() != 28 {
            return Err(WalkError::Schema);
        }
        let word = |at| u32::from_le_bytes(asset.header[at..at + 4].try_into().unwrap());
        let sets = crate::walk::decode_ptr(word(12));
        let set_count = word(16);
        let materials = crate::walk::decode_ptr(word(20));
        let material_count = word(24);
        for (pointer, count, stride) in [(sets, set_count, 20usize), (materials, material_count, 8)]
        {
            if count != 0 {
                let len = (count as usize)
                    .checked_mul(stride)
                    .ok_or(WalkError::Schema)?;
                load.blocks.bytes(pointer.ok_or(WalkError::Schema)?, len)?;
            }
        }
        Ok(Self {
            load,
            asset,
            sets,
            set_count,
            materials,
            material_count,
        })
    }

    pub fn name(&self) -> Option<&'a str> {
        let raw = u32::from_le_bytes(self.asset.header[..4].try_into().ok()?);
        core::str::from_utf8(self.load.blocks.cstr(crate::walk::decode_ptr(raw)?).ok()?).ok()
    }

    pub fn solid_base(&self) -> Option<&'a LoadedAsset> {
        self.asset
            .field(4)
            .and_then(|index| self.load.assets.get(index))
    }

    pub fn pattern_base(&self) -> Option<&'a LoadedAsset> {
        self.asset
            .field(8)
            .and_then(|index| self.load.assets.get(index))
    }

    pub fn image_set_count(&self) -> u32 {
        self.set_count
    }

    pub fn image_set(&self, slot: u32) -> Option<CamoImageSet<'a>> {
        if slot >= self.set_count {
            return None;
        }
        let address = self.sets?.at(slot * 20);
        let bytes = self.load.blocks.bytes(address, 20).ok()?;
        let float = |at| f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        Some(CamoImageSet {
            solid: self.load.asset_in(self.asset, address),
            pattern: self.load.asset_in(self.asset, address.at(4)),
            offset: [float(8), float(12)],
            scale: float(16),
        })
    }

    pub fn material_set_count(&self) -> u32 {
        self.material_count
    }

    pub fn material_set(
        &self,
        slot: u32,
    ) -> Result<impl Iterator<Item = CamoMaterialView<'a>> + 'a, WalkError> {
        if slot >= self.material_count {
            return Err(WalkError::Schema);
        }
        let set = self.materials.ok_or(WalkError::Schema)?.at(slot * 8);
        let count = self.load.blocks.u32_at(set)?;
        let address = self.load.blocks.ptr_at(set.at(4))?;
        let bytes = if count == 0 {
            &[]
        } else {
            self.load.blocks.bytes(
                address.ok_or(WalkError::Schema)?,
                (count as usize).checked_mul(44).ok_or(WalkError::Schema)?,
            )?
        };
        let load = self.load;
        let owner = self.asset;
        Ok(bytes
            .as_chunks::<44>()
            .0
            .iter()
            .enumerate()
            .map(move |(i, bytes)| CamoMaterialView {
                load,
                owner,
                address: address.unwrap().at(i as u32 * 44),
                bytes,
            }))
    }
}

impl<'a> CamoMaterialView<'a> {
    pub fn replace_flags(&self) -> u16 {
        u16::from_le_bytes(self.bytes[0..2].try_into().unwrap())
    }
    pub fn material_count(&self) -> u16 {
        u16::from_le_bytes(self.bytes[2..4].try_into().unwrap())
    }
    pub fn shader_constants(&self) -> [f32; 8] {
        core::array::from_fn(|i| {
            f32::from_le_bytes(self.bytes[12 + i * 4..16 + i * 4].try_into().unwrap())
        })
    }
    pub fn material_pair(&self, index: u16) -> Option<(&'a LoadedAsset, &'a LoadedAsset)> {
        if index >= self.material_count() {
            return None;
        }
        let base = self.load.blocks.ptr_at(self.address.at(4)).ok()??;
        let camo = self.load.blocks.ptr_at(self.address.at(8)).ok()??;
        Some((
            self.load
                .asset_in(self.owner, base.at(u32::from(index) * 4))?,
            self.load
                .asset_in(self.owner, camo.at(u32::from(index) * 4))?,
        ))
    }
}
