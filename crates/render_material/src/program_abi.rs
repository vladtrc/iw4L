use crate::{
    PassAbiRefusal, PassProgramAbi, PortId, RuntimeMaterialCatalog, RuntimePass, RuntimeShaderPair,
    RuntimeShaderProgram, RuntimeShaderStage, RuntimeVertexDecl, Sm3IrError,
};
use asset_core::FamilyId;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramLoweringError {
    pub family: FamilyId,
    pub reason: String,
    pub census_key: String,
}

pub trait PreparedShaderProgram: Send + Sync {
    /// Backend-specific data affecting emitted source. None disables disk caching.
    fn cache_key(&self, abi: &PassProgramAbi) -> Option<Vec<u8>>;
    fn lower(&self, abi: &PassProgramAbi) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError>;
    fn lower_exit_clip(
        &self,
        abi: &PassProgramAbi,
        _mode: d3d9_sm3::ExitClipMode,
    ) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        Err(ProgramLoweringError {
            family: abi.vertex_layout.owner,
            reason: "material adapter has no exit-plane lowering".into(),
            census_key: "Wgsl/ExitClipUnsupported".into(),
        })
    }
}

/// Retain the admitted program's IR so GPU variants are lowered lazily rather
/// than reconstructed by editing WGSL. Identity is the content-addressed port;
/// its ABI is retained and compared separately by the port owner.
#[derive(Clone)]
pub struct ProgramLoweringRecipe {
    id: PortId,
    program: std::sync::Arc<dyn PreparedShaderProgram>,
}
impl ProgramLoweringRecipe {
    pub fn new(id: PortId, program: Box<dyn PreparedShaderProgram>) -> Self {
        Self {
            id,
            program: program.into(),
        }
    }
    pub fn lower_exit_clip(
        &self,
        abi: &PassProgramAbi,
        mode: d3d9_sm3::ExitClipMode,
    ) -> Result<d3d9_sm3::PassWgsl, ProgramLoweringError> {
        self.program.lower_exit_clip(abi, mode)
    }
}
impl std::fmt::Debug for ProgramLoweringRecipe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ProgramLoweringRecipe")
            .field(&self.id)
            .finish()
    }
}
impl PartialEq for ProgramLoweringRecipe {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for ProgramLoweringRecipe {}

pub struct ProgramPreparation<'a> {
    pub pass: &'a RuntimePass,
    pub declaration: &'a RuntimeVertexDecl,
    pub vertex_type: u8,
    pub vertex: &'a RuntimeShaderProgram,
    pub pixel: &'a RuntimeShaderProgram,
}

pub struct PreparedMaterialProgram {
    pub abi: PassProgramAbi,
    pub program: Box<dyn PreparedShaderProgram>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightingConstantBindings {
    pub reflection_sh: &'static [u16],
    pub grid_sh: &'static [u16],
}

pub trait MaterialAdapter: Send + Sync + 'static {
    fn lighting_constants(&self) -> LightingConstantBindings;
    fn prepare(
        &self,
        input: ProgramPreparation<'_>,
    ) -> Result<PreparedMaterialProgram, ProgramAbiRefusal>;
}

#[derive(Default)]
pub struct MaterialAdapters(BTreeMap<FamilyId, &'static dyn MaterialAdapter>);
impl MaterialAdapters {
    pub fn register(&mut self, family: FamilyId, adapter: &'static dyn MaterialAdapter) {
        assert!(!self.0.contains_key(&family), "duplicate material adapter");
        self.0.insert(family, adapter);
    }
}
static ADAPTERS: OnceLock<MaterialAdapters> = OnceLock::new();
pub fn material_lighting_bindings() -> impl Iterator<Item = LightingConstantBindings> {
    ADAPTERS
        .get()
        .expect("builder must install material adapters before rendering")
        .0
        .values()
        .map(|adapter| adapter.lighting_constants())
}

pub fn install_material_adapters(adapters: MaterialAdapters) -> Result<(), &'static str> {
    ADAPTERS
        .set(adapters)
        .map_err(|_| "material adapters already installed")
}

