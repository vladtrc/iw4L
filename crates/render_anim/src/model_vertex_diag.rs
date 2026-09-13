use std::sync::OnceLock;

use bevy::prelude::Mesh;
use bevy::render::mesh::VertexAttributeValues;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelVertexColorMode {
    #[default]
    Current,

    WhiteVertex,

    PerSurface,
}

impl ModelVertexColorMode {
    pub fn from_env() -> Self {
        static MODE: OnceLock<ModelVertexColorMode> = OnceLock::new();
        *MODE.get_or_init(|| match std::env::var("IW4L_MODEL_VERTEX_COLOR") {
            Err(_) => Self::Current,
            Ok(raw) => match raw.as_str() {
                "current" | "" => Self::Current,
                "white_vertex" => {
                    diag::info!(
                        World,
                        "model vertex-color diagnostic: white_vertex (copy-only; source unchanged)"
                    );
                    Self::WhiteVertex
                }
                "per_surface" => {
                    diag::info!(
                        World,
                        "model vertex-color diagnostic: per_surface (copy-only; source unchanged)"
                    );
                    Self::PerSurface
                }
                other => {
                    diag::warn!(
                        World,
                        "unknown IW4L_MODEL_VERTEX_COLOR={other:?}; using current"
                    );
                    Self::Current
                }
            },
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::WhiteVertex => "white_vertex",
            Self::PerSurface => "per_surface",
        }
    }
}

fn surface_diag_rgba(surface_index: usize) -> [f32; 4] {
    const PALETTE: [[f32; 3]; 8] = [
        [1.0, 0.15, 0.15],
        [0.15, 1.0, 0.15],
        [0.2, 0.4, 1.0],
        [1.0, 1.0, 0.15],
        [1.0, 0.2, 1.0],
        [0.15, 1.0, 1.0],
        [1.0, 0.55, 0.1],
        [0.65, 0.25, 1.0],
    ];
    let rgb = PALETTE[surface_index % PALETTE.len()];
    [rgb[0], rgb[1], rgb[2], 1.0]
}

pub fn apply_model_vertex_color_diag(
    mesh: &mut Mesh,
    mode: ModelVertexColorMode,
    surface_index: usize,
) {
    match mode {
        ModelVertexColorMode::Current => {}
        ModelVertexColorMode::WhiteVertex => replace_mesh_colors(mesh, [1.0, 1.0, 1.0, 1.0]),
        ModelVertexColorMode::PerSurface => {
            replace_mesh_colors(mesh, surface_diag_rgba(surface_index));
        }
    }
}

fn replace_mesh_colors(mesh: &mut Mesh, rgba: [f32; 4]) {
    let Some(values) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else {
        return;
    };
    let count = match values {
        VertexAttributeValues::Float32x4(v) => v.len(),
        _ => return,
    };
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        VertexAttributeValues::Float32x4(vec![rgba; count]),
    );
}
