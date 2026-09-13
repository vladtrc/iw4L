use crate::stretch_pic_cmd::{GFX_RENDER_CMD_TAIL_RESERVE, r_convert_color_to_bytes};
use crate::text_fx::{FX_DECODE_RENDERFLAGS, TextPulseFx};

pub const GFX_CMD_DRAW_TEXT_2D: u16 = 0x10;

pub const GFX_CMD_DRAW_TEXT_TEXT: usize = 0x50;

pub const GFX_CMD_DRAW_TEXT_X: usize = 4;

pub const GFX_CMD_DRAW_TEXT_Y: usize = 8;

pub const GFX_CMD_DRAW_TEXT_ROTATION: usize = 12;

pub const GFX_CMD_DRAW_TEXT_FONT: usize = 16;

pub const GFX_CMD_DRAW_TEXT_XSCALE: usize = 20;

pub const GFX_CMD_DRAW_TEXT_YSCALE: usize = 24;

pub const GFX_CMD_DRAW_TEXT_COLOR: usize = 28;

pub const GFX_CMD_DRAW_TEXT_MAXCHARS: usize = 32;

pub const GFX_CMD_DRAW_TEXT_FLAGS: usize = 36;

pub const GFX_CMD_DRAW_TEXT_FX_BIRTH_TIME: usize = 0x34;

pub const GFX_CMD_DRAW_TEXT_FX_LETTER_TIME: usize = 0x38;

pub const GFX_CMD_DRAW_TEXT_FX_DECAY_START: usize = 0x3c;

pub const GFX_CMD_DRAW_TEXT_FX_DECAY_DURATION: usize = 0x40;

pub const GFX_CMD_DRAW_TEXT_FX_MATERIAL: usize = 0x44;

pub const GFX_CMD_DRAW_TEXT_FX_MATERIAL_GLOW: usize = 0x48;

pub const GFX_CMD_DRAW_TEXT_PADDING: usize = 0x4c;

#[must_use]
pub fn gfx_cmd_draw_text_size(text_len: usize) -> u32 {
    (text_len as u32).saturating_add(0x54) & !3
}

