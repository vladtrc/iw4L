use crate::material_catalog::{T5TechniqueOccupancy, TechniqueTable};

pub const T5_TECHNIQUE_TYPE_COUNT: usize = 130;

pub const IW4_TECHNIQUE_TYPE_COUNT: usize = 48;

pub const T5_TECHNIQUE_TYPE_NAMES: [&str; T5_TECHNIQUE_TYPE_COUNT] = [
    "depth prepass",
    "build floatz",
    "build shadowmap depth",
    "build shadowmap color",
    "unlit",
    "emissive",
    "emissive shadow",
    "emissive nvintz",
    "emissive shadow nvintz",
    "emissive reflected",
    "lit",
    "lit sun",
    "lit sun shadow",
    "lit spot",
    "lit spot shadow",
    "lit omni",
    "lit omni shadow",
    "lit dlight",
    "lit sun dlight",
    "lit sun shadow dlight",
    "lit spot dlight",
    "lit spot shadow dlight",
    "lit omni dlight",
    "lit omni shadow dlight",
    "lit glight",
    "lit sun glight",
    "lit sun shadow glight",
    "lit spot glight",
    "lit spot shadow glight",
    "lit omni glight",
    "lit omni shadow glight",
    "lit dlight glight",
    "lit sun dlight glight",
    "lit sun shadow dlight glight",
    "lit spot dlight glight",
    "lit spot shadow dlight glight",
    "lit omni dlight glight",
    "lit omni shadow dlight glight",
    "lit alpha",
    "lit sun alpha",
    "lit sun shadow alpha",
    "lit spot alpha",
    "lit spot shadow alpha",
    "lit omni alpha",
    "lit omni shadow alpha",
    "lit remap",
    "lit sun remap",
    "lit sun shadow remap",
    "lit spot remap",
    "lit spot shadow remap",
    "lit omni remap",
    "lit omni shadow remap",
    "lit fade",
    "lit sun fade",
    "lit sun shadow fade",
    "lit spot fade",
    "lit spot shadow fade",
    "lit omni fade",
    "lit omni shadow fade",
    "lit charred",
    "lit fade charred",
    "lit sun charred",
    "lit sun fade charred",
    "lit sun shadow charred",
    "lit sun shadow fade charred",
    "lit spot charred",
    "lit spot fade charred",
    "lit spot shadow charred",
    "lit spot shadow fade charred",
    "lit omni charred",
    "lit omni fade charred",
    "lit omni shadow charred",
    "lit omni shadow fade charred",
    "lit instanced",
    "lit instanced sun",
    "lit instanced sun shadow",
    "lit instanced spot",
    "lit instanced spot shadow",
    "lit instanced omni",
    "lit instanced omni shadow",
    "lit nvintz",
    "lit sun nvintz",
    "lit sun shadow nvintz",
    "lit spot nvintz",
    "lit spot shadow nvintz",
    "lit omni nvintz",
    "lit omni shadow nvintz",
    "lit dlight nvintz",
    "lit sun dlight nvintz",
    "lit sun shadow dlight nvintz",
    "lit spot dlight nvintz",
    "lit spot shadow dlight nvintz",
    "lit omni dlight nvintz",
    "lit omni shadow dlight nvintz",
    "lit glight nvintz",
    "lit sun glight nvintz",
    "lit sun shadow glight nvintz",
    "lit spot glight nvintz",
    "lit spot shadow glight nvintz",
    "lit omni glight nvintz",
    "lit omni shadow glight nvintz",
    "lit dlight glight nvintz",
    "lit sun dlight glight nvintz",
    "lit sun shadow dlight glight nvintz",
    "lit spot dlight glight nvintz",
    "lit spot shadow dlight glight nvintz",
    "lit omni dlight glight nvintz",
    "lit omni shadow dlight glight nvintz",
    "lit instanced nvintz",
    "lit instanced sun nvintz",
    "lit instanced sun shadow nvintz",
    "lit instanced spot nvintz",
    "lit instanced spot shadow nvintz",
    "lit instanced omni nvintz",
    "lit instanced omni shadow nvintz",
    "light spot",
    "light omni",
    "light spot shadow",
    "light spot charred",
    "light omni charred",
    "light spot shadow charred",
    "fakelight normal",
    "fakelight view",
    "sunlight preview",
    "case texture",
    "solid wireframe",
    "shaded wireframe",
    "debug bumpmap",
    "debug bumpmap instanced",
    "impact mask",
];

