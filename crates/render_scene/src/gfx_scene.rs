pub const SCENE_GFX_ENT_CAP: u32 = 0x80;

pub const SCENE_MODEL_CAP: u32 = 0x400;

pub const SCENE_DOBJ_CAP: u32 = 0x200;

pub const SCENE_DOBJ_FLAG30_CAP: u32 = 8;

pub const SCENE_BRUSH_CAP: u32 = 0x200;

pub const SCENE_MODEL_STRIDE: u32 = 0x48;

pub const SCENE_DOBJ_STRIDE: u32 = 0x78;

pub const SCENE_BRUSH_STRIDE: u32 = 0x28;

pub const SCENE_GFX_ENT_STRIDE: u32 = 8;

pub const SCENE_MODEL_ARM_NUM_MODELS: u8 = 1;

pub const SCENE_MODEL_ARM_FLAG_MASK: u32 = 0x34;

pub const SCENE_DOBJ_FLAG30_MASK: u32 = 0x30;

pub const SCENE_GFX_ENT_FLAG_BIT: u32 = 2;

pub const SCENE_MODEL_OFF_XMODEL: u32 = 0x00;

pub const SCENE_MODEL_OFF_DOBJ: u32 = 0x04;

pub const SCENE_MODEL_OFF_QUAT: u32 = 0x08;

pub const SCENE_MODEL_OFF_ORIGIN: u32 = 0x18;

pub const SCENE_MODEL_OFF_SCALE: u32 = 0x24;

pub const SCENE_MODEL_OFF_INFO: u32 = 0x28;

pub const SCENE_MODEL_OFF_RADIUS: u32 = 0x2c;

pub const SCENE_MODEL_OFF_UNREAD: u32 = 0x30;

pub const SCENE_MODEL_OFF_LIGHTING: u32 = 0x34;

pub const SCENE_MODEL_OFF_LOD_HINT: u32 = 0x45;

pub const SCENE_DOBJ_OFF_LIGHTING: u32 = 0x00;

pub const SCENE_DOBJ_OFF_QUAT: u32 = 0x0c;

pub const SCENE_DOBJ_OFF_ORIGIN: u32 = 0x1c;

pub const SCENE_DOBJ_OFF_TYPE3_GATE: u32 = 0x28;

pub const SCENE_DOBJ_OFF_ORIGIN_DUP: u32 = 0x2c;

pub const SCENE_DOBJ_OFF_RADIUS: u32 = 0x38;

pub const SCENE_DOBJ_OFF_INFO: u32 = 0x68;

pub const SCENE_DOBJ_OFF_DOBJ: u32 = 0x6c;

pub const SCENE_DOBJ_OFF_POSE: u32 = 0x70;

pub const SCENE_GFX_ENT_OFF_FLAGS2: u32 = 0;
pub const SCENE_GFX_ENT_OFF_MATERIAL_TIME: u32 = 4;

pub const SCENE_BRUSH_OFF_SURF_FROM_BMODEL: i32 = -2;
pub const SCENE_BRUSH_OFF_QUAT_FROM_BMODEL: u32 = 4;

pub const SCENE_BRUSH_OFF_LIGHTING_FROM_BMODEL: u32 = 0x14;

pub const SCENE_INDEX_EMPTY: u16 = 0xffff;

pub const SCENE_VIEWMODEL_ENTNUM: u32 = 0x7ff;

pub const SCENE_VIEWMODEL_LEFT_ENTNUM: u32 = 0x7fe;

pub const SCENE_VIEWMODEL_FX_FLAGS: u32 = 3;

#[must_use]
pub fn scene_info_entnum(info: u32) -> u32 {
    (info >> 7) & 0xfff
}

