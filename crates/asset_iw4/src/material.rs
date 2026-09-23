use crate::size::TECHNIQUE_SLOT_COUNT;

pub const TECHNIQUE_MODEL_LIGHTING: usize = 9;

pub const CODE_CONST_MODEL_LIGHTING: u16 = 0x3a;

pub const TECHNIQUE_COLOR_BAND_FIRST: usize = 4;

pub const TECHNIQUE_UNLIT: usize = 4;

pub const SORT_KEY_SKY: u8 = 2;

pub const SORT_KEY_SKYBOX: u8 = 3;

pub const TECHNIQUE_SHADOW_MASK: u64 = (1 << 2) | (1 << 3);

pub const TECHNIQUE_LIT_MASK: u64 = (1 << 37) | (1 << 38) | (1 << 39);

pub const TECHNIQUE_UNIVERSAL_MASK: u64 = (1 << 4) | (1 << 44);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialPass {
    ShadowOnly,

    Sky(u8),

    Lit,

    Unlit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialDrawRoute {
    pub primary_sort_key: u8,

    pub prepass: u8,

    pub custom_index: u8,

    pub pass: MaterialPass,

    pub technique_slots: u64,

    pub uses_model_lighting_const: bool,
}

impl MaterialDrawRoute {
    #[inline]
    pub const fn has_technique(&self, index: usize) -> bool {
        index < TECHNIQUE_SLOT_COUNT && (self.technique_slots >> index) & 1 != 0
    }

    pub const fn route(
        primary_sort_key: u8,
        prepass: u8,
        custom_index: u8,
        technique_slots: u64,
        uses_model_lighting_const: bool,
    ) -> Self {
        let bucket = primary_sort_key & 0x3f;
        let pass = if technique_slots & !(TECHNIQUE_SHADOW_MASK | TECHNIQUE_UNIVERSAL_MASK) == 0 {
            MaterialPass::ShadowOnly
        } else if bucket == SORT_KEY_SKY || bucket == SORT_KEY_SKYBOX {
            MaterialPass::Sky(bucket)
        } else if technique_slots & TECHNIQUE_LIT_MASK != 0 {
            MaterialPass::Lit
        } else {
            MaterialPass::Unlit
        };
        Self {
            primary_sort_key: bucket,
            prepass,
            custom_index,
            pass,
            technique_slots,
            uses_model_lighting_const,
        }
    }

    #[inline]
    pub const fn takes_model_lighting(&self) -> bool {
        self.has_technique(TECHNIQUE_MODEL_LIGHTING) && self.uses_model_lighting_const
    }

    #[inline]
    pub const fn lit_band_is_partial(&self) -> bool {
        let band = self.technique_slots & TECHNIQUE_LIT_MASK;
        band != 0 && band != TECHNIQUE_LIT_MASK
    }
}

pub const MATERIAL_SURFACE_TYPE_BITS: usize = 0x10;

pub const MATERIAL_STATE_BITS_ENTRY: usize = 0x18;

pub const MATERIAL_STATE_BITS_COUNT: usize = 0x4a;

pub const MATERIAL_STATE_FLAGS: usize = 0x4b;

pub const MATERIAL_CAMERA_REGION: usize = 0x4c;

pub const CAMERA_REGION_LIT_OPAQUE: u8 = 0;

pub const CAMERA_REGION_LIT_TRANS: u8 = 1;

pub const CAMERA_REGION_EMISSIVE: u8 = 2;

pub const CAMERA_REGION_DEPTH_HACK: u8 = 3;

pub const CAMERA_REGION_NONE: u8 = 4;

pub const MATERIAL_STATE_BITS_TABLE: usize = 0x5c;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorPassAgreement<T> {
    Agreed(T),

    Disagrees { distinct: u8 },

    EntryTableNotWalked,

    EntryOutOfRange { entry: u8, len: u8 },

    NoColorTechnique,

    Absent,
}

impl<T> ColorPassAgreement<T> {
    #[inline]
    pub fn agreed(self) -> Option<T> {
        match self {
            Self::Agreed(value) => Some(value),
            _ => None,
        }
    }
}

pub fn lit_band_decode_conflicts<T: PartialEq>(
    state_bits_entry: Option<&[u8; TECHNIQUE_SLOT_COUNT]>,
    table: &[[u32; 2]],
    technique_slots: u64,
    decode: impl Fn([u32; 2]) -> T,
) -> bool {
    let mut seen: Option<T> = None;
    for tech in [37usize, 38, 39] {
        let Some(row) =
            color_pass_row_for_tech_type(state_bits_entry, table, technique_slots, tech)
        else {
            continue;
        };
        let decoded = decode(row);
        if let Some(ref prev) = seen {
            if *prev != decoded {
                return true;
            }
        } else {
            seen = Some(decoded);
        }
    }
    false
}

pub fn color_pass_row_for_tech_type(
    state_bits_entry: Option<&[u8; TECHNIQUE_SLOT_COUNT]>,
    table: &[[u32; 2]],
    technique_slots: u64,
    tech_type: usize,
) -> Option<[u32; 2]> {
    color_pass_row_for_tech_type_pass(state_bits_entry, table, technique_slots, tech_type, 0)
}

pub fn color_pass_row_for_tech_type_pass(
    state_bits_entry: Option<&[u8; TECHNIQUE_SLOT_COUNT]>,
    table: &[[u32; 2]],
    technique_slots: u64,
    tech_type: usize,
    pass_index: usize,
) -> Option<[u32; 2]> {
    if tech_type >= TECHNIQUE_SLOT_COUNT || (technique_slots >> tech_type) & 1 == 0 {
        return None;
    }
    let entries = state_bits_entry?;
    let entry = entries[tech_type] as usize + pass_index;
    table.get(entry).copied()
}

pub fn color_pass_agreement<T: PartialEq>(
    state_bits_entry: Option<&[u8; TECHNIQUE_SLOT_COUNT]>,
    table: &[[u32; 2]],
    technique_slots: Option<u64>,
    decode: impl Fn([u32; 2]) -> T,
) -> ColorPassAgreement<T> {
    if table.is_empty() {
        return ColorPassAgreement::Absent;
    }
    let slots = technique_slots.unwrap_or(u64::MAX);
    if slots >> TECHNIQUE_COLOR_BAND_FIRST == 0 {
        return ColorPassAgreement::NoColorTechnique;
    }
    let out_of_range = |entry: u8| ColorPassAgreement::EntryOutOfRange {
        entry,
        len: table.len().min(u8::MAX as usize) as u8,
    };
    let Some(entries) = state_bits_entry else {
        return match table {
            [only] if table.len() == 1 => ColorPassAgreement::Agreed(decode(*only)),
            _ => ColorPassAgreement::EntryTableNotWalked,
        };
    };
    let mut answer = None;
    let mut distinct = 0u8;
    for slot in TECHNIQUE_COLOR_BAND_FIRST..TECHNIQUE_SLOT_COUNT {
        if (slots >> slot) & 1 == 0 {
            continue;
        }
        let entry = entries[slot];
        let Some(row) = table.get(entry as usize) else {
            return out_of_range(entry);
        };
        let decoded = decode(*row);

        if !colour_band_saw(entries, table, slots, slot, &decoded, &decode) {
            distinct = distinct.saturating_add(1);
        }
        if answer.is_none() {
            answer = Some(decoded);
        }
    }
    match answer {
        Some(value) if distinct <= 1 => ColorPassAgreement::Agreed(value),
        Some(_) => ColorPassAgreement::Disagrees { distinct },
        None => ColorPassAgreement::NoColorTechnique,
    }
}

fn colour_band_saw<T: PartialEq>(
    entries: &[u8; TECHNIQUE_SLOT_COUNT],
    table: &[[u32; 2]],
    slots: u64,
    before: usize,
    decoded: &T,
    decode: &impl Fn([u32; 2]) -> T,
) -> bool {
    for slot in TECHNIQUE_COLOR_BAND_FIRST..before {
        if (slots >> slot) & 1 == 0 {
            continue;
        }
        if let Some(row) = table.get(entries[slot] as usize)
            && decode(*row) == *decoded
        {
            return true;
        }
    }
    false
}
