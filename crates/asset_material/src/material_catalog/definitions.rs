use crate::asset_graph::{AssetRef, AssetRefCensus};
use bevy::prelude::{Image, Resource};
use fastfile_iw4::{AssetType, Ptr};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const TS_2D: u8 = 0;
pub const TS_FUNCTION: u8 = 1;
pub const TS_COLOR_MAP: u8 = 2;

pub const TS_DETAIL_MAP: u8 = 3;
pub const TS_NORMAL_MAP: u8 = 5;
pub const TS_SPECULAR_MAP: u8 = 8;
pub const TS_WATER_MAP: u8 = 0x0B;

pub const TS_T5_COLOR0_MAP: u8 = 0x0C;
pub const TS_T5_COLOR15_MAP: u8 = 0x1B;
pub const TS_T5_THROW_MAP: u8 = 0x1C;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageVariantId {
    pub payload: u64,
    pub usage: u32,
}

#[derive(Clone, Debug)]
pub struct AuthoredImage {
    pub namespace: crate::FamilyId,
    pub name: AssetRef,
    pub map_type: u8,
    pub semantic: u8,
    pub category: u8,
    pub use_srgb_reads: bool,
    pub width: u16,
    pub height: u16,
    pub depth: u16,
    pub level_count: u8,
    pub format: u32,
    pub payload: Arc<Vec<u8>>,
    pub decoded: Option<Arc<Image>>,
    pub common_owned: bool,
    pub decoded_variant: Option<ImageVariantId>,
    pub decoded_by: Option<u64>,

    pub pending_decode: Option<u64>,
}

impl AuthoredImage {
    pub(super) fn accepts_decoded_from(&self, source: &Self) -> bool {
        if self.map_type != source.map_type
            || self.semantic != source.semantic
            || self.category != source.category
            || self.use_srgb_reads != source.use_srgb_reads
            || self.width != source.width
            || self.height != source.height
            || self.depth != source.depth
            || self.level_count != source.level_count
            || self.format != source.format
            || self.payload != source.payload
        {
            return false;
        }
        match (self.decoded_variant, source.decoded_variant) {
            (Some(expected), Some(actual)) => expected == actual,
            (None, None) => !self.payload.is_empty(),
            _ => false,
        }
    }

    pub(super) fn take_decoded_from(&mut self, source: &mut Self) {
        self.decoded = source.decoded.take();
        self.decoded_variant = source.decoded_variant;
        self.decoded_by = source.decoded_by;
        self.common_owned = source.common_owned;
    }
}

#[derive(Clone, Debug, Default)]
pub struct StandInTextures {
    pub color: Option<(String, Arc<Image>, bool)>,
    pub normal: Option<(String, Arc<Image>, bool)>,
    pub specular: Option<(String, Arc<Image>, bool)>,
}

#[derive(Clone, Debug)]
pub struct MaterialTextureBinding {
    pub name_hash: u32,

