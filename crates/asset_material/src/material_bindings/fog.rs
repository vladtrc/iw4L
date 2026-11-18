use super::{BindingWriter, FrameStep, MaterialFogInputs};
/// Row order: color, density, height integral, sun direction, sun color, sun angles.
#[derive(Clone, Copy, Debug)]
pub struct FogLayout {
    pub rows: [u16; 6],
    pub hdr_color_scale: bool,
}
pub fn prepare(steps: &mut Vec<FrameStep>, requested: &[u16], layout: FogLayout) {
    if super::demands_any(requested, &layout.rows) {
        steps.push(FrameStep::Fog(layout));
    }
}
pub fn produce(
    sources: &mut BindingWriter<'_>,
    fog: &MaterialFogInputs,
    eye_z: f32,
    enabled: bool,
    layout: FogLayout,
) {
    let (height_density, base_height, color) = match fog.volumetric {
        Some(volume) => (
            if volume.halfway_height < 1.0 {
                0.0
            } else {
                1.0 / volume.halfway_height
            },
            volume.base_height,
            [
                fog.color_rgb[0],
                fog.color_rgb[1],
                fog.color_rgb[2],
                volume.color_scale,
            ],
        ),

        None => (
            0.0,
            0.0,
            super::fog_color_linear_and_gamma(fog.color_rgb, 1.0).0,
        ),
    };
    let (sun_color, sun_dir, angles) = match fog.sun {
        Some(sun) => {
            let rgb = if fog.volumetric.is_some() {
                sun.color_rgb
            } else {
                let linear = super::fog_color_linear_and_gamma(sun.color_rgb, 1.0).0;
                [linear[0], linear[1], linear[2]]
            };
            (
                [rgb[0], rgb[1], rgb[2], fog.max_opacity],
                [
                    sun.sun_dir[0],
                    sun.sun_dir[1],
                    sun.sun_dir[2],
                    sun.begin_angle_deg,
                ],
                [sun.begin_angle_deg, sun.end_angle_deg],
            )
        }
        None => ([0.0, 0.0, 0.0, fog.max_opacity], [0.0; 4], [0.0; 2]),
    };
    let start = angles[0].to_radians().cos();
    let end = angles[1].to_radians().cos();
    let slope = if start == end {
        1.0e7
    } else {
        1.0 / (start - end)
    };
    let density = if fog.halfway_dist > 0.0 {
        1.0 / fog.halfway_dist
    } else {
        0.0
    };
    let (fog_row, fog2) = density_rows(
        density,
        height_density,
        base_height,
        fog.start_dist,
        eye_z,
        enabled,
    );
    let scale = fog.volumetric.map_or(1.0, |volume| volume.color_scale);
    let hdr = |rgb: [f32; 4]| {
        [
            rgb[0] * scale,
            rgb[1] * scale,
            rgb[2] * scale,
            fog.max_opacity,
        ]
    };
    let rows = if layout.hdr_color_scale {
        [
            hdr(color),
            fog_row,
            fog2,
            sun_dir,
            hdr(sun_color),
            [-end * slope, slope, 0.0, 0.0],
        ]
    } else {
        [
            color,
            fog_row,
            fog2,
            sun_dir,
            sun_color,
            [-end * slope, slope, 0.0, 0.0],
        ]
    };
    for (code, row) in layout.rows.into_iter().zip(rows) {
        sources.set_constant_rows(code, &[row.map(f32::to_bits)]);
    }
}

fn density_rows(
    density: f32,
    height: f32,
    base: f32,
    start: f32,
    eye_z: f32,
    enabled: bool,
) -> ([f32; 4], [f32; 4]) {
    if !enabled || density == 0.0 {
        return ([0.0; 4], [0.0; 4]);
    }
    let max_density = 100.0 * density;
    let log_density =
        (density / max_density).ln() - height * std::f32::consts::LN_2 * (eye_z - base);
    let integral = if log_density >= 0.0 {
        log_density + 1.0
    } else {
        log_density.exp()
    };
    let scale = if height > 0.0 {
        -max_density * std::f32::consts::LN_2
    } else {
        -max_density
    };
    (
        [
            log_density,
            scale,
            start * density,
            -height * std::f32::consts::LN_2,
        ],
        [integral, 0.0, 0.0, 0.0],
    )
}
