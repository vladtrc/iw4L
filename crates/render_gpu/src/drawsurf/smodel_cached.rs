use asset_iw4::vertex_decl as vd;
use bevy::mesh::VertexBufferLayout;
use bevy::render::render_resource::{VertexAttribute, VertexFormat, VertexStepMode};

use render_material::PassProgramAbi;

pub const SMODEL_PRETESS_VERTEX: usize = asset_iw4::size::GFX_PACKED_VERTEX + 16;

pub fn cached_lighting_location(abi: &PassProgramAbi) -> Option<u32> {
    let max = abi
        .vertex_inputs
        .iter()
        .map(|input| input.attribute.map(|attribute| attribute.location))
        .max()??;
    max.checked_add(1)
}

pub fn lighting_texcoord_register(abi: &PassProgramAbi) -> Option<String> {
    abi.varyings.iter().find_map(|varying| {
        if varying.semantic.usage == vd::D3DDECLUSAGE_TEXCOORD && varying.semantic.usage_index == 6
        {
            Some(format!("o{}", varying.vertex_register.index))
        } else {
            None
        }
    })
}

pub fn cached_lighting_vertex_buffers(
    packed: &[VertexBufferLayout],
    location: u32,
) -> Vec<VertexBufferLayout> {
    packed
        .iter()
        .map(|buffer| {
            let mut attributes = buffer.attributes.to_vec();
            attributes.push(VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: asset_iw4::size::GFX_PACKED_VERTEX as u64,
                shader_location: location,
            });
            VertexBufferLayout {
                array_stride: SMODEL_PRETESS_VERTEX as u64,
                step_mode: VertexStepMode::Vertex,
                attributes: attributes.into(),
            }
        })
        .collect()
}

pub fn inject_cached_lighting_attribute(
    source: &str,
    location: u32,
    lighting_register: Option<&str>,
) -> Option<String> {
    const SIG_END: &str = ") -> Sm3Varyings {";
    let start = source.find("@vertex")?;
    let sig = source[start..].find(SIG_END)? + start;
    let mut out = String::with_capacity(source.len() + 160);
    out.push_str(&source[..sig]);
    out.push_str(&format!(
        "    @location({location}) attribute_{location}: vec4<f32>,\n"
    ));
    out.push_str(SIG_END);
    let after_sig = sig + SIG_END.len();
    out.push_str(&format!(
        "\n    let lighting_coords = attribute_{location};\n"
    ));
    if let Some(register) = lighting_register {
        const RET: &str = "    return Sm3Varyings(";
        let rel = source[after_sig..].find(RET)?;
        let at = after_sig + rel;
        out.push_str(&source[after_sig..at]);
        out.push_str(&format!("    {register} = lighting_coords;\n"));
        out.push_str(&source[at..]);
    } else {
        out.push_str(&source[after_sig..]);
    }
    Some(out)
}