pub const IW4_TECHNIQUE_TYPE_NAMES: [&str; IW4_TECHNIQUE_TYPE_COUNT] = [
    "depth prepass",
    "build floatz",
    "build shadowmap depth",
    "build shadowmap color",
    "unlit",
    "emissive",
    "emissive dfog",
    "emissive shadow",
    "emissive shadow dfog",
    "lit",
    "lit dfog",
    "lit sun",
    "lit sun dfog",
    "lit sun shadow",
    "lit sun shadow dfog",
    "lit spot",
    "lit spot dfog",
    "lit spot shadow",
    "lit spot shadow dfog",
    "lit omni",
    "lit omni dfog",
    "lit omni shadow",
    "lit omni shadow dfog",
    "lit instanced",
    "lit instanced dfog",
    "lit instanced sun",
    "lit instanced sun dfog",
    "lit instanced sun shadow",
    "lit instanced sun shadow dfog",
    "lit instanced spot",
    "lit instanced spot dfog",
    "lit instanced spot shadow",
    "lit instanced spot shadow dfog",
    "lit instanced omni",
    "lit instanced omni dfog",
    "lit instanced omni shadow",
    "lit instanced omni shadow dfog",
    "light spot",
    "light omni",
    "light spot shadow",
    "fakelight normal",
    "fakelight view",
    "sunlight preview",
    "case texture",
    "solid wireframe",
    "shaded wireframe",
    "debug bumpmap",
    "debug bumpmap instanced",
];