    pub name_start: u8,
    pub name_end: u8,
    pub sampler_state: u8,
    pub semantic: u8,
    pub image: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct MaterialConstant {
    pub name_hash: u32,
    pub name: [u8; 12],
    pub literal: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct AuthoredShader {
    pub namespace: crate::FamilyId,
    pub name: AssetRef,
    pub kind: AssetType,

    pub program: Vec<u8>,
}

impl AuthoredShader {
    pub const fn is_vertex(&self) -> bool {
        matches!(self.kind, AssetType::VertexShader)
    }

    pub const fn is_pixel(&self) -> bool {
        matches!(self.kind, AssetType::PixelShader)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredVertexDecl {
    pub layout: crate::VertexLayout,
    pub name: AssetRef,
    pub stream_count: u8,
    pub has_optional_source: u8,
    pub routing: [[u8; 2]; asset_iw4::vertex_decl::ROUTING_COUNT],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderSourceCensus {
    pub programs: usize,

    pub unresolved_aliases: usize,

    pub byteless: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VertexDeclStreamCensus {
    pub n: usize,
    pub stream0: usize,
    pub ppcc_n: usize,
    pub ppcc_stream_count: Option<u8>,
}

#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetRefDumpCensus {
    pub materials: AssetRefCensus,
    pub images: AssetRefCensus,
    pub shaders: AssetRefCensus,
    pub decls: AssetRefCensus,

    pub by_family: BTreeMap<crate::FamilyId, usize>,
}

impl AssetRefDumpCensus {
    pub fn from_catalog(catalog: &MaterialDefinitions) -> Self {
        Self {
            materials: catalog.material_ref_census(),
            images: catalog.image_ref_census(),
            shaders: catalog.shader_ref_census(),
            decls: catalog.vertex_decl_ref_census(),
            by_family: catalog
                .materials
                .iter()
                .fold(BTreeMap::new(), |mut counts, material| {
                    *counts.entry(material.namespace).or_default() += 1;
                    counts
                }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssetPointerIdentity {
    pub block: u8,
    pub offset: u32,
}

impl From<Ptr> for AssetPointerIdentity {
    fn from(pointer: Ptr) -> Self {
        Self {
            block: pointer.block,
            offset: pointer.offset,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MaterialLinkAddress {
    pub identity: AssetPointerIdentity,
    pub aliasable: bool,
}
impl MaterialLinkAddress {
    pub const fn new(block: u8, offset: u32, aliasable: bool) -> Self {
        Self {
            identity: AssetPointerIdentity { block, offset },
            aliasable,
        }
    }
}
impl From<Ptr> for MaterialLinkAddress {
    fn from(pointer: Ptr) -> Self {
        Self::new(
            pointer.block,
            pointer.offset,
            fastfile_iw4::block_is_aliasable(pointer.block),
        )
    }
}
impl From<MaterialLinkAddress> for AssetPointerIdentity {
    fn from(address: MaterialLinkAddress) -> Self {
        address.identity
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedShaderRef {
    pub pointer_identity: AssetPointerIdentity,
    pub shader: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedShaderArgument {
    MaterialVertexConstant {
        destination: u16,
        name_hash: u32,
    },
    LiteralVertexConstant {
        destination: u16,
        words: Option<[u32; 4]>,
    },
    MaterialPixelSampler {
        destination: u16,
        name_hash: u32,
    },
    CodeVertexConstant {
        destination: u16,
        index: u16,
        first_row: u8,
        row_count: u8,
    },
    CodePixelSampler {
        destination: u16,
        index: u32,
    },
    CodePixelConstant {
        destination: u16,
        index: u16,
        first_row: u8,
        row_count: u8,
    },
    MaterialPixelConstant {
        destination: u16,
        name_hash: u32,
    },

    LiteralPixelConstant {
        destination: u16,
        words: Option<[u32; 4]>,
    },
    Unknown {
        argument_type: u16,
        raw: [u8; 8],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedMaterialPass {
    pub pass_index: u8,
    pub vertex_decl_identity: AssetPointerIdentity,

    pub vertex_decl: Option<usize>,
    pub vertex_shader: OwnedShaderRef,
    pub pixel_shader: OwnedShaderRef,
    pub per_prim_arg_count: u8,
    pub per_obj_arg_count: u8,
    pub stable_arg_count: u8,

    pub custom_sampler_flags: u8,

    pub t5_custom_sampler_flags: u8,
    pub arguments: Vec<OwnedShaderArgument>,
    pub arguments_truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedTechnique {
    pub source_selection: Option<render_material::SourceTechniqueSelection>,
    pub flags: u16,
    pub passes: Vec<OwnedMaterialPass>,
    pub body_scanned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedTechniqueGraph {
    pub slots: Vec<Option<OwnedTechnique>>,
    pub rows_truncated: u16,
    pub arguments_truncated: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechniqueTable {
    pub slots: u64,

    pub scanned: u64,

    pub technique0_flags: u8,

    pub model_lighting_const: Option<bool>,

    pub max_pass_count: u16,

    pub pass_count_by_slot: [u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT],

    pub graph: Option<OwnedTechniqueGraph>,
}

impl Default for TechniqueTable {
    fn default() -> Self {
        Self {
            slots: 0,
            scanned: 0,
            technique0_flags: 0,
            model_lighting_const: None,
            max_pass_count: 0,
            pass_count_by_slot: [0; asset_iw4::size::TECHNIQUE_SLOT_COUNT],
            graph: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceTechniqueOccupancy {
    pub slots: Vec<u64>,
    pub scanned: Vec<u64>,
    pub technique0_flags: u8,
    pub max_pass_count: u16,
    pub pass_count_by_slot: Vec<u8>,
}
impl SourceTechniqueOccupancy {
    pub fn slot_occupied(&self, slot: usize) -> bool {
        self.slots
            .get(slot / 64)
            .is_some_and(|word| word & (1u64 << (slot % 64)) != 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossGameReason {
    FeatureVariantDonor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossGameTechsetResolution {
    pub want_namespace: crate::FamilyId,
    pub want_name: String,
    pub got_namespace: crate::FamilyId,
    pub got_name: String,
    pub reason: CrossGameReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechniqueSetFacts {
    pub namespace: crate::FamilyId,
    pub name: AssetRef,

    pub zone: crate::ZoneOwner,

    pub table: Option<TechniqueTable>,

    pub source_occupancy: Option<SourceTechniqueOccupancy>,

    pub fallback_table: Option<TechniqueTable>,

    pub world_vert_format: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TechsetKey<'a> {
    pub namespace: crate::FamilyId,
    pub name: &'a str,
}

impl<'a> TechsetKey<'a> {
    pub fn new(namespace: crate::FamilyId, name: &'a str) -> Self {
        Self { namespace, name }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TechsetResolve<'a> {
    Hit {
        index: usize,
        facts: &'a TechniqueSetFacts,
    },

    GraphMissing {
        index: usize,
    },

    Foreign {
        got: crate::FamilyId,
        index: usize,
    },
    Missing,
}

#[derive(Clone, Debug)]
pub struct AuthoredMaterial {
    pub name: AssetRef,

    pub namespace: crate::FamilyId,
    pub technique_set: AssetRef,

    pub technique_set_edge: crate::AssetEdge<crate::TechniqueSetSpace>,
    pub draw_surf: u64,
    pub sort_key: u8,

    pub info_game_flags: u8,

    pub texture_atlas: Option<[u8; 2]>,

    pub surface_type_bits: Option<u32>,
    pub layered_surface_types: Option<u32>,

    pub state_flags: u8,

    pub camera_region: u8,

    pub state_bits: Vec<[u32; 2]>,

    pub state_bits_entry: Option<[u8; asset_iw4::size::TECHNIQUE_SLOT_COUNT]>,

    pub native_state_bits_entry: Option<Vec<u8>>,

    pub technique_table: Option<TechniqueTable>,

    pub route: Option<asset_iw4::MaterialDrawRoute>,
    pub textures: Vec<MaterialTextureBinding>,
    pub constants: Vec<MaterialConstant>,

    pub zone: crate::asset_graph::ZoneOwner,
}

impl AuthoredMaterial {
    pub fn state_bits_agreement<T: PartialEq>(
        &self,
        decode: impl Fn([u32; 2]) -> T,
    ) -> asset_iw4::ColorPassAgreement<T> {
        asset_iw4::color_pass_agreement(
            self.state_bits_entry.as_ref(),
            &self.state_bits,
            self.route.map(|route| route.technique_slots),
            decode,
        )
    }

    pub fn agreed_draw_mode(&self) -> Option<crate::MaterialDrawMode> {
        self.state_bits_agreement(crate::MaterialDrawMode::from_state_bits)
            .agreed()
    }

    pub fn agreed_alpha_test_cutoff(&self) -> Option<Option<f32>> {
        self.state_bits_agreement(crate::alpha_test_cutoff_from_state_bits)
            .agreed()
    }

    pub fn is_sky(&self) -> bool {
        matches!(
            self.route.map(|route| route.pass),
            Some(asset_iw4::MaterialPass::Sky(_))
        )
    }

    pub fn is_multiply(&self) -> bool {
        self.agreed_draw_mode() == Some(crate::MaterialDrawMode::Multiply)
    }

    pub fn is_shadowcaster(&self) -> bool {
        matches!(
            self.route.map(|route| route.pass),
            Some(asset_iw4::MaterialPass::ShadowOnly)
        )
    }

    pub fn takes_model_lighting(&self) -> Option<bool> {
        Some(self.route?.takes_model_lighting())
    }

    pub fn is_unlit(&self) -> Option<bool> {
        Some(matches!(
            self.route?.pass,
            asset_iw4::MaterialPass::Unlit | asset_iw4::MaterialPass::Sky(_)
        ))
    }

    pub fn cull_face(&self) -> Option<crate::MaterialCullFace> {
        self.state_bits_agreement(crate::cull_face_from_state_bits)
            .agreed()
    }

    pub fn color_map_transform(&self) -> crate::ColorMapTransform {
        match self.agreed_draw_mode() {
            Some(
                crate::MaterialDrawMode::Blend
                | crate::MaterialDrawMode::Additive
                | crate::MaterialDrawMode::Screen,
            ) => crate::ColorMapTransform::Unknown,
            None => match self.route.map(|route| route.pass) {
                Some(
                    asset_iw4::MaterialPass::Lit
                    | asset_iw4::MaterialPass::Unlit
                    | asset_iw4::MaterialPass::Sky(_),
                ) => crate::ColorMapTransform::Square,
                Some(asset_iw4::MaterialPass::ShadowOnly) | None => {
                    crate::ColorMapTransform::Unknown
                }
            },
            Some(_) => crate::ColorMapTransform::Square,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MaterialDefinitions {
    pub materials: Vec<AuthoredMaterial>,
    pub images: Vec<AuthoredImage>,
    pub shaders: Vec<AuthoredShader>,
    pub vertex_decls: Vec<AuthoredVertexDecl>,
    pub(super) techsets: Vec<TechniqueSetFacts>,

    pub capture_gaps: usize,

    pub argument_gaps: BTreeMap<crate::FamilyId, BTreeMap<String, u32>>,

    pub link_reused_materials: usize,
    pub link_reused_images: usize,

    pub cross_game_techset_resolutions: Vec<CrossGameTechsetResolution>,

    /// Zones whose materials sort after the retail 4096, like a foreign
    /// common's: the other maps of a projectile_flight atlas, drawn with the match.
    pub sort_leftover_zones: std::collections::BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialImageMemory {
    pub images: usize,
    pub decoded_images: usize,
    pub payload_bytes: usize,
    pub decoded_bytes: usize,
}

impl MaterialImageMemory {
    pub const fn total_bytes(self) -> usize {
        self.payload_bytes + self.decoded_bytes
    }

    pub fn report_row(self, label: &str) -> String {
        format!(
            "{label}: images={} decoded={} payload={:.1}MiB decoded_pixels={:.1}MiB total={:.1}MiB",
            self.images,
            self.decoded_images,
            self.payload_bytes as f64 / (1024.0 * 1024.0),
            self.decoded_bytes as f64 / (1024.0 * 1024.0),
            self.total_bytes() as f64 / (1024.0 * 1024.0),
        )
    }
}

pub fn alternate_techset_name(family: crate::FamilyId, name: &str) -> String {
    crate::material_compile::compiler::compiler_for(family).map_or_else(
        || name.to_owned(),
        |compiler| compiler.alternate_techset_name(name),
    )
}

impl MaterialDefinitions {
    pub fn note_argument_gap(&mut self, family: crate::FamilyId, key: String) {
        *self
            .argument_gaps
            .entry(family)
            .or_default()
            .entry(key)
            .or_default() += 1;
    }
    pub(super) fn resolve_technique_set_in<'a>(
        techsets: &'a [TechniqueSetFacts],
        key: TechsetKey<'_>,
    ) -> TechsetResolve<'a> {
        let want = AssetRef::bare_name(key.name);
        if want.is_empty() {
            return TechsetResolve::Missing;
        }
        let find_named = |name: &str| {
            techsets.iter().enumerate().find(|(_, facts)| {
                facts.namespace == key.namespace
                    && facts.name.is_real()
                    && facts.name.as_str() == name
            })
        };
        if let Some((index, facts)) = find_named(want) {
            if facts.table.is_some() {
                return TechsetResolve::Hit { index, facts };
            }
            let stripped = alternate_techset_name(key.namespace, want);
            if stripped != want
                && let Some((index, facts)) = find_named(&stripped)
                && facts.table.is_some()
            {
                return TechsetResolve::Hit { index, facts };
            }
            return TechsetResolve::GraphMissing { index };
        }
        let stripped = alternate_techset_name(key.namespace, want);
        if stripped != want
            && let Some((index, facts)) = find_named(&stripped)
        {
            if facts.table.is_some() {
                return TechsetResolve::Hit { index, facts };
            }
            return TechsetResolve::GraphMissing { index };
        }
        if let Some((index, facts)) = techsets.iter().enumerate().find(|(_, facts)| {
            facts.namespace != key.namespace
                && facts.name.is_real()
                && facts.name.as_str() == want
                && facts.table.is_some()
        }) {
            return TechsetResolve::Foreign {
                got: facts.namespace,
                index,
            };
        }
        TechsetResolve::Missing
    }

    fn ref_census<'a>(names: impl Iterator<Item = &'a AssetRef>) -> AssetRefCensus {
        let mut census = AssetRefCensus::default();
        for name in names {
            census.push(name);
        }
        census
    }

    pub fn image_memory(&self) -> MaterialImageMemory {
        let mut census = MaterialImageMemory {
            images: self.images.len(),
            ..MaterialImageMemory::default()
        };
        for image in &self.images {
            census.payload_bytes += image.payload.len();
            if let Some(decoded) = image.decoded.as_ref() {
                census.decoded_images += 1;
                census.decoded_bytes += decoded.data.as_ref().map_or(0, Vec::len);
            }
        }
        census
    }

    pub fn namespace_count(&self, ns: crate::FamilyId) -> usize {
        self.materials.iter().filter(|m| m.namespace == ns).count()
    }

    pub fn zone_of(&self, index: usize) -> crate::asset_graph::ZoneOwner {
        self.materials
            .get(index)
            .map(|material| material.zone)
            .unwrap_or(asset_core::ZoneOwner::UNKNOWN)
    }

    pub fn material_index_by_key(&self, key: &crate::MaterialKey) -> Option<crate::MaterialIndex> {
        self.material_index_by_ns(key.namespace, &key.name)
    }

    pub fn material_index_by_ns(
        &self,
        namespace: crate::FamilyId,
        name: &str,
    ) -> Option<crate::MaterialIndex> {
        let want = AssetRef::bare_name(name);
        if want.is_empty() {
            return None;
        }
        self.materials
            .iter()
            .position(|m| m.namespace == namespace && m.name.is_real() && m.name.as_str() == want)
            .map(crate::MaterialIndex::from_order)
    }

    pub fn image_index_by_key(&self, namespace: crate::FamilyId, name: &str) -> Option<usize> {
        let want = AssetRef::bare_name(name);
        if want.is_empty() {
            return None;
        }
        self.images.iter().position(|image| {
            image.namespace == namespace && image.name.is_real() && image.name.as_str() == want
        })
    }

    pub fn material_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.materials.iter().map(|m| &m.name))
    }

    pub fn image_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.images.iter().map(|image| &image.name))
    }

    pub fn shader_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.shaders.iter().map(|shader| &shader.name))
    }

    pub fn vertex_decl_ref_census(&self) -> AssetRefCensus {
        Self::ref_census(self.vertex_decls.iter().map(|decl| &decl.name))
    }

    pub fn vertex_decl_stream_census(&self) -> VertexDeclStreamCensus {
        let mut census = VertexDeclStreamCensus {
            n: self.vertex_decls.len(),
            ..VertexDeclStreamCensus::default()
        };
        for decl in &self.vertex_decls {
            if decl.stream_count == 0 {
                census.stream0 += 1;
            }
            if decl.name.as_str() == "ppcc0t0t0nn" {
                census.ppcc_n += 1;
                census.ppcc_stream_count = Some(decl.stream_count);
            }
        }
        census
    }

    pub fn agreed_draw_mode_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| material.agreed_draw_mode().is_some())
            .count()
    }

    pub fn lit_band_draw_mode_conflict_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                let Some(route) = material.route else {
                    return false;
                };
                if !matches!(route.pass, asset_iw4::MaterialPass::Lit)
                    || route.takes_model_lighting()
                {
                    return false;
                }
                asset_iw4::lit_band_decode_conflicts(
                    material.state_bits_entry.as_ref(),
                    &material.state_bits,
                    route.technique_slots,
                    crate::MaterialDrawMode::from_state_bits,
                )
            })
            .count()
    }

    pub fn unresolved_draw_mode_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| material.agreed_draw_mode().is_none())
            .count()
    }

    pub fn technique_set_facts(&self) -> &[TechniqueSetFacts] {
        &self.techsets
    }

    pub fn resolve_technique_set(&self, key: TechsetKey<'_>) -> TechsetResolve<'_> {
        Self::resolve_technique_set_in(&self.techsets, key)
    }

    pub fn technique_set_edge_census(&self) -> crate::AssetEdgeCensus {
        let mut census = crate::AssetEdgeCensus::default();
        for material in &self.materials {
            census.push(material.technique_set_edge);
        }
        census
    }

    pub fn shader_source_census(&self) -> ShaderSourceCensus {
        ShaderSourceCensus {
            programs: self.shaders.len(),
            unresolved_aliases: self
                .shaders
                .iter()
                .filter(|shader| shader.name.is_reference())
                .count(),
            byteless: self
                .shaders
                .iter()
                .filter(|shader| shader.program.is_empty())
                .count(),
        }
    }

    pub fn unrouted_material_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| material.route.is_none())
            .count()
    }

    pub fn constant(material: &AuthoredMaterial, name: &str) -> Option<[f32; 4]> {
        material
            .constants
            .iter()
            .find(|constant| crate::material_constant_name(&constant.name) == name)
            .map(|constant| constant.literal)
    }

    pub fn material_animation(material: &AuthoredMaterial) -> [[f32; 4]; 4] {
        let uv_anim = Self::constant(material, "uvAnimParms").unwrap_or([0.0; 4]);
        let (Some(parms), Some(begin), Some(end)) = (
            Self::constant(material, "falloffParms"),
            Self::constant(material, "falloffBegin"),
            Self::constant(material, "falloffEndCo"),
        ) else {
            return [uv_anim, [0.0; 4], [0.0; 4], [0.0; 4]];
        };
        [uv_anim, parms, [begin[0], begin[1], begin[2], 1.0], end]
    }

    pub fn hud_image_name(&self, material: &AuthoredMaterial) -> Option<&str> {
        let named = |semantic: u8| {
            material
                .textures
                .iter()
                .find(|texture| texture.semantic == semantic && texture.image.is_some())
                .and_then(|texture| self.images.get(texture.image?))
                .map(|image| image.name.as_str())
                .filter(|name| !name.is_empty())
        };
        named(TS_2D).or_else(|| named(TS_COLOR_MAP))
    }

    pub fn color_binding_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                material
                    .textures
                    .iter()
                    .any(|texture| texture.semantic == TS_COLOR_MAP && texture.image.is_some())
            })
            .count()
    }

    pub fn normal_binding_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                material
                    .textures
                    .iter()
                    .any(|texture| texture.semantic == TS_NORMAL_MAP && texture.image.is_some())
            })
            .count()
    }

    pub fn alpha_test_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                material
                    .agreed_alpha_test_cutoff()
                    .is_some_and(|v| v.is_some())
            })
            .count()
    }

    pub fn blend_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| {
                matches!(
                    material.agreed_draw_mode(),
                    Some(crate::MaterialDrawMode::Blend)
                )
            })
            .count()
    }

    pub fn multiply_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| material.is_multiply())
            .count()
    }

    pub fn sky_count(&self) -> usize {
        self.materials
            .iter()
            .filter(|material| material.is_sky())
            .count()
    }
}
