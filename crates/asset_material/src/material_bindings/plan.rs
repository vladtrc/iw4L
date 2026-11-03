use super::{BindingWriter, MaterialFrameBindingInputs, float4_bits};
use bevy::math::Vec3;

#[derive(Clone, Debug)]
pub enum FrameStep {
    Static(Vec<(u16, [u32; 4])>),
    EyeOffset(u16),
    Producer(fn(&mut BindingWriter<'_>, &MaterialFrameBindingInputs)),
    Fog(super::fog::FogLayout),
    Sky {
        fixed: Vec<(u16, [u32; 4])>,
        targets: Vec<u16>,
    },
    Water {
        target: u16,
        angular_speed: f64,
    },
}
pub(super) fn prepare(requested: &[u16]) -> Vec<(asset_core::FamilyId, Vec<FrameStep>)> {
    super::material_binding_families()
        .iter()
        .map(|(id, family)| (id, (family.prepare_frame)(requested)))
        .filter(|(_, steps)| !steps.is_empty())
        .collect()
}
impl FrameStep {
    pub fn execute(&self, sources: &mut BindingWriter<'_>, inputs: &MaterialFrameBindingInputs) {
        match self {
            Self::Static(rows) => {
                for &(index, row) in rows {
                    sources.set_constant_rows(index, &[row]);
                }
            }
            Self::EyeOffset(index) => sources.set_constant_rows(
                *index,
                &[float4_bits([inputs.eye.x, inputs.eye.y, inputs.eye.z, 1.0])],
            ),
            Self::Producer(produce) => produce(sources, inputs),
            Self::Fog(layout) => super::fog::produce(
                sources,
                &inputs.fog,
                inputs.eye.z,
                inputs.fog_enabled,
                *layout,
            ),
            Self::Sky { fixed, targets } => {
                if let Some(authored) = inputs.world.sky_intensity {
                    let forward_z = inputs.world_from_view.transform_vector3(Vec3::NEG_Z).z;
                    super::sky::produce(sources, fixed, targets, authored, forward_z);
                }
            }
            Self::Water {
                target,
                angular_speed,
            } => {
                let phase = (*angular_speed * f64::from(inputs.time)) as f32;
                sources.set_constant_rows(*target, &[float4_bits([phase; 4])]);
            }
        }
    }
}