pub const T5_NAME_MATCH_IW4_SLOT: [Option<u8>; T5_TECHNIQUE_TYPE_COUNT] = [
    Some(0),
    Some(1),
    Some(2),
    Some(3),
    Some(4),
    Some(5),
    Some(7),
    None,
    None,
    None,
    Some(9),
    Some(11),
    Some(13),
    Some(15),
    Some(17),
    Some(19),
    Some(21),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some(23),
    Some(25),
    Some(27),
    Some(29),
    Some(31),
    Some(33),
    Some(35),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some(37),
    Some(38),
    Some(39),
    None,
    None,
    None,
    Some(40),
    Some(41),
    Some(42),
    Some(43),
    Some(44),
    Some(45),
    Some(46),
    Some(47),
    None,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T5SlotProjection {
    NameMatch {
        t5_slot: u8,
        iw4_slot: u8,
        name: &'static str,
    },

    Unavailable {
        t5_slot: u8,
        name: &'static str,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T5OccupancyRemapGap {
    NameMatchIsNotRuntimeBind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OccupancyProjectionCensus {
    pub occupied: usize,
    pub name_match: usize,
    pub unavailable: usize,
    pub occupied_t5_lit: bool,
    pub occupied_t5_slot9: bool,
}

pub fn project_t5_slot(t5_slot: usize) -> Option<T5SlotProjection> {
    let name = *T5_TECHNIQUE_TYPE_NAMES.get(t5_slot)?;
    let t5_slot = t5_slot as u8;
    match T5_NAME_MATCH_IW4_SLOT[t5_slot as usize] {
        Some(iw4_slot) => Some(T5SlotProjection::NameMatch {
            t5_slot,
            iw4_slot,
            name,
        }),
        None => Some(T5SlotProjection::Unavailable { t5_slot, name }),
    }
}

pub fn census_occupancy(occupancy: &T5TechniqueOccupancy) -> OccupancyProjectionCensus {
    let mut census = OccupancyProjectionCensus {
        occupied_t5_lit: occupancy.slot_occupied(10),
        occupied_t5_slot9: occupancy.slot_occupied(9),
        ..OccupancyProjectionCensus::default()
    };
    for slot in 0..T5_TECHNIQUE_TYPE_COUNT {
        if !occupancy.slot_occupied(slot) {
            continue;
        }
        census.occupied += 1;
        match project_t5_slot(slot) {
            Some(T5SlotProjection::NameMatch { .. }) => census.name_match += 1,
            Some(T5SlotProjection::Unavailable { .. }) => census.unavailable += 1,
            None => {}
        }
    }
    census
}

pub fn occupancy_to_technique_table(
    _occupancy: &T5TechniqueOccupancy,
) -> Result<TechniqueTable, T5OccupancyRemapGap> {
    Err(T5OccupancyRemapGap::NameMatchIsNotRuntimeBind)
}

pub const STATE_BITS_ENTRY_UNUSED: u8 = 0xFF;

pub fn remap_t5_state_bits_entry(
    t5: &[u8; T5_TECHNIQUE_TYPE_COUNT],
) -> [u8; IW4_TECHNIQUE_TYPE_COUNT] {
    let mut out = [STATE_BITS_ENTRY_UNUSED; IW4_TECHNIQUE_TYPE_COUNT];
    for (iw4_slot, value) in out.iter_mut().enumerate() {
        if let Some(t5_slot) = iw4_slot_to_t5(iw4_slot) {
            *value = t5[t5_slot];
        }
    }
    out
}

pub fn leftover_t5_slots() -> impl Iterator<Item = (usize, &'static str)> {
    T5_NAME_MATCH_IW4_SLOT
        .iter()
        .enumerate()
        .filter_map(|(slot, mapped)| {
            mapped
                .is_none()
                .then_some((slot, T5_TECHNIQUE_TYPE_NAMES[slot]))
        })
}

pub fn leftover_t5_selector_census<'a>(
    entries: impl Iterator<Item = &'a [u8; T5_TECHNIQUE_TYPE_COUNT]>,
) -> Option<String> {
    let mut mats = 0u32;
    let mut leftover_sel = 0u32;
    let mut slot_hits = [0u32; T5_TECHNIQUE_TYPE_COUNT];
    for raw in entries {
        mats = mats.saturating_add(1);
        for (slot, _) in leftover_t5_slots() {
            if raw[slot] != STATE_BITS_ENTRY_UNUSED {
                leftover_sel = leftover_sel.saturating_add(1);
                slot_hits[slot] = slot_hits[slot].saturating_add(1);
            }
        }
    }
    if mats == 0 {
        return None;
    }
    let mut slots = String::new();
    for (slot, name) in leftover_t5_slots() {
        if slot_hits[slot] == 0 {
            continue;
        }
        if !slots.is_empty() {
            slots.push(',');
        }
        slots.push_str(&format!("{slot}:{name}={}", slot_hits[slot]));
    }
    if slots.is_empty() {
        slots.push_str("none");
    }
    Some(format!(
        "mats={mats} leftover_sel={leftover_sel} occupied={slots}"
    ))
}

#[must_use]
pub fn remap_occupancy_bits(t5: &[u64; fastfile_t5::TECHNIQUE_OCCUPANCY_WORDS]) -> u64 {
    let mut out = 0u64;
    for iw4_slot in 0..IW4_TECHNIQUE_TYPE_COUNT {
        if iw4_slot_to_t5(iw4_slot).is_some_and(|t5_slot| fastfile_t5::occupancy_test(t5, t5_slot))
        {
            out |= 1u64 << iw4_slot;
        }
    }
    out
}

#[must_use]
pub fn remap_pass_count_by_slot(
    t5: &[u8; T5_TECHNIQUE_TYPE_COUNT],
) -> [u8; IW4_TECHNIQUE_TYPE_COUNT] {
    let mut out = [0u8; IW4_TECHNIQUE_TYPE_COUNT];
    for (iw4_slot, value) in out.iter_mut().enumerate() {
        if let Some(t5_slot) = iw4_slot_to_t5(iw4_slot) {
            *value = t5[t5_slot];
        }
    }
    out
}

pub const T5_TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE: u16 = 0x8;

pub const IW4_TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE: u16 =
    asset_iw4::vertex_decl::TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE;

#[must_use]
pub fn remap_t5_technique_flags(t5_flags: u16) -> u16 {
    let mut flags = t5_flags;
    if flags & T5_TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE != 0 {
        flags &= !T5_TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE;
        flags |= IW4_TECHNIQUE_FLAG_VERTEX_TYPE_FROM_SURFACE;
    }
    flags
}

#[must_use]
pub fn remap_technique_flags_by_slot(
    t5: &[u16; T5_TECHNIQUE_TYPE_COUNT],
) -> [u16; IW4_TECHNIQUE_TYPE_COUNT] {
    let mut out = [0u16; IW4_TECHNIQUE_TYPE_COUNT];
    for (iw4_slot, value) in out.iter_mut().enumerate() {
        if let Some(t5_slot) = iw4_slot_to_t5(iw4_slot) {
            *value = remap_t5_technique_flags(t5[t5_slot]);
        }
    }
    out
}

#[must_use]
pub fn t5_slot_to_iw4(t5_slot: usize) -> Option<usize> {
    T5_NAME_MATCH_IW4_SLOT
        .get(t5_slot)
        .copied()
        .flatten()
        .map(usize::from)
}

#[must_use]
pub fn iw4_slot_to_t5(iw4_slot: usize) -> Option<usize> {
    if iw4_slot >= IW4_TECHNIQUE_TYPE_COUNT {
        return None;
    }
    let iw4_slot = match iw4_slot {
        6 | 8 => iw4_slot - 1,
        10..=36 if iw4_slot % 2 == 0 => iw4_slot - 1,
        _ => iw4_slot,
    };
    T5_NAME_MATCH_IW4_SLOT
        .iter()
        .enumerate()
        .find_map(|(t5, mapped)| (*mapped == Some(iw4_slot as u8)).then_some(t5))
}
