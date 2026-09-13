#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum MarkFragmentsAgainst {
    WorldBrushes = 0,
    Models = 1,
}

impl MarkFragmentsAgainst {
    #[inline]
    pub const fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::WorldBrushes),
            1 => Some(Self::Models),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkReceiver {
    World,
    Ents,
    Smodels,
}

#[inline]
pub const fn fx_impact_mark_outer_gate(fx_marks: bool, no_marks: bool) -> bool {
    fx_marks && !no_marks
}

#[inline]
pub const fn fx_impact_mark_skip_world_from_stored_bolt(bolt: u8) -> bool {
    bolt != 0xff
}

#[inline]
pub const fn fx_impact_mark_models_generate(fx_marks_ents: bool, fx_marks_smodels: bool) -> bool {
    fx_marks_ents || fx_marks_smodels
}

#[inline]
pub const fn fx_impact_mark_calls_box_surfaces() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkWorldMesh {
    GfxSurface,

    ClipMapCollision,
}

#[inline]
pub const fn fx_impact_mark_add_entity(fx_marks_ents: bool) -> bool {
    fx_marks_ents
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkGoDispatch {
    WorldBrushesThenClip,

    Models,
}

#[inline]
pub const fn fx_mark_go_dispatch(against: MarkFragmentsAgainst) -> MarkGoDispatch {
    match against {
        MarkFragmentsAgainst::WorldBrushes => MarkGoDispatch::WorldBrushesThenClip,
        MarkFragmentsAgainst::Models => MarkGoDispatch::Models,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkGenerateAddEntity {
    None,
    Brush,
    Model,
    Local,
}

#[inline]
pub fn fx_impact_mark_material<'a>(
    materials: [Option<&'a str>; 2],
    against: MarkFragmentsAgainst,
) -> Option<&'a str> {
    match against {
        MarkFragmentsAgainst::WorldBrushes => materials[1],
        MarkFragmentsAgainst::Models => materials[0],
    }
}

#[inline]
pub const fn fx_impact_mark_generate_add_entity(
    fx_marks_ents: bool,
    against: MarkFragmentsAgainst,
    generate_local_dobj: bool,
) -> MarkGenerateAddEntity {
    if !fx_marks_ents {
        return MarkGenerateAddEntity::None;
    }
    match against {
        MarkFragmentsAgainst::WorldBrushes => MarkGenerateAddEntity::Brush,
        MarkFragmentsAgainst::Models => {
            if generate_local_dobj {
                MarkGenerateAddEntity::Local
            } else {
                MarkGenerateAddEntity::Model
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxGenerateMarkVertsPacked {
    pub fx_marks: bool,
    pub fx_marks_smodels: bool,
    pub fx_marks_ents: bool,
}

#[inline]
pub const fn fx_fill_generate_mark_verts_cmd(
    fx_marks: bool,
    fx_marks_smodels: bool,
    fx_marks_ents: bool,
) -> FxGenerateMarkVertsPacked {
    FxGenerateMarkVertsPacked {
        fx_marks,
        fx_marks_smodels,
        fx_marks_ents,
    }
}

#[inline]
pub const fn fx_dyn_mark_verts_worker_gate(cmd: FxGenerateMarkVertsPacked) -> bool {
    cmd.fx_marks && (cmd.fx_marks_smodels || cmd.fx_marks_ents)
}
