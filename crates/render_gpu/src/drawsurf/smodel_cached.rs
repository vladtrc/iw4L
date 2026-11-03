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
                attributes,
            }
        })
        .collect()
}

pub fn inject_cached_lighting_attribute(
    source: &str,
    location: u32,
    lighting_register: Option<&str>,
) -> Option<String> {
    let start = source.find("@vertex")?;
    let output = if source[start..].contains(") -> Sm3VertexOut {") {
        "Sm3VertexOut"
    } else {
        "Sm3Varyings"
    };
    let sig_end = format!(") -> {output} {{");
    let sig = source[start..].find(&sig_end)? + start;
    let mut out = String::with_capacity(source.len() + 160);
    out.push_str(&source[..sig]);
    out.push_str(&format!(
        "    @location({location}) attribute_{location}: vec4<f32>,\n"
    ));
    out.push_str(&sig_end);
    let after_sig = sig + sig_end.len();
    out.push_str(&format!(
        "\n    let lighting_coords = attribute_{location};\n"
    ));
    if let Some(register) = lighting_register {
        let ret = format!("    return {output}(");
        let rel = source[after_sig..].find(&ret)?;
        let at = after_sig + rel;
        out.push_str(&source[after_sig..at]);
        out.push_str(&format!("    {register} = lighting_coords;\n"));
        out.push_str(&source[at..]);
    } else {
        out.push_str(&source[after_sig..]);
    }
    Some(out)
}