pub fn pack_scene_info(entnum: u32, render_fx_flags: u32, gfx_ent: u32) -> u32 {
    ((entnum & 0xfff | render_fx_flags << 0xc) << 7) | (gfx_ent & 0x7f)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAddKind {
    Model(u32),
    Dobj(u32),
    DobjFlag30(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAddError {
    GfxEntOverflow,
    ModelOverflow,
    DobjOverflow,
    DobjFlag30Overflow,
    BrushOverflow,
    BrushNoSurfs,
}

#[derive(Clone, Copy, Debug)]
pub struct AddDObjArgs {
    pub render_fx_flags: u32,
    pub material_time: f32,

    pub has_tree: bool,

    pub num_models: u8,

    pub pose: Option<AddDObjPose>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AddDObjPose {
    pub origin: [f32; 3],
    pub lighting_origin: [f32; 3],

    pub radius: Option<f32>,

    pub entnum: u32,

    pub quat: Option<[f32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxSceneModel {
    pub origin: [f32; 3],
    pub lighting_origin: [f32; 3],
    pub radius: Option<f32>,
    pub info: u32,

    pub scale: f32,
    pub quat: Option<[f32; 4]>,

    pub posed_bounds: Option<dpvs_iw4::Bounds>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GfxSceneDobj {
    pub origin: [f32; 3],
    pub lighting_origin: [f32; 3],
    pub radius: Option<f32>,
    pub info: u32,
    pub quat: Option<[f32; 4]>,

    pub cull_gate: u32,

    pub posed_bounds: Option<dpvs_iw4::Bounds>,

    pub lods: Vec<i8>,

    pub skinned_surfs: Option<SceneEntSkinnedSurfs>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneEntSkinnedSurfs {
    pub entries: Vec<dpvs_iw4::SceneEntSkinEntry>,
    pub summary: dpvs_iw4::PreSkinSummary,
}

#[derive(Clone, Copy, Debug)]
pub struct AddBModelArgs {
    pub surf_id: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AddBModelPose {
    pub model_index: u32,

    pub origin: [f32; 3],

    pub quat: Option<[f32; 4]>,

    pub param_4: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxSceneBrush {
    pub model_index: u32,
    pub origin: [f32; 3],
    pub quat: Option<[f32; 4]>,
    pub param_4: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GfxScene {
    pub gfx_ent_count: u32,
    pub scene_model_count: u32,
    pub scene_dobj_count: u32,
    pub scene_dobj_flag30_count: u32,
    pub scene_brush_count: u32,
    pub scene_models: Vec<GfxSceneModel>,
    pub scene_dobjs: Vec<GfxSceneDobj>,
    pub scene_dobj_flag30: Vec<GfxSceneDobj>,
    pub scene_brushes: Vec<GfxSceneBrush>,

    pub scene_model_index: Vec<u16>,

    pub scene_dobj_index: Vec<u16>,

    pub scene_ent_visible: Vec<u8>,

    pub scene_ent_walked: bool,
}

impl GfxScene {
    pub fn clear(&mut self) {
        let n = dpvs_iw4::GFX_CFG_ENT_COUNT as usize;
        *self = Self {
            scene_model_index: vec![SCENE_INDEX_EMPTY; n],
            scene_dobj_index: vec![SCENE_INDEX_EMPTY; n],
            scene_ent_visible: vec![0; n],
            ..Self::default()
        };
    }

    pub fn mark_scene_ent_visible(&mut self, entnum: u32) {
        dpvs_iw4::mark_scene_ent_visible(&mut self.scene_ent_visible, entnum);
    }

    #[must_use]
    pub fn scene_ent_visible(&self, entnum: u32) -> bool {
        dpvs_iw4::scene_ent_is_visible(&self.scene_ent_visible, entnum)
    }

    #[must_use]
    pub fn scene_ent_hidden(&self, entnum: u32) -> bool {
        self.scene_ent_walked && self.scene_ent_live(entnum) && !self.scene_ent_visible(entnum)
    }

    #[must_use]
    pub fn scene_ent_skips_draw(&self, entnum: u32) -> bool {
        self.scene_ent_hidden(entnum) || self.scene_ent_surface_count(entnum) == Some(0)
    }

    #[must_use]
    pub fn scene_dobj_live(&self, entnum: u32) -> bool {
        self.scene_dobj_index
            .get(entnum as usize)
            .is_some_and(|&slot| slot != SCENE_INDEX_EMPTY)
    }

    #[must_use]
    pub fn scene_model_live(&self, entnum: u32) -> bool {
        self.scene_model_index
            .get(entnum as usize)
            .is_some_and(|&slot| slot != SCENE_INDEX_EMPTY)
    }

    #[must_use]
    pub fn scene_ent_live(&self, entnum: u32) -> bool {
        self.scene_dobj_live(entnum) || self.scene_model_live(entnum)
    }

    pub fn alloc_scene_model(&mut self) -> u32 {
        let index = self.scene_model_count;
        self.scene_model_count = self.scene_model_count.saturating_add(1);
        if index > SCENE_MODEL_CAP - 1 {
            self.scene_model_count = SCENE_MODEL_CAP;
        }
        index
    }

    pub fn alloc_scene_brush(&mut self, args: AddBModelArgs) -> Result<u32, SceneAddError> {
        if args.surf_id == 0 {
            return Err(SceneAddError::BrushNoSurfs);
        }
        let index = self.scene_brush_count;
        self.scene_brush_count = self.scene_brush_count.saturating_add(1);
        if index > SCENE_BRUSH_CAP - 1 {
            self.scene_brush_count = SCENE_BRUSH_CAP;
            return Err(SceneAddError::BrushOverflow);
        }
        Ok(index)
    }

    pub fn add_bmodel(
        &mut self,
        args: AddBModelArgs,
        pose: AddBModelPose,
    ) -> Result<u32, SceneAddError> {
        let index = self.alloc_scene_brush(args)?;
        self.scene_brushes.push(GfxSceneBrush {
            model_index: pose.model_index,
            origin: pose.origin,
            quat: pose.quat,
            param_4: pose.param_4,
        });
        Ok(index)
    }

    pub fn add_dobj(&mut self, args: AddDObjArgs) -> Result<SceneAddKind, SceneAddError> {
        let gfx_ent =
            if args.material_time != 0.0 || (args.render_fx_flags & SCENE_GFX_ENT_FLAG_BIT) != 0 {
                let index = self.gfx_ent_count;
                self.gfx_ent_count = self.gfx_ent_count.saturating_add(1);
                if index > SCENE_GFX_ENT_CAP - 1 {
                    self.gfx_ent_count = SCENE_GFX_ENT_CAP;
                    return Err(SceneAddError::GfxEntOverflow);
                }
                index
            } else {
                0
            };
        let model_arm = (args.render_fx_flags & SCENE_MODEL_ARM_FLAG_MASK) == 0
            && !args.has_tree
            && args.num_models == SCENE_MODEL_ARM_NUM_MODELS;
        if model_arm {
            let index = self.alloc_scene_model();
            if index < SCENE_MODEL_CAP {
                let kind = SceneAddKind::Model(index);
                self.fill_pose(kind, args.render_fx_flags, gfx_ent, args.pose);
                return Ok(kind);
            }
            return Err(SceneAddError::ModelOverflow);
        }
        if (args.render_fx_flags & SCENE_DOBJ_FLAG30_MASK) == 0 {
            if self.scene_dobj_count >= SCENE_DOBJ_CAP {
                return Err(SceneAddError::DobjOverflow);
            }
            let index = self.scene_dobj_count;
            self.scene_dobj_count = self.scene_dobj_count.saturating_add(1);
            let kind = SceneAddKind::Dobj(index);
            self.fill_pose(kind, args.render_fx_flags, gfx_ent, args.pose);
            Ok(kind)
        } else {
            if self.scene_dobj_flag30_count >= SCENE_DOBJ_FLAG30_CAP {
                return Err(SceneAddError::DobjFlag30Overflow);
            }
            let index = self.scene_dobj_flag30_count;
            self.scene_dobj_flag30_count = self.scene_dobj_flag30_count.saturating_add(1);
            let kind = SceneAddKind::DobjFlag30(index + SCENE_DOBJ_CAP);
            self.fill_pose(kind, args.render_fx_flags, gfx_ent, args.pose);
            Ok(kind)
        }
    }

    fn fill_pose(
        &mut self,
        kind: SceneAddKind,
        render_fx_flags: u32,
        gfx_ent: u32,
        pose: Option<AddDObjPose>,
    ) {
        let Some(pose) = pose else {
            return;
        };
        write_scene_index_for_kind(
            kind,
            pose.entnum,
            &mut self.scene_model_index,
            &mut self.scene_dobj_index,
        );
        let info = pack_scene_info(pose.entnum, render_fx_flags, gfx_ent);
        match kind {
            SceneAddKind::Model(_) => {
                self.scene_models.push(GfxSceneModel {
                    origin: pose.origin,
                    lighting_origin: pose.lighting_origin,
                    radius: pose.radius,
                    info,
                    scale: 1.0,
                    quat: pose.quat,
                    posed_bounds: seed_bounds(pose.origin, pose.radius),
                });
            }
            SceneAddKind::Dobj(_) => {
                self.scene_dobjs.push(dobj_from_pose(pose, info));
            }
            SceneAddKind::DobjFlag30(_) => {
                self.scene_dobj_flag30.push(dobj_from_pose(pose, info));
            }
        }
    }

    pub fn store_pose_origin_quat(
        &mut self,
        entnum: u32,
        origin: [f32; 3],
        quat: Option<[f32; 4]>,
    ) {
        let i = entnum as usize;
        if let Some(&slot) = self.scene_dobj_index.get(i) {
            if slot != SCENE_INDEX_EMPTY {
                if let Some(dobj) = self.scene_dobjs.get_mut(usize::from(slot)) {
                    dobj.origin = origin;
                    dobj.quat = quat;
                    return;
                }
            }
        }
        if let Some(&slot) = self.scene_model_index.get(i) {
            if slot != SCENE_INDEX_EMPTY {
                if let Some(model) = self.scene_models.get_mut(usize::from(slot)) {
                    model.origin = origin;
                    model.quat = quat;
                }
            }
        }
    }

    pub fn store_scene_ent_lods(&mut self, entnum: u32, lods: &[i8]) {
        if let Some(dobj) = self.scene_dobj_mut(entnum) {
            dobj.lods.clear();
            dobj.lods
                .extend_from_slice(&lods[..lods.len().min(dpvs_iw4::SCENE_ENT_CULL_LOD_COUNT)]);
        }
    }

    pub fn store_scene_ent_skin(&mut self, entnum: u32, surfs: SceneEntSkinnedSurfs) {
        if let Some(dobj) = self.scene_dobj_mut(entnum) {
            dpvs_iw4::scene_dobj_gate_skinned(&mut dobj.cull_gate, surfs.summary.surface_count);
            dobj.skinned_surfs = Some(surfs);
        }
    }

    #[must_use]
    pub fn scene_ent_surface_count(&self, entnum: u32) -> Option<u32> {
        self.scene_dobj(entnum)
            .and_then(|dobj| dpvs_iw4::scene_dobj_surface_count(dobj.cull_gate))
    }

    #[must_use]
    pub fn scene_ent_skinned_surfs(&self, entnum: u32) -> Option<&SceneEntSkinnedSurfs> {
        self.scene_dobj(entnum)
            .and_then(|dobj| dobj.skinned_surfs.as_ref())
    }

    #[must_use]
    pub fn scene_dobj(&self, entnum: u32) -> Option<&GfxSceneDobj> {
        let slot = self
            .scene_dobj_index
            .get(entnum as usize)
            .copied()
            .filter(|slot| *slot != SCENE_INDEX_EMPTY)?;
        self.scene_dobjs.get(usize::from(slot))
    }

    #[must_use]
    pub fn scene_model(&self, entnum: u32) -> Option<&GfxSceneModel> {
        let slot = self
            .scene_model_index
            .get(entnum as usize)
            .copied()
            .filter(|slot| *slot != SCENE_INDEX_EMPTY)?;
        self.scene_models.get(usize::from(slot))
    }

    fn scene_dobj_mut(&mut self, entnum: u32) -> Option<&mut GfxSceneDobj> {
        let slot = self
            .scene_dobj_index
            .get(entnum as usize)
            .copied()
            .filter(|slot| *slot != SCENE_INDEX_EMPTY)?;
        self.scene_dobjs.get_mut(usize::from(slot))
    }

    pub fn store_posed_bounds(&mut self, entnum: u32, bounds: dpvs_iw4::Bounds) {
        let i = entnum as usize;
        if let Some(&slot) = self.scene_dobj_index.get(i) {
            if slot != SCENE_INDEX_EMPTY {
                if let Some(dobj) = self.scene_dobjs.get_mut(usize::from(slot)) {
                    dobj.posed_bounds = Some(bounds);
                    return;
                }
            }
        }
        if let Some(&slot) = self.scene_model_index.get(i) {
            if slot != SCENE_INDEX_EMPTY {
                if let Some(model) = self.scene_models.get_mut(usize::from(slot)) {
                    model.posed_bounds = Some(bounds);
                }
            }
        }
    }
}

fn write_scene_index(table: &mut Vec<u16>, entnum: u32, slot: u16) {
    let n = dpvs_iw4::GFX_CFG_ENT_COUNT as usize;
    let i = entnum as usize;
    if i >= n {
        return;
    }
    if table.len() != n {
        table.resize(n, SCENE_INDEX_EMPTY);
    }
    table[i] = slot;
}

fn write_scene_index_for_kind(
    kind: SceneAddKind,
    entnum: u32,
    model_index: &mut Vec<u16>,
    dobj_index: &mut Vec<u16>,
) {
    match kind {
        SceneAddKind::Model(slot) => write_scene_index(model_index, entnum, slot as u16),
        SceneAddKind::Dobj(slot) => write_scene_index(dobj_index, entnum, slot as u16),
        SceneAddKind::DobjFlag30(_) => {}
    }
}

fn dobj_from_pose(pose: AddDObjPose, info: u32) -> GfxSceneDobj {
    GfxSceneDobj {
        origin: pose.origin,
        lighting_origin: pose.lighting_origin,
        radius: pose.radius,
        info,
        quat: pose.quat,
        cull_gate: dpvs_iw4::SCENE_DOBJ_GATE_IDLE,
        posed_bounds: seed_bounds(pose.origin, pose.radius),
        lods: Vec::new(),
        skinned_surfs: None,
    }
}

fn seed_bounds(origin: [f32; 3], radius: Option<f32>) -> Option<dpvs_iw4::Bounds> {
    radius.map(|r| dpvs_iw4::scene_dobj_initial_bounds(origin, r))
}
