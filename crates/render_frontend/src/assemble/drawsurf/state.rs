use bevy::render::render_resource::{
    BlendComponent, BlendFactor as WgpuBlendFactor, BlendOperation, BlendState, ColorWrites,
};
use d3d9_state::BlendFactor;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DrawBlendOperation {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
    Unknown(u8),
}

impl DrawBlendOperation {
    fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::Add,
            2 => Self::Subtract,
            3 => Self::ReverseSubtract,
            4 => Self::Min,
            5 => Self::Max,
            value => Self::Unknown(value),
        }
    }

    fn to_wgpu(self) -> Option<BlendOperation> {
        Some(match self {
            Self::Add => BlendOperation::Add,
            Self::Subtract => BlendOperation::Subtract,
            Self::ReverseSubtract => BlendOperation::ReverseSubtract,
            Self::Min => BlendOperation::Min,
            Self::Max => BlendOperation::Max,
            Self::Unknown(_) => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct DrawBlendComponent {
    pub src: BlendFactor,
    pub dst: BlendFactor,
    pub operation: DrawBlendOperation,
}

impl DrawBlendComponent {
    fn from_bits(bits: u32) -> Self {
        Self {
            src: BlendFactor::from_raw(bits & 0xf),
            dst: BlendFactor::from_raw((bits >> 4) & 0xf),
            operation: DrawBlendOperation::from_raw(((bits >> 8) & 0x7) as u8),
        }
    }

    fn to_wgpu(self) -> BlendComponent {
        BlendComponent {
            src_factor: d3d_blend_to_wgpu(self.src)
                .expect("unsupported D3D9 source blend factor reached the GPU adapter"),
            dst_factor: d3d_blend_to_wgpu(self.dst)
                .expect("unsupported D3D9 destination blend factor reached the GPU adapter"),
            operation: self
                .operation
                .to_wgpu()
                .expect("unsupported D3D9 blend operation reached the GPU adapter"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum DrawBlend {
    #[default]
    Opaque,

    Factors {
        colour: DrawBlendComponent,
        alpha: DrawBlendComponent,
    },

    Multiply {
        alpha: DrawBlendComponent,
    },
}

impl DrawBlend {
    pub fn from_word0(word0: u32, multiply_pass: bool) -> Self {
        if multiply_pass {
            let colour = DrawBlendComponent {
                src: BlendFactor::Zero,
                dst: BlendFactor::SrcColor,
                operation: DrawBlendOperation::Add,
            };
            let alpha_bits = (word0 >> 16) & 0x7ff;
            let alpha = if alpha_bits & 0x700 == 0 {
                colour
            } else {
                DrawBlendComponent::from_bits(alpha_bits)
            };
            return Self::Multiply { alpha };
        }
        let blend_op = (word0 >> 8) & 0b111;
        let src = word0 & 0xf;
        let dst = (word0 >> 4) & 0xf;
        if blend_op == 0 {
            return Self::Opaque;
        }

        let colour = DrawBlendComponent::from_bits(word0);
        let alpha_bits = (word0 >> 16) & 0x7ff;
        let alpha = if alpha_bits & 0x700 == 0 {
            colour
        } else {
            DrawBlendComponent::from_bits(alpha_bits)
        };
        if blend_op == 1 && src == 1 && dst == 3 {
            Self::Multiply { alpha }
        } else {
            Self::Factors { colour, alpha }
        }
    }

    pub fn blend_state(self) -> Option<BlendState> {
        let (color, alpha) = match self {
            Self::Opaque => return None,
            Self::Factors { colour, alpha } => (colour.to_wgpu(), alpha.to_wgpu()),

            Self::Multiply { alpha } => (
                BlendComponent {
                    src_factor: WgpuBlendFactor::Zero,
                    dst_factor: WgpuBlendFactor::Src,
                    operation: BlendOperation::Add,
                },
                alpha.to_wgpu(),
            ),
        };
        Some(BlendState { color, alpha })
    }
}

fn d3d_blend_to_wgpu(factor: BlendFactor) -> Option<WgpuBlendFactor> {
    Some(match factor {
        BlendFactor::Zero => WgpuBlendFactor::Zero,
        BlendFactor::One => WgpuBlendFactor::One,
        BlendFactor::SrcColor => WgpuBlendFactor::Src,
        BlendFactor::InvSrcColor => WgpuBlendFactor::OneMinusSrc,
        BlendFactor::SrcAlpha => WgpuBlendFactor::SrcAlpha,
        BlendFactor::InvSrcAlpha => WgpuBlendFactor::OneMinusSrcAlpha,
        BlendFactor::DestAlpha => WgpuBlendFactor::DstAlpha,
        BlendFactor::InvDestAlpha => WgpuBlendFactor::OneMinusDstAlpha,
        BlendFactor::DestColor => WgpuBlendFactor::Dst,
        BlendFactor::InvDestColor => WgpuBlendFactor::OneMinusDst,
        BlendFactor::SrcAlphaSat => WgpuBlendFactor::SrcAlphaSaturated,
        BlendFactor::Unknown(_) => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChangeState0Host {
    pub blend: DrawBlend,

    pub cull: u8,
    pub srgb_write: bool,

    pub colour_write: u8,
    pub line_fill: bool,

    pub alpha_test: Option<d3d9_state::AlphaTest>,
}

impl ChangeState0Host {
    pub fn colour_writes(self) -> ColorWrites {
        let mut writes = ColorWrites::empty();
        if self.colour_write & 1 != 0 {
            writes |= ColorWrites::RED | ColorWrites::GREEN | ColorWrites::BLUE;
        }
        if self.colour_write & 2 != 0 {
            writes |= ColorWrites::ALPHA;
        }
        writes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChangeState1Host {
    pub depth_write: bool,
    pub depth_test_enable: bool,

    pub depth_func: u8,

    pub polyoffset_level: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GfxPassState {
    pub word0: u32,
    pub word1: u32,
    alpha_test: Option<d3d9_state::AlphaTest>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnsupportedStateFields {
    pub unknown_blend_factor: bool,
    pub unknown_blend_operation: bool,
    pub stencil: bool,
}

impl UnsupportedStateFields {
    pub fn any(self) -> bool {
        self.unknown_blend_factor || self.unknown_blend_operation || self.stencil
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AuthoredStateFields {
    pub non_add_blend: bool,
    pub independent_alpha_blend: bool,
    pub partial_colour_write: bool,
    pub line_fill: bool,
    pub stencil: bool,
}

impl GfxPassState {
    pub fn from_state_bits(word0: u32, word1: u32) -> Self {
        Self::for_material(assets::AssetNamespace::Iw4, [word0, word1])
    }

    pub fn for_material(namespace: assets::AssetNamespace, bits: [u32; 2]) -> Self {
        Self {
            word0: bits[0],
            word1: bits[1],
            alpha_test: assets::material_alpha_test(namespace, bits),
        }
    }

    pub fn from_bits(bits: render_material::GfxPassStateBits) -> Self {
        Self::for_material(bits.namespace, [bits.word0, bits.word1])
    }

    pub fn draw_mode(self) -> assets::MaterialDrawMode {
        assets::MaterialDrawMode::from_state_bits([self.word0, self.word1])
    }

    pub fn to_draw_blend(self, multiply_pass: bool) -> DrawBlend {
        DrawBlend::from_word0(self.word0, multiply_pass)
    }

    pub fn srgb_write_enable(self) -> bool {
        assets::srgb_write_enable_from_state_bits([self.word0, self.word1])
    }

    pub fn cull_face(self) -> assets::MaterialCullFace {
        assets::cull_face_from_state_bits([self.word0, self.word1])
    }

    pub fn apply_change_state_0_host(
        self,
        _alpha_mode: bevy::prelude::AlphaMode,
        multiply_pass: bool,
    ) -> ChangeState0Host {
        let cull = match self.cull_face() {
            assets::MaterialCullFace::Back => 1,
            assets::MaterialCullFace::Front => 2,
            assets::MaterialCullFace::None => 0,
        };
        ChangeState0Host {
            blend: self.to_draw_blend(multiply_pass),
            cull,
            srgb_write: self.srgb_write_enable(),
            colour_write: u8::from(self.word0 & 0x0800_0000 != 0)
                | (u8::from(self.word0 & 0x1000_0000 != 0) << 1),
            line_fill: self.word0 & 0x8000_0000 != 0,
            alpha_test: self.authored_alpha_test(),
        }
    }

    pub fn authored_host_fields(self) -> AuthoredStateFields {
        let colour_op = ((self.word0 >> 8) & 0x7) as u8;
        let colour_blend = self.word0 & 0x7ff;
        let alpha_blend = (self.word0 >> 16) & 0x7ff;
        let alpha_op = ((alpha_blend >> 8) & 0x7) as u8;
        AuthoredStateFields {
            non_add_blend: colour_op > 1 || (colour_op != 0 && alpha_op > 1),
            independent_alpha_blend: colour_op != 0 && alpha_op != 0 && alpha_blend != colour_blend,
            partial_colour_write: self.word0 & 0x1800_0000 != 0x1800_0000,
            line_fill: self.word0 & 0x8000_0000 != 0,
            stencil: self.word1 & 0xc0 != 0,
        }
    }

    pub fn authored_alpha_test(self) -> Option<d3d9_state::AlphaTest> {
        self.alpha_test
    }

    pub fn unsupported_host_fields(self) -> Option<UnsupportedStateFields> {
        let blend_op = ((self.word0 >> 8) & 0x7) as u8;
        let colour = DrawBlendComponent::from_bits(self.word0);
        let alpha_blend = (self.word0 >> 16) & 0x7ff;
        let alpha_blend_op = (alpha_blend >> 8) & 0x7;
        let alpha = DrawBlendComponent::from_bits(alpha_blend);
        let fields = UnsupportedStateFields {
            unknown_blend_factor: blend_op != 0
                && (!colour.src.is_known()
                    || !colour.dst.is_known()
                    || (alpha_blend_op != 0 && (!alpha.src.is_known() || !alpha.dst.is_known()))),
            unknown_blend_operation: blend_op > 5 || (blend_op != 0 && alpha_blend_op > 5),
            stencil: self.word1 & 0xc0 != 0,
        };
        fields.any().then_some(fields)
    }

    pub fn apply_change_state_1_host(self) -> ChangeState1Host {
        ChangeState1Host {
            depth_write: assets::depth_write_enable(self.word1),
            depth_test_enable: assets::depth_test_enable(self.word1),
            depth_func: ((self.word1 >> 2) & 3) as u8,
            polyoffset_level: assets::polygon_offset_level(self.word1) as u8,
        }
    }
}
