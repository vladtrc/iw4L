use render_material::RuntimeCodeSources;

const CODE_COLOR_BIAS: u16 = 46;
const CODE_COLOR_TINT_BASE: u16 = 47;
const CODE_COLOR_TINT_DELTA: u16 = 48;
const CODE_COLOR_TINT_QUADRATIC_DELTA: u16 = 49;
const FILM_DESAT_MIN: f32 = 1.0 / 4096.0;

fn float4_bits(row: [f32; 4]) -> [u32; 4] {
    row.map(f32::to_bits)
}

pub fn set_film_vision(sources: &mut RuntimeCodeSources, vision: Option<asset_world::FilmVision>) {
    let authored = vision.unwrap_or_else(asset_world::FilmVision::default);
    let vision = if authored.enable {
        authored
    } else {
        asset_world::FilmVision::default()
    };
    let desaturation = vision.desaturation.max(FILM_DESAT_MIN);
    let mut contrast = vision.contrast;
    let mut offset = vision.brightness + 0.5 - 0.5 * contrast;
    if vision.invert {
        contrast = -contrast;
        offset += 1.0;
    }
    let base = vision.dark_tint.map(|value| value * contrast);
    let delta: [f32; 3] = std::array::from_fn(|channel| {
        2.0 * contrast * (vision.medium_tint[channel] - vision.dark_tint[channel])
    });
    let quadratic: [f32; 3] = std::array::from_fn(|channel| {
        contrast
            * (vision.light_tint[channel] - 2.0 * vision.medium_tint[channel]
                + vision.dark_tint[channel])
    });
    for (slot, row) in [
        (
            CODE_COLOR_BIAS,
            [offset, offset, offset, 1.0 / desaturation - 1.0],
        ),
        (
            CODE_COLOR_TINT_BASE,
            [base[0], base[1], base[2], vision.desaturation_dark],
        ),
        (
            CODE_COLOR_TINT_DELTA,
            [
                delta[0],
                delta[1],
                delta[2],
                vision.desaturation - vision.desaturation_dark,
            ],
        ),
        (
            CODE_COLOR_TINT_QUADRATIC_DELTA,
            [quadratic[0], quadratic[1], quadratic[2], 0.0],
        ),
    ] {
        sources.set_constant_rows(slot, &[float4_bits(row)]);
    }
    let (setup, apply) = lighting_iw4::set_glow_info(
        authored.glow_bloom_cutoff,
        authored.glow_bloom_desaturation,
        authored.glow_bloom_intensity,
    )
    .map_or(([0; 4], [0; 4]), |consts| {
        (float4_bits(consts.setup), float4_bits(consts.apply))
    });
    sources.set_constant_rows(lighting_iw4::CONST_SRC_CODE_GLOW_SETUP, &[setup]);
    sources.set_constant_rows(lighting_iw4::CONST_SRC_CODE_GLOW_APPLY, &[apply]);
}
