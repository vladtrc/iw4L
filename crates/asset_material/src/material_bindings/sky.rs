use super::{BindingWriter, FrameStep, float4_bits};
pub fn prepare(
    steps: &mut Vec<FrameStep>,
    requested: &[u16],
    fixed: &[(u16, [u32; 4])],
    targets: &[u16],
) {
    let fixed: Vec<_> = fixed
        .iter()
        .copied()
        .filter(|(index, _)| requested.contains(index))
        .collect();
    let targets: Vec<_> = targets
        .iter()
        .copied()
        .filter(|index| requested.contains(index))
        .collect();
    if !fixed.is_empty() || !targets.is_empty() {
        steps.push(FrameStep::Sky { fixed, targets });
    }
}
pub fn produce(
    sources: &mut BindingWriter<'_>,
    fixed: &[(u16, [u32; 4])],
    targets: &[u16],
    authored: [f32; 4],
    forward_z: f32,
) {
    for &(index, row) in fixed {
        sources.set_constant_rows(index, &[row]);
    }
    if !targets.is_empty() {
        let row = float4_bits([sky_intensity(authored, forward_z); 4]);
        for &index in targets {
            sources.set_constant_rows(index, &[row]);
        }
    }
}
fn sky_intensity([angle0, angle1, factor0, factor1]: [f32; 4], forward_z: f32) -> f32 {
    let radians = f32::from_bits(0x3c8efa35);
    let cos0 = (((90.0 - angle0) * radians) as f64).cos() as f32;
    let cos1 = (((90.0 - angle1) * radians) as f64).cos() as f32;
    let delta = cos1 - cos0;
    let blend = if delta.abs() <= f32::from_bits(0x38d1b717) {
        0.0
    } else {
        let t = ((forward_z - cos0) / delta).clamp(0.0, 1.0);
        t * t
    };
    (1.0 - blend) * factor0 + blend * factor1
}
