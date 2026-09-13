use std::collections::HashMap;

use asset_iw4::size as sz;
use fastfile_iw4::{ClipMapGeometry, Ptr, ZoneStream};

pub const DYNENT_DRAW_MODEL: usize = 0;

pub const DYNENT_DRAW_BRUSH: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynEntType {
    Invalid,
    Clutter,
    Destruct,

    Unknown(u8),
}

impl DynEntType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Invalid,
            1 => Self::Clutter,
            2 => Self::Destruct,
            other => Self::Unknown(other),
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Invalid => 0,
            Self::Clutter => 1,
            Self::Destruct => 2,
            Self::Unknown(v) => v,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynEntDrawType {
    Model,
    Brush,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynEntProps {
    pub client_only: bool,
    pub clip_move: bool,
    pub use_physics: bool,
    pub destroyable: bool,
}

pub const RETAIL_DYN_ENT_PROPS: [DynEntProps; 3] = [
    DynEntProps {
        client_only: false,
        clip_move: false,
        use_physics: false,
        destroyable: false,
    },
    DynEntProps {
        client_only: true,
        clip_move: false,
        use_physics: true,
        destroyable: false,
    },
    DynEntProps {
        client_only: true,
        clip_move: false,
        use_physics: true,
        destroyable: true,
    },
];

pub fn retail_dyn_ent_props(ty: DynEntType) -> Option<DynEntProps> {
    RETAIL_DYN_ENT_PROPS.get(ty.as_u8() as usize).copied()
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedPhysPreset {
    pub name: String,

    pub preset_type: i32,
    pub mass: f32,
    pub bounce: f32,
    pub friction: f32,
    pub bullet_force_scale: f32,
    pub explosive_force_scale: f32,
    pub snd_alias_prefix: String,
    pub pieces_spread_fraction: f32,
    pub pieces_upward_velocity: f32,
    pub temp_default_to_cylinder: bool,
    pub per_surface_snd_alias: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PhysPresetCatalog {
    direct: HashMap<Ptr, OwnedPhysPreset>,
    aliases: HashMap<Ptr, Ptr>,
}

impl PhysPresetCatalog {
    pub fn capture(&mut self, stream: &ZoneStream<'_>, slot: Ptr, insert_slot: Option<Ptr>) {
        let Some(geometry) = stream.phys_preset() else {
            return;
        };
        let Some(owned) = read_phys_preset(stream, geometry) else {
            return;
        };
        self.direct.insert(slot, owned.clone());
        if let Some(insert_slot) = insert_slot {
            self.direct.insert(insert_slot, owned);
        }
    }

    pub fn alias(&mut self, slot: Ptr, target: Ptr) {
        self.aliases.insert(slot, target);
    }

    pub fn at_slot(&self, slot: Ptr) -> Option<&OwnedPhysPreset> {
        let mut at = slot;
        for _ in 0..self.aliases.len().saturating_add(1) {
            if let Some(preset) = self.direct.get(&at) {
                return Some(preset);
            }
            at = *self.aliases.get(&at)?;
        }
        None
    }

    pub fn len(&self) -> usize {
        self.direct.len()
    }

    pub fn is_empty(&self) -> bool {
        self.direct.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynEntDef {
    pub draw_type: DynEntDrawType,

    pub index: u16,
    pub ty: DynEntType,

    pub quat: [f32; 4],

    pub origin: [f32; 3],
    pub xmodel: Option<String>,

    pub brush_model: u16,
    pub physics_brush_model: u16,
    pub destroy_fx: Option<String>,
    pub phys_preset: Option<OwnedPhysPreset>,
    pub health: i32,

    pub phys_mass: [f32; 9],
    pub contents: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DynEntCatalog {
    pub models: Vec<DynEntDef>,
    pub brushes: Vec<DynEntDef>,
    pub truncated: usize,
    pub missing_xmodel: usize,
    pub missing_preset: usize,
}

impl DynEntCatalog {
    pub fn model_n(&self) -> usize {
        self.models.len()
    }

    pub fn brush_n(&self) -> usize {
        self.brushes.len()
    }

    pub fn count_type(&self, ty: DynEntType) -> usize {
        self.models
            .iter()
            .chain(self.brushes.iter())
            .filter(|def| def.ty == ty)
            .count()
    }

    pub fn phys_preset_named_n(&self) -> usize {
        self.models
            .iter()
            .chain(self.brushes.iter())
            .filter(|def| def.phys_preset.is_some())
            .count()
    }

    pub fn report_line(&self) -> String {
        format!(
            "dynents: models={} brushes={} clutter={} destruct={} truncated={} missing_xmodel={} missing_preset={} (ClipMap; not script_model; clipMove=false)",
            self.model_n(),
            self.brush_n(),
            self.count_type(DynEntType::Clutter),
            self.count_type(DynEntType::Destruct),
            self.truncated,
            self.missing_xmodel,
            self.missing_preset
        )
    }
}

pub fn parse_dyn_ent_def_scalars(
    bytes: &[u8],
    format: fastfile_iw4::Iw4WireFormat,
) -> Option<DynEntDefScalars> {
    let off = |x86, x64| match format {
        fastfile_iw4::Iw4WireFormat::X86 => x86,
        fastfile_iw4::Iw4WireFormat::X64 => x64,
    };
    if bytes.len() < off(sz::DYN_ENTITY_DEF, 112) {
        return None;
    }
    Some(DynEntDefScalars {
        ty: bytes[0],
        quat: [
            f32_at(bytes, 4),
            f32_at(bytes, 8),
            f32_at(bytes, 12),
            f32_at(bytes, 16),
        ],
        origin: [f32_at(bytes, 20), f32_at(bytes, 24), f32_at(bytes, 28)],
        brush_model: u16_at(bytes, off(36, 40)),
        physics_brush_model: u16_at(bytes, off(38, 42)),
        health: i32_at(bytes, off(48, 64)),
        phys_mass: core::array::from_fn(|i| f32_at(bytes, off(52, 68) + i * 4)),
        contents: i32_at(bytes, off(88, 104)),
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynEntDefScalars {
    pub ty: u8,
    pub quat: [f32; 4],
    pub origin: [f32; 3],
    pub brush_model: u16,
    pub physics_brush_model: u16,
    pub health: i32,
    pub phys_mass: [f32; 9],
    pub contents: i32,
}

pub fn build_dyn_ent_catalog(
    stream: &ZoneStream<'_>,
    geometry: ClipMapGeometry,
    mut xmodel_name: impl FnMut(Ptr) -> Option<String>,
    mut fx_name: impl FnMut(Ptr) -> Option<String>,
    mut preset_at: impl FnMut(Ptr) -> Option<OwnedPhysPreset>,
) -> DynEntCatalog {
    let mut catalog = DynEntCatalog::default();
    let (models, t0, x0, p0) = copy_list(
        stream,
        geometry.dyn_ent_count[DYNENT_DRAW_MODEL],
        geometry.dyn_ent_defs[DYNENT_DRAW_MODEL],
        DynEntDrawType::Model,
        &mut xmodel_name,
        &mut fx_name,
        &mut preset_at,
    );
    let (brushes, t1, x1, p1) = copy_list(
        stream,
        geometry.dyn_ent_count[DYNENT_DRAW_BRUSH],
        geometry.dyn_ent_defs[DYNENT_DRAW_BRUSH],
        DynEntDrawType::Brush,
        &mut xmodel_name,
        &mut fx_name,
        &mut preset_at,
    );
    catalog.models = models;
    catalog.brushes = brushes;
    catalog.truncated = t0 + t1;
    catalog.missing_xmodel = x0 + x1;
    catalog.missing_preset = p0 + p1;
    catalog
}

fn copy_list(
    stream: &ZoneStream<'_>,
    count: usize,
    defs: Option<Ptr>,
    draw_type: DynEntDrawType,
    xmodel_name: &mut impl FnMut(Ptr) -> Option<String>,
    fx_name: &mut impl FnMut(Ptr) -> Option<String>,
    preset_at: &mut impl FnMut(Ptr) -> Option<OwnedPhysPreset>,
) -> (Vec<DynEntDef>, usize, usize, usize) {
    let mut out = Vec::new();
    let mut truncated = 0usize;
    let mut missing_xmodel = 0usize;
    let mut missing_preset = 0usize;
    let Some(base) = defs else {
        return (out, count, 0, 0);
    };
    out.reserve(count);
    for index in 0..count {
        let def = base.at(index * stream.layout(sz::DYN_ENTITY_DEF, 112));
        let Ok(bytes) = stream.slice_at(def, 0, stream.layout(sz::DYN_ENTITY_DEF, 112)) else {
            truncated += 1;
            continue;
        };
        let Some(scalars) = parse_dyn_ent_def_scalars(bytes, stream.wire_format()) else {
            truncated += 1;
            continue;
        };
        let xmodel = xmodel_name(def.at(32));
        if xmodel.is_none() && draw_type == DynEntDrawType::Model {
            missing_xmodel += 1;
        }
        let phys_preset = preset_at(def.at(stream.layout(44, 56)));
        if phys_preset.is_none() {
            missing_preset += 1;
        }
        out.push(DynEntDef {
            draw_type,
            index: index as u16,
            ty: DynEntType::from_u8(scalars.ty),
            quat: scalars.quat,
            origin: scalars.origin,
            xmodel,
            brush_model: scalars.brush_model,
            physics_brush_model: scalars.physics_brush_model,
            destroy_fx: fx_name(def.at(stream.layout(40, 48))),
            phys_preset,
            health: scalars.health,
            phys_mass: scalars.phys_mass,
            contents: scalars.contents,
        });
    }
    (out, truncated, missing_xmodel, missing_preset)
}

fn read_phys_preset(
    stream: &ZoneStream<'_>,
    geometry: fastfile_iw4::PhysPresetGeometry,
) -> Option<OwnedPhysPreset> {
    let header = geometry.header;
    Some(OwnedPhysPreset {
        name: string_at(stream, geometry.name).unwrap_or_default(),
        preset_type: stream.i32_at(header, stream.layout(4, 8)).ok()?,
        mass: stream.f32_at(header, stream.layout(8, 12)).ok()?,
        bounce: stream.f32_at(header, stream.layout(12, 16)).ok()?,
        friction: stream.f32_at(header, stream.layout(16, 20)).ok()?,
        bullet_force_scale: stream.f32_at(header, stream.layout(20, 24)).ok()?,
        explosive_force_scale: stream.f32_at(header, stream.layout(24, 28)).ok()?,
        snd_alias_prefix: string_at(stream, geometry.snd_alias_prefix).unwrap_or_default(),
        pieces_spread_fraction: stream.f32_at(header, stream.layout(32, 40)).ok()?,
        pieces_upward_velocity: stream.f32_at(header, stream.layout(36, 44)).ok()?,
        temp_default_to_cylinder: stream.u8_at(header, stream.layout(40, 48)).ok()? != 0,
        per_surface_snd_alias: stream.u8_at(header, stream.layout(41, 49)).ok()? != 0,
    })
}

fn string_at(stream: &ZoneStream<'_>, p: Option<Ptr>) -> Option<String> {
    let p = p?;
    stream.cstr(p).ok().map(str::to_owned)
}

fn f32_at(bytes: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn u16_at(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes(bytes[off..off + 2].try_into().unwrap())
}

fn i32_at(bytes: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}
