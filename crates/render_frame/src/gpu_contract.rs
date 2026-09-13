use std::collections::{BTreeMap, BTreeSet};

use asset_iw4::vertex_decl::D3dDeclType;
use d3d9_sm3::PassWgsl;
use render_material::{PassProgramAbi, SamplerTextureDimension};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WgpuVertexFormat {
    Float32x2,
    Float32x3,
    Float32x4,
    Unorm8x4,
    Uint8x4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WgpuVertexAttribute {
    pub location: u32,
    pub offset: u64,
    pub format: WgpuVertexFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuVertexBufferLayout {
    pub stream: u8,
    pub array_stride: u64,
    pub attributes: Vec<WgpuVertexAttribute>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WgpuShaderVisibility {
    Vertex,
    Fragment,
    VertexFragment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WgpuBindingKind {
    UniformBuffer {
        min_size: u64,
    },
    ReadOnlyStorageBuffer,

    TextureArray {
        dimension: SamplerTextureDimension,
        count: u32,
    },

    SamplerArray {
        count: u32,
    },
}

pub const TEXTURE_TABLE_2D_CAPACITY: u32 = 8192;
pub const TEXTURE_TABLE_CUBE_CAPACITY: u32 = 256;
pub const TEXTURE_TABLE_3D_CAPACITY: u32 = 32;
pub const TEXTURE_TABLE_SAMPLER_CAPACITY: u32 = 128;

pub fn texture_table_bind_entries() -> [WgpuBindLayoutEntry; 4] {
    let entry = |binding: u32, kind| WgpuBindLayoutEntry {
        group: u8::try_from(d3d9_sm3::TEXTURE_TABLE_GROUP).expect("group 1"),
        binding: u16::try_from(binding).expect("table binding fits u16"),
        visibility: WgpuShaderVisibility::VertexFragment,
        kind,
        retail_register: None,
    };
    [
        entry(
            d3d9_sm3::TEXTURE_TABLE_BINDING_2D,
            WgpuBindingKind::TextureArray {
                dimension: SamplerTextureDimension::D2,
                count: TEXTURE_TABLE_2D_CAPACITY,
            },
        ),
        entry(
            d3d9_sm3::TEXTURE_TABLE_BINDING_CUBE,
            WgpuBindingKind::TextureArray {
                dimension: SamplerTextureDimension::Cube,
                count: TEXTURE_TABLE_CUBE_CAPACITY,
            },
        ),
        entry(
            d3d9_sm3::TEXTURE_TABLE_BINDING_3D,
            WgpuBindingKind::TextureArray {
                dimension: SamplerTextureDimension::D3,
                count: TEXTURE_TABLE_3D_CAPACITY,
            },
        ),
        entry(
            d3d9_sm3::TEXTURE_TABLE_BINDING_SAMPLERS,
            WgpuBindingKind::SamplerArray {
                count: TEXTURE_TABLE_SAMPLER_CAPACITY,
            },
        ),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WgpuBindLayoutEntry {
    pub group: u8,
    pub binding: u16,
    pub visibility: WgpuShaderVisibility,
    pub kind: WgpuBindingKind,

    pub retail_register: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuPassLayout {
    pub vertex_buffers: Vec<WgpuVertexBufferLayout>,
    pub bind_entries: Vec<WgpuBindLayoutEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WgpuLayoutRefusal {
    UnknownVertexFormat {
        source: u8,
        raw: u8,
    },
    MissingStreamExtent {
        vertex_type: u8,
        stream: u8,
    },
    DuplicateShaderLocation {
        location: u32,
    },
    VertexAttributePastStride {
        stream: u8,
        offset: u8,
        byte_len: u8,
        stride: u16,
    },
}

pub fn derive_wgpu_pass_layout(
    abi: &PassProgramAbi,
    _module: &PassWgsl,
) -> Result<WgpuPassLayout, WgpuLayoutRefusal> {
    let mut by_stream = BTreeMap::<u8, Vec<WgpuVertexAttribute>>::new();
    let mut locations = BTreeSet::new();
    for attribute in &abi.attributes {
        if !locations.insert(attribute.location) {
            return Err(WgpuLayoutRefusal::DuplicateShaderLocation {
                location: attribute.location,
            });
        }
        let (format, byte_len) = vertex_format(attribute.source, attribute.layout.decl_type)?;
        let stride = abi
            .vertex_family
            .host_stream_stride(abi.vertex_type, attribute.layout.stream)
            .ok_or(WgpuLayoutRefusal::MissingStreamExtent {
                vertex_type: abi.vertex_type,
                stream: attribute.layout.stream,
            })?;
        let end = u16::from(attribute.layout.offset) + u16::from(byte_len);
        if end > stride {
            return Err(WgpuLayoutRefusal::VertexAttributePastStride {
                stream: attribute.layout.stream,
                offset: attribute.layout.offset,
                byte_len,
                stride,
            });
        }
        by_stream
            .entry(attribute.layout.stream)
            .or_default()
            .push(WgpuVertexAttribute {
                location: attribute.location,
                offset: u64::from(attribute.layout.offset),
                format,
            });
    }
    let vertex_buffers = by_stream
        .into_iter()
        .map(|(stream, mut attributes)| {
            attributes.sort_by_key(|attribute| (attribute.offset, attribute.location));
            WgpuVertexBufferLayout {
                stream,
                array_stride: u64::from(
                    abi.vertex_family
                        .host_stream_stride(abi.vertex_type, stream)
                        .expect("stream extent was checked above"),
                ),
                attributes,
            }
        })
        .collect();

    let mut bind_entries = Vec::new();
    bind_entries.push(WgpuBindLayoutEntry {
        group: 0,
        binding: 0,
        visibility: WgpuShaderVisibility::VertexFragment,
        kind: WgpuBindingKind::ReadOnlyStorageBuffer,
        retail_register: None,
    });

    bind_entries.extend(texture_table_bind_entries());

    Ok(WgpuPassLayout {
        vertex_buffers,
        bind_entries,
    })
}

fn vertex_format(
    source: u8,
    decl_type: D3dDeclType,
) -> Result<(WgpuVertexFormat, u8), WgpuLayoutRefusal> {
    let format = match decl_type {
        D3dDeclType::Float2 => WgpuVertexFormat::Float32x2,
        D3dDeclType::Float3 => WgpuVertexFormat::Float32x3,
        D3dDeclType::Float4 => WgpuVertexFormat::Float32x4,

        D3dDeclType::D3dColor | D3dDeclType::UByte4N => WgpuVertexFormat::Unorm8x4,

        D3dDeclType::UByte4 => WgpuVertexFormat::Uint8x4,
        D3dDeclType::Unknown(raw) => {
            return Err(WgpuLayoutRefusal::UnknownVertexFormat { source, raw });
        }
    };
    Ok((
        format,
        decl_type
            .byte_len()
            .expect("all accepted declaration types have a byte length"),
    ))
}