#[must_use]
pub fn r_draw_text_render_flags(style: i32) -> u32 {
    if style == 3 {
        4
    } else if style == 6 {
        0xc
    } else if style == 0x80 {
        1
    } else if style == 0x84 {
        5
    } else if style == 7 {
        0x400
    } else if style == 8 {
        0xc00
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxCmdDrawTextFx {
    pub fx: TextPulseFx,
    pub fx_material: u32,
    pub fx_material_glow: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxCmdDrawText2DArgs<'a> {
    pub text: &'a str,
    pub max_chars: i32,

    pub font: u32,
    pub x: f32,
    pub y: f32,
    pub x_scale: f32,
    pub y_scale: f32,
    pub rotation: f32,
    pub color: [f32; 4],
    pub style: i32,

    pub fx: Option<GfxCmdDrawTextFx>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxCmdDrawText2D<'a> {
    pub x: f32,
    pub y: f32,
    pub rotation: f32,
    pub font: u32,
    pub x_scale: f32,
    pub y_scale: f32,
    pub color_bgra: [u8; 4],
    pub max_chars: i32,
    pub render_flags: u32,

    pub fx: Option<GfxCmdDrawTextFx>,
    pub text: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddDrawTextCmd {
    Wrote { used: u32 },
    Refused,
    Empty,
}

#[must_use]
pub fn r_add_cmd_draw_text(
    buf: &mut [u8],
    used: u32,
    cap: u32,
    args: GfxCmdDrawText2DArgs<'_>,
    color_ptr_null: bool,
) -> AddDrawTextCmd {
    if args.text.is_empty() {
        return AddDrawTextCmd::Empty;
    }
    let bytes = args.text.as_bytes();
    let size = gfx_cmd_draw_text_size(bytes.len());
    if size > u16::MAX as u32 {
        return AddDrawTextCmd::Refused;
    }
    if used
        .saturating_add(size)
        .saturating_add(GFX_RENDER_CMD_TAIL_RESERVE)
        > cap
        || (used as usize).saturating_add(size as usize) > buf.len()
        || (used as usize).saturating_add(GFX_CMD_DRAW_TEXT_TEXT + bytes.len() + 1) > buf.len()
    {
        return AddDrawTextCmd::Refused;
    }
    let start = used as usize;
    let end = start + size as usize;
    for b in &mut buf[start..end] {
        *b = 0;
    }
    write_u16(buf, start, GFX_CMD_DRAW_TEXT_2D);
    write_u16(buf, start + 2, size as u16);
    write_f32(buf, start + GFX_CMD_DRAW_TEXT_X, args.x);
    write_f32(buf, start + GFX_CMD_DRAW_TEXT_Y, args.y);
    write_f32(buf, start + GFX_CMD_DRAW_TEXT_ROTATION, args.rotation);
    write_u32(buf, start + GFX_CMD_DRAW_TEXT_FONT, args.font);
    write_f32(buf, start + GFX_CMD_DRAW_TEXT_XSCALE, args.x_scale);
    write_f32(buf, start + GFX_CMD_DRAW_TEXT_YSCALE, args.y_scale);
    let packed = if color_ptr_null {
        [0xff, 0xff, 0xff, 0xff]
    } else {
        r_convert_color_to_bytes(args.color)
    };
    buf[start + GFX_CMD_DRAW_TEXT_COLOR..start + GFX_CMD_DRAW_TEXT_COLOR + 4]
        .copy_from_slice(&packed);
    write_i32(buf, start + GFX_CMD_DRAW_TEXT_MAXCHARS, args.max_chars);
    let mut flags = r_draw_text_render_flags(args.style);
    if let Some(fx) = args.fx.filter(|fx| fx.fx.birth_time != 0) {
        flags |= FX_DECODE_RENDERFLAGS;
        write_i32(
            buf,
            start + GFX_CMD_DRAW_TEXT_FX_BIRTH_TIME,
            fx.fx.birth_time,
        );
        write_i32(
            buf,
            start + GFX_CMD_DRAW_TEXT_FX_LETTER_TIME,
            fx.fx.letter_time,
        );
        write_i32(
            buf,
            start + GFX_CMD_DRAW_TEXT_FX_DECAY_START,
            fx.fx.decay_start_time,
        );
        write_i32(
            buf,
            start + GFX_CMD_DRAW_TEXT_FX_DECAY_DURATION,
            fx.fx.decay_duration,
        );
        write_u32(buf, start + GFX_CMD_DRAW_TEXT_FX_MATERIAL, fx.fx_material);
        write_u32(
            buf,
            start + GFX_CMD_DRAW_TEXT_FX_MATERIAL_GLOW,
            fx.fx_material_glow,
        );
        write_u32(buf, start + GFX_CMD_DRAW_TEXT_PADDING, 0);
    }
    write_u32(buf, start + GFX_CMD_DRAW_TEXT_FLAGS, flags);
    let text_at = start + GFX_CMD_DRAW_TEXT_TEXT;
    buf[text_at..text_at + bytes.len()].copy_from_slice(bytes);
    buf[text_at + bytes.len()] = 0;
    AddDrawTextCmd::Wrote { used: used + size }
}

#[must_use]
pub fn parse_gfx_cmd_draw_text(cmd: &[u8]) -> Option<GfxCmdDrawText2D<'_>> {
    if cmd.len() < GFX_CMD_DRAW_TEXT_TEXT + 1 {
        return None;
    }
    let ty = read_u16(cmd, 0)?;
    let size = read_u16(cmd, 2)? as usize;
    if ty != GFX_CMD_DRAW_TEXT_2D || size < GFX_CMD_DRAW_TEXT_TEXT + 1 || cmd.len() < size {
        return None;
    }
    let text_bytes = &cmd[GFX_CMD_DRAW_TEXT_TEXT..size];
    let n = text_bytes.iter().position(|b| *b == 0)?;
    let text = core::str::from_utf8(&text_bytes[..n]).ok()?;
    let render_flags = read_u32(cmd, GFX_CMD_DRAW_TEXT_FLAGS)?;
    let fx = if render_flags & crate::text_fx::TEXT_RENDERFLAG_FX_DECODE == 0 {
        None
    } else {
        Some(GfxCmdDrawTextFx {
            fx: TextPulseFx {
                birth_time: read_i32(cmd, GFX_CMD_DRAW_TEXT_FX_BIRTH_TIME)?,
                letter_time: read_i32(cmd, GFX_CMD_DRAW_TEXT_FX_LETTER_TIME)?,
                decay_start_time: read_i32(cmd, GFX_CMD_DRAW_TEXT_FX_DECAY_START)?,
                decay_duration: read_i32(cmd, GFX_CMD_DRAW_TEXT_FX_DECAY_DURATION)?,
            },
            fx_material: read_u32(cmd, GFX_CMD_DRAW_TEXT_FX_MATERIAL)?,
            fx_material_glow: read_u32(cmd, GFX_CMD_DRAW_TEXT_FX_MATERIAL_GLOW)?,
        })
    };
    Some(GfxCmdDrawText2D {
        x: read_f32(cmd, GFX_CMD_DRAW_TEXT_X)?,
        y: read_f32(cmd, GFX_CMD_DRAW_TEXT_Y)?,
        rotation: read_f32(cmd, GFX_CMD_DRAW_TEXT_ROTATION)?,
        font: read_u32(cmd, GFX_CMD_DRAW_TEXT_FONT)?,
        x_scale: read_f32(cmd, GFX_CMD_DRAW_TEXT_XSCALE)?,
        y_scale: read_f32(cmd, GFX_CMD_DRAW_TEXT_YSCALE)?,
        color_bgra: [
            *cmd.get(GFX_CMD_DRAW_TEXT_COLOR)?,
            *cmd.get(GFX_CMD_DRAW_TEXT_COLOR + 1)?,
            *cmd.get(GFX_CMD_DRAW_TEXT_COLOR + 2)?,
            *cmd.get(GFX_CMD_DRAW_TEXT_COLOR + 3)?,
        ],
        max_chars: read_i32(cmd, GFX_CMD_DRAW_TEXT_MAXCHARS)?,
        render_flags,
        fx,
        text,
    })
}

fn write_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_i32(buf: &mut [u8], off: usize, v: i32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn write_f32(buf: &mut [u8], off: usize, v: f32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn read_u16(buf: &[u8], off: usize) -> Option<u16> {
    let b = buf.get(off..off + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

fn read_u32(buf: &[u8], off: usize) -> Option<u32> {
    let b = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_i32(buf: &[u8], off: usize) -> Option<i32> {
    let b = buf.get(off..off + 4)?;
    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_f32(buf: &[u8], off: usize) -> Option<f32> {
    let b = buf.get(off..off + 4)?;
    Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