pub struct PreparedProgramAbi<'a> {
    id: PortId,
    pair: RuntimeShaderPair,
    vertex_name: &'a str,
    pixel_name: &'a str,
    abi: PassProgramAbi,
    program: Box<dyn PreparedShaderProgram>,
}
impl<'a> PreparedProgramAbi<'a> {
    pub fn id(&self) -> PortId {
        self.id
    }
    pub fn pair(&self) -> RuntimeShaderPair {
        self.pair
    }
    pub fn names(&self) -> (&'a str, &'a str) {
        (self.vertex_name, self.pixel_name)
    }
    pub fn into_programs(self) -> (PassProgramAbi, Box<dyn PreparedShaderProgram>) {
        (self.abi, self.program)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramAbiRefusal {
    PassShaderPairMissing,
    MissingExactSource {
        pair: RuntimeShaderPair,
        stage: RuntimeShaderStage,
    },
    StageMismatch {
        pair: RuntimeShaderPair,
        expected: RuntimeShaderStage,
        actual: RuntimeShaderStage,
    },
    MissingVertexDeclaration {
        pointer_identity: u32,
    },
    Decode {
        stage: RuntimeShaderStage,
        cause: Sm3IrError,
    },
    Abi(PassAbiRefusal),
    MissingMaterialAdapter {
        family: asset_core::FamilyId,
    },
    Adapter {
        family: asset_core::FamilyId,
        reason: String,
    },
}

impl ProgramAbiRefusal {
    pub fn census_key(&self) -> String {
        match self {
            Self::StageMismatch { .. } => "StageMismatch".into(),
            Self::Abi(refusal) => abi_census_key(*refusal),
            Self::Decode { cause, .. } => format!("Decode({cause:?})")
                .split(['{', '('])
                .next()
                .unwrap_or("Decode")
                .trim()
                .to_owned(),
            other => {
                let text = format!("{other:?}");
                text.split(['{', '('])
                    .next()
                    .unwrap_or(&text)
                    .trim()
                    .to_owned()
            }
        }
    }
}

pub fn prepare_program_abi<'a>(
    catalog: &'a RuntimeMaterialCatalog,
    pass: &RuntimePass,
    vertex_type: u8,
) -> Result<PreparedProgramAbi<'a>, ProgramAbiRefusal> {
    let pair = pass
        .shader_pair
        .ok_or(ProgramAbiRefusal::PassShaderPairMissing)?;
    let vertex = exact_stage_source(catalog, pair, RuntimeShaderStage::Vertex)?;
    let pixel = exact_stage_source(catalog, pair, RuntimeShaderStage::Pixel)?;
    let declaration = catalog.vertex_decl(pair.vertex_decl_slot).ok_or(
        ProgramAbiRefusal::MissingVertexDeclaration {
            pointer_identity: pair.vertex_decl_slot,
        },
    )?;
    let family = declaration.layout.owner;
    let adapter = ADAPTERS
        .get()
        .and_then(|adapters| adapters.0.get(&family))
        .ok_or(ProgramAbiRefusal::MissingMaterialAdapter { family })?;
    let prepared = adapter.prepare(ProgramPreparation {
        pass,
        declaration,
        vertex_type,
        vertex,
        pixel,
    })?;
    let id =
        PortId::from_pass(pass, vertex_type).ok_or(ProgramAbiRefusal::PassShaderPairMissing)?;
    Ok(PreparedProgramAbi {
        id,
        pair,
        vertex_name: &vertex.name,
        pixel_name: &pixel.name,
        abi: prepared.abi,
        program: prepared.program,
    })
}

fn exact_stage_source(
    catalog: &RuntimeMaterialCatalog,
    pair: RuntimeShaderPair,
    expected: RuntimeShaderStage,
) -> Result<&RuntimeShaderProgram, ProgramAbiRefusal> {
    let id = match expected {
        RuntimeShaderStage::Vertex => pair.vertex,
        RuntimeShaderStage::Pixel => pair.pixel,
    };
    let program = catalog
        .shader_program(id)
        .ok_or(ProgramAbiRefusal::MissingExactSource {
            pair,
            stage: expected,
        })?;
    if program.stage != expected {
        return Err(ProgramAbiRefusal::StageMismatch {
            pair,
            expected,
            actual: program.stage,
        });
    }
    Ok(program)
}

fn abi_census_key(refusal: crate::sm3_abi::PassAbiRefusal) -> String {
    match refusal {
        crate::sm3_abi::PassAbiRefusal::UnsupportedSamplerTextureType { texture_type, .. } => {
            format!("Abi/UnsupportedSamplerTextureType({texture_type})")
        }
        crate::sm3_abi::PassAbiRefusal::UnknownArgumentType { argument_type } => {
            format!("Abi/UnknownArgumentType({argument_type})")
        }
        crate::sm3_abi::PassAbiRefusal::VertexTypeMissingSource {
            vertex_type,
            source,
        } => format!("Abi/VertexTypeMissingSource({vertex_type},{source})"),
        crate::sm3_abi::PassAbiRefusal::ConstantRegisterUnbound { stage, register } => {
            format!("Abi/ConstantRegisterUnbound({stage:?},{register})")
        }
        crate::sm3_abi::PassAbiRefusal::VertexInputUnrouted { semantic, .. } => {
            format!("Abi/VertexInputUnrouted({semantic:?})")
        }
        crate::sm3_abi::PassAbiRefusal::SamplerRegisterConflict { register } => {
            format!("Abi/SamplerRegisterConflict({register})")
        }
        crate::sm3_abi::PassAbiRefusal::SamplerRegisterUnbound { register } => {
            format!("Abi/SamplerRegisterUnbound({register})")
        }
        other => {
            let text = format!("{other:?}");
            let name = text.split(['{', '(']).next().unwrap_or(&text).trim();
            format!("Abi/{name}")
        }
    }
}
