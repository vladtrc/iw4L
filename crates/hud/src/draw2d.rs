use core::fmt::{self, Write as _};
use std::collections::HashMap;

use assets::FontDef;

#[derive(Clone, Debug, PartialEq)]
pub enum Draw2dOp {
    StretchPic,
    RotateSt {
        center_s: f32,
        center_t: f32,
        radius_st: f32,
        scale_final_s: f32,
        scale_final_t: f32,
        deg: f32,
    },
    ClipRect,
    TextRun {
        font: String,
        scale: f32,
        text: String,
        loc_key: String,

        style: i32,

        fx: Option<TextRunFx>,
    },
}

pub const TEXT_STYLE_HUDELEM: i32 = 3;

pub const TEXT_STYLE_UNREAD: i32 = -1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextRunFx {
    pub scene_time: i32,
    pub fx: hud_iw4::TextPulseFx,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Draw2dProvenance {
    Objective,
    MapUseHint,
    MenuItem { menu: String, index: usize },
    OwnerDraw(i32),
    CgDraw { site: &'static str },
    HudElem { index: i32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draw2dCmd {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
    pub color: [f32; 4],
    pub material: String,

    pub material_namespace: assets::AssetNamespace,
    pub op: Draw2dOp,
    pub provenance: Draw2dProvenance,

    pub layer: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draw2dList {
    pub cmds: Vec<Draw2dCmd>,
}

impl Draw2dList {
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    pub fn snapshot(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "draw2d n={}", self.cmds.len());
        for (i, cmd) in self.cmds.iter().enumerate() {
            let _ = writeln!(
                out,
                "  [{i}] {} layer={} xywh=({:.4},{:.4},{:.4},{:.4}) uv=({:.4},{:.4},{:.4},{:.4}) color=[{:.3},{:.3},{:.3},{:.3}] mat={:?}:{:?} via={}{}",
                op_name(&cmd.op),
                cmd.layer,
                cmd.x,
                cmd.y,
                cmd.w,
                cmd.h,
                cmd.s0,
                cmd.t0,
                cmd.s1,
                cmd.t1,
                cmd.color[0],
                cmd.color[1],
                cmd.color[2],
                cmd.color[3],
                cmd.material_namespace,
                cmd.material,
                provenance_name(&cmd.provenance),
                op_extra(&cmd.op),
            );
        }
        out
    }
}

fn op_name(op: &Draw2dOp) -> &'static str {
    match op {
        Draw2dOp::StretchPic => "StretchPic",
        Draw2dOp::RotateSt { .. } => "RotateST",
        Draw2dOp::ClipRect => "ClipRect",
        Draw2dOp::TextRun { .. } => "TextRun",
    }
}

fn op_extra(op: &Draw2dOp) -> String {
    match op {
        Draw2dOp::RotateSt {
            center_s,
            center_t,
            radius_st,
            scale_final_s,
            scale_final_t,
            deg,
        } => format!(
            " st=(c={center_s:.4},{center_t:.4} r={radius_st:.4} sf={scale_final_s:.4},{scale_final_t:.4} deg={deg:.3})"
        ),
        Draw2dOp::TextRun {
            font,
            scale,
            text,
            loc_key,
            style,
            fx,
        } => {
            let fx = match fx {
                Some(fx) => format!(
                    " fx=(t={} birth={} letter={} decay={}+{})",
                    fx.scene_time,
                    fx.fx.birth_time,
                    fx.fx.letter_time,
                    fx.fx.decay_start_time,
                    fx.fx.decay_duration
                ),
                None => String::new(),
            };
            let flags = hud_iw4::r_draw_text_render_flags(*style);
            format!(
                " font={font:?} scale={scale:.4} text={text:?} loc={loc_key:?} style={style} flags={flags:#x}{fx}"
            )
        }
        Draw2dOp::StretchPic | Draw2dOp::ClipRect => String::new(),
    }
}

fn provenance_name(p: &Draw2dProvenance) -> String {
    match p {
        Draw2dProvenance::Objective => "objective".into(),
        Draw2dProvenance::MapUseHint => "map-use-hint".into(),
        Draw2dProvenance::MenuItem { menu, index } => format!("menu:{menu}[{index}]"),
        Draw2dProvenance::OwnerDraw(id) => format!("ownerDraw:{id}"),
        Draw2dProvenance::CgDraw { site } => format!("cg:{site}"),
        Draw2dProvenance::HudElem { index } => format!("hudelem:{index}"),
    }
}

impl fmt::Display for Draw2dList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.snapshot())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draw2dQuad {
    pub xy: [[f32; 2]; 4],
    pub st: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub material: String,

    pub material_namespace: assets::AssetNamespace,
    pub provenance: Draw2dProvenance,
    pub layer: u8,

    pub clip: Option<[f32; 4]>,
}

pub const DRAW2D_QUAD_INDICES: [u16; 6] = hud_iw4::RB_DRAW_STRETCHPIC_INDICES;

pub fn rotate_st_corners(
    center_s: f32,
    center_t: f32,
    radius_st: f32,
    scale_final_s: f32,
    scale_final_t: f32,
    deg: f32,
) -> [[f32; 2]; 4] {
    let rad = deg * (core::f32::consts::PI / 180.0);
    let cos = rad.cos();
    let sin = rad.sin();
    let step_s = [
        radius_st * cos * scale_final_s,
        radius_st * sin * scale_final_t,
    ];
    let step_t = [
        -radius_st * sin * scale_final_s,
        radius_st * cos * scale_final_t,
    ];
    [
        [
            center_s - step_s[0] - step_t[0],
            center_t - step_s[1] - step_t[1],
        ],
        [
            center_s + step_s[0] - step_t[0],
            center_t + step_s[1] - step_t[1],
        ],
        [
            center_s + step_s[0] + step_t[0],
            center_t + step_s[1] + step_t[1],
        ],
        [
            center_s - step_s[0] + step_t[0],
            center_t - step_s[1] + step_t[1],
        ],
    ]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Draw2dCmdCensus {
    pub used: u32,
    pub walk_n: u32,
    pub type8_n: u32,
    pub type10_n: u32,
    pub skip_n: u32,
    pub overflow_n: u32,
}

enum CmdSlot {
    Buf,
    Rotate(Draw2dQuad),
}

struct CmdHost {
    material_namespace: assets::AssetNamespace,
    clip: Option<[f32; 4]>,
    provenance: Draw2dProvenance,
    layer: u8,
    glyph_material: Option<String>,

    scene_time: Option<i32>,
}

pub fn tessellate(list: &Draw2dList) -> (Vec<Draw2dQuad>, Draw2dCmdCensus) {
    tessellate_fonts(list, &HashMap::new())
}

pub fn tessellate_fonts(
    list: &Draw2dList,
    fonts: &HashMap<String, &FontDef>,
) -> (Vec<Draw2dQuad>, Draw2dCmdCensus) {
    let mut clip: Option<[f32; 4]> = None;
    let mut raw = vec![0u8; hud_iw4::GFX_RENDER_CMD_BUF_SIZE as usize];
    let mut rc = hud_iw4::GfxRenderCommandBuf::new(&mut raw);
    let mut mats: Vec<String> = Vec::new();
    let mut hosts: Vec<CmdHost> = Vec::new();
    let mut slots: Vec<CmdSlot> = Vec::new();
    let mut overflow_n = 0u32;
    for cmd in &list.cmds {
        match &cmd.op {
            Draw2dOp::ClipRect => {
                let next = [cmd.x, cmd.y, cmd.x + cmd.w, cmd.y + cmd.h];
                clip = Some(match clip {
                    Some(c) => [
                        c[0].max(next[0]),
                        c[1].max(next[1]),
                        c[2].min(next[2]),
                        c[3].min(next[3]),
                    ],
                    None => next,
                });
            }
            Draw2dOp::TextRun {
                font,
                text,
                fx,
                style,
                ..
            } => {
                if fonts.get(font).is_none() {
                    continue;
                }
                let font_tok = intern_name(&mut mats, font);

                let cmd_fx = fx.map(|fx| hud_iw4::GfxCmdDrawTextFx {
                    fx: fx.fx,
                    fx_material: intern_name(&mut mats, hud_iw4::DECODE_CHARACTERS_MATERIAL),
                    fx_material_glow: intern_name(
                        &mut mats,
                        hud_iw4::DECODE_CHARACTERS_GLOW_MATERIAL,
                    ),
                });
                match rc.add_draw_text(
                    hud_iw4::GfxCmdDrawText2DArgs {
                        text,
                        max_chars: 0x7fff_ffff,
                        font: font_tok,
                        x: cmd.x,
                        y: cmd.y,
                        x_scale: cmd.w,
                        y_scale: cmd.h,
                        rotation: 0.0,
                        color: cmd.color,
                        style: *style,
                        fx: cmd_fx,
                    },
                    false,
                ) {
                    hud_iw4::AddDrawTextCmd::Wrote { .. } => {
                        hosts.push(CmdHost {
                            material_namespace: cmd.material_namespace,
                            clip,
                            provenance: cmd.provenance.clone(),
                            layer: cmd.layer,
                            glyph_material: Some(cmd.material.clone()),
                            scene_time: fx.map(|fx| fx.scene_time),
                        });
                        slots.push(CmdSlot::Buf);
                    }
                    hud_iw4::AddDrawTextCmd::Refused => overflow_n += 1,
                    hud_iw4::AddDrawTextCmd::Empty => {}
                }
            }
            Draw2dOp::StretchPic => {
                let material = intern_name(&mut mats, &cmd.material);
                match rc.add_stretch_pic(
                    hud_iw4::GfxCmdStretchPicArgs {
                        material,
                        x: cmd.x,
                        y: cmd.y,
                        w: cmd.w,
                        h: cmd.h,
                        s0: cmd.s0,
                        t0: cmd.t0,
                        s1: cmd.s1,
                        t1: cmd.t1,
                        color: cmd.color,
                    },
                    false,
                ) {
                    hud_iw4::AddStretchPicCmd::Wrote { .. } => {
                        hosts.push(CmdHost {
                            material_namespace: cmd.material_namespace,
                            clip,
                            provenance: cmd.provenance.clone(),
                            layer: cmd.layer,
                            glyph_material: None,
                            scene_time: None,
                        });
                        slots.push(CmdSlot::Buf);
                    }
                    hud_iw4::AddStretchPicCmd::Refused => overflow_n += 1,
                }
            }
            Draw2dOp::RotateSt { .. } => {
                slots.push(CmdSlot::Rotate(quad_from_cmd(cmd, clip)));
            }
        }
    }
    let mut census = Draw2dCmdCensus {
        used: rc.used,
        overflow_n,
        ..Draw2dCmdCensus::default()
    };
    let mut walk = hud_iw4::r_walk_render_commands(rc.buf, rc.used);
    let mut host_i = 0usize;
    let mut out = Vec::new();
    for slot in slots {
        match slot {
            CmdSlot::Rotate(quad) => out.push(quad),
            CmdSlot::Buf => {
                let Some(view) = walk.next() else {
                    census.skip_n += 1;
                    break;
                };
                let view = match view {
                    Ok(v) => v,
                    Err(_) => {
                        census.skip_n += 1;
                        break;
                    }
                };
                census.walk_n += 1;
                let host = &hosts[host_i];
                host_i += 1;
                match view.ty {
                    hud_iw4::GFX_CMD_DRAW_STRETCHPIC => {
                        census.type8_n += 1;
                        let Some(parsed) = hud_iw4::parse_gfx_cmd_stretch_pic(view.bytes) else {
                            census.skip_n += 1;
                            continue;
                        };
                        let Some(material) = mats.get(parsed.material as usize).cloned() else {
                            census.skip_n += 1;
                            continue;
                        };
                        out.push(quad_from_stretch(
                            parsed,
                            host.clip,
                            material,
                            host.material_namespace,
                            host.provenance.clone(),
                            host.layer,
                        ));
                    }
                    hud_iw4::GFX_CMD_DRAW_TEXT_2D => {
                        census.type10_n += 1;
                        let Some(parsed) = hud_iw4::parse_gfx_cmd_draw_text(view.bytes) else {
                            census.skip_n += 1;
                            continue;
                        };
                        let Some(font_name) = mats.get(parsed.font as usize) else {
                            census.skip_n += 1;
                            continue;
                        };
                        let Some(def) = fonts.get(font_name) else {
                            continue;
                        };
                        let Some(material) = host.glyph_material.clone() else {
                            census.skip_n += 1;
                            continue;
                        };
                        out.extend(text_run_quads(
                            def,
                            parsed.text,
                            parsed.x,
                            parsed.y,
                            parsed.x_scale,
                            parsed.y_scale,
                            hud_iw4::unpack_color_bgra(parsed.color_bgra),
                            host.clip,
                            &material,
                            host.material_namespace,
                            host.provenance.clone(),
                            host.layer,
                            parsed.render_flags,
                            parsed
                                .fx
                                .zip(host.scene_time)
                                .map(|(fx, scene_time)| TextRunFx {
                                    scene_time,
                                    fx: fx.fx,
                                }),
                        ));
                    }
                    _ => census.skip_n += 1,
                }
            }
        }
    }
    (out, census)
}

fn quad_from_stretch(
    parsed: hud_iw4::GfxCmdStretchPic,
    clip: Option<[f32; 4]>,
    material: String,
    material_namespace: assets::AssetNamespace,
    provenance: Draw2dProvenance,
    layer: u8,
) -> Draw2dQuad {
    let (xy, st) = hud_iw4::rb_draw_stretch_pic_corners(
        parsed.x, parsed.y, parsed.w, parsed.h, parsed.s0, parsed.t0, parsed.s1, parsed.t1,
    );
    Draw2dQuad {
        xy,
        st,
        color: hud_iw4::unpack_color_bgra(parsed.color_bgra),
        material,
        material_namespace,
        provenance,
        layer,
        clip,
    }
}

#[allow(clippy::too_many_arguments)]
fn text_run_quads(
    font: &FontDef,
    text: &str,
    x: f32,
    y: f32,
    x_scale: f32,
    y_scale: f32,
    run_color: [f32; 4],
    clip: Option<[f32; 4]>,
    material: &str,
    material_namespace: assets::AssetNamespace,
    provenance: Draw2dProvenance,
    layer: u8,
    render_flags: u32,
    fx: Option<TextRunFx>,
) -> Vec<Draw2dQuad> {
    let mut max_length_remaining = i32::MAX;
    let mut vars = None;
    if let Some(fx) = fx {
        match hud_iw4::setup_pulse_fx_vars(
            hud_iw4::seh_print_strlen(text),
            max_length_remaining,
            fx.scene_time,
            fx.fx,
        ) {
            None => return Vec::new(),
            Some(v) => {
                max_length_remaining = v.max_length;
                vars = Some(v);
            }
        }
    }
    let mut seed = vars.map_or(1, |v| v.rand_seed);
    let draw_rand_char_at_end = vars.is_some_and(|v| v.draw_rand_char_at_end);
    let decay_time_elapsed = vars.map_or(0, |v| v.decay_time_elapsed);
    let fx_birth_time = fx.map_or(0, |f| f.fx.birth_time);
    let fx_decay_duration = fx.map_or(0, |f| f.fx.decay_duration);

    let decaying = vars.is_some_and(|v| v.decaying)
        && hud_iw4::fx_decay_tick_count(fx_decay_duration).is_some();

    let shadow_offset = hud_iw4::text_drop_shadow_offset(render_flags);
    let shadow_color = [0.0, 0.0, 0.0, run_color[3]];

    let mut cursor_x = x;
    let mut color = run_color;
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if max_length_remaining == 0 {
            break;
        }

        if c == '^'
            && let Some(n) = chars.peek().copied()
            && n.is_ascii_digit()
        {
            chars.next();
            let mut tint = hud_iw4::color_from_caret_digit(n);
            tint[3] *= run_color[3];
            color = tint;
            continue;
        }
        let orig_letter = c as u32;
        let mut letter = orig_letter;
        let mut fade_alpha = 0u8;
        let mut skip_drawing = false;
        let mut extra_fx_char = false;

        if draw_rand_char_at_end && max_length_remaining == 1 {
            letter = hud_iw4::r_font_get_random_letter(seed);
            fade_alpha = hud_iw4::FX_TYPING_LETTER_ALPHA;
            if hud_iw4::rand_with_seed(&mut seed) % 2 != 0 {
                letter = hud_iw4::FX_EXTRA_CHAR_LETTER;
                extra_fx_char = true;
            }
        } else if decaying
            && let Some(info) = hud_iw4::get_decaying_letter_info(
                letter,
                &mut seed,
                decay_time_elapsed,
                fx_birth_time,
                fx_decay_duration,
                (color[3] * 255.0) as u8,
            )
        {
            skip_drawing = info.skip_drawing;
            fade_alpha = info.alpha;
            letter = info.letter;
            extra_fx_char = info.draw_extra_fx_char;
        }

        let extra_index = extra_fx_char.then(|| {
            let mut temp = seed;
            hud_iw4::rand_with_seed(&mut temp)
        });

        let Some(glyph) = font.glyph(letter) else {
            continue;
        };

        let (decay_offset, advance) = if letter == orig_letter {
            (0.0, glyph.dx)
        } else {
            match font.glyph(orig_letter) {
                Some(orig) => (
                    f32::from(orig.pixel_width) * 0.5 - f32::from(glyph.pixel_width) * 0.5,
                    orig.dx,
                ),
                None => (0.0, glyph.dx),
            }
        };
        let mut letter_color = color;
        if decaying || (draw_rand_char_at_end && max_length_remaining == 1) {
            letter_color[3] = f32::from(hud_iw4::modulate_byte_colors(
                (color[3] * 255.0) as u8,
                fade_alpha,
            )) / 255.0;
        }
        if !skip_drawing {
            let gx = cursor_x + (f32::from(glyph.x0) + decay_offset) * x_scale;
            let gy = y + f32::from(glyph.y0) * y_scale;
            let gw = f32::from(glyph.pixel_width) * x_scale;
            let gh = f32::from(glyph.pixel_height) * y_scale;
            let (quad_material, s0, t0, s1, t1) = match extra_index {
                Some(index) => {
                    let (s0, s1) = hud_iw4::decode_fx_char_st(index);
                    (
                        hud_iw4::DECODE_CHARACTERS_MATERIAL.to_owned(),
                        s0,
                        0.0,
                        s1,
                        1.0,
                    )
                }
                None => (material.to_owned(), glyph.s0, glyph.t0, glyph.s1, glyph.t1),
            };
            if let Some(off) = shadow_offset {
                let (xy, st) = hud_iw4::rb_draw_stretch_pic_corners(
                    gx + off,
                    gy + off,
                    gw,
                    gh,
                    s0,
                    t0,
                    s1,
                    t1,
                );
                out.push(Draw2dQuad {
                    xy,
                    st,

                    color: if extra_index.is_some() {
                        letter_color
                    } else {
                        shadow_color
                    },
                    material: quad_material.clone(),
                    material_namespace,
                    provenance: provenance.clone(),
                    layer,
                    clip,
                });
            }
            let (xy, st) = hud_iw4::rb_draw_stretch_pic_corners(gx, gy, gw, gh, s0, t0, s1, t1);
            out.push(Draw2dQuad {
                xy,
                st,
                color: letter_color,
                material: quad_material,
                material_namespace,
                provenance: provenance.clone(),
                layer,
                clip,
            });
        }
        cursor_x += f32::from(advance) * x_scale;
        max_length_remaining = max_length_remaining.saturating_sub(1);
    }
    out
}

fn quad_from_cmd(cmd: &Draw2dCmd, clip: Option<[f32; 4]>) -> Draw2dQuad {
    let xy = [
        [cmd.x, cmd.y],
        [cmd.x + cmd.w, cmd.y],
        [cmd.x + cmd.w, cmd.y + cmd.h],
        [cmd.x, cmd.y + cmd.h],
    ];
    let st = match &cmd.op {
        Draw2dOp::RotateSt {
            center_s,
            center_t,
            radius_st,
            scale_final_s,
            scale_final_t,
            deg,
        } => rotate_st_corners(
            *center_s,
            *center_t,
            *radius_st,
            *scale_final_s,
            *scale_final_t,
            *deg,
        ),
        Draw2dOp::StretchPic | Draw2dOp::ClipRect | Draw2dOp::TextRun { .. } => [
            [cmd.s0, cmd.t0],
            [cmd.s1, cmd.t0],
            [cmd.s1, cmd.t1],
            [cmd.s0, cmd.t1],
        ],
    };
    Draw2dQuad {
        xy,
        st,
        color: cmd.color,
        material: cmd.material.clone(),
        material_namespace: cmd.material_namespace,
        provenance: cmd.provenance.clone(),
        layer: cmd.layer,
        clip,
    }
}

fn intern_name(names: &mut Vec<String>, name: &str) -> u32 {
    for (i, existing) in names.iter().enumerate() {
        if existing == name {
            return i as u32;
        }
    }
    let i = names.len() as u32;
    names.push(name.to_string());
    i
}
