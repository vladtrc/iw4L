pub const GFX_CMD_DRAW_STRETCHPIC: u16 = 8;

pub const GFX_CMD_DRAW_STRETCHPIC_SIZE: u16 = 0x2c;

pub const GFX_RENDER_CMD_TAIL_RESERVE: u32 = 0x2000;

pub const GFX_RENDER_CMD_BUF_SIZE: u32 = 0x18000;

pub const GFX_CMD_STRETCHPIC_MATERIAL: usize = 4;

pub const GFX_CMD_STRETCHPIC_X: usize = 8;

pub const GFX_CMD_STRETCHPIC_Y: usize = 12;

pub const GFX_CMD_STRETCHPIC_W: usize = 16;

pub const GFX_CMD_STRETCHPIC_H: usize = 20;

pub const GFX_CMD_STRETCHPIC_S0: usize = 24;

pub const GFX_CMD_STRETCHPIC_T0: usize = 28;

pub const GFX_CMD_STRETCHPIC_S1: usize = 32;

pub const GFX_CMD_STRETCHPIC_T1: usize = 36;

pub const GFX_CMD_STRETCHPIC_COLOR: usize = 40;

pub const COLOR_TO_BYTE_SCALE: f32 = 255.0;

pub const COLOR_TO_BYTE_BIAS: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxCmdStretchPicArgs {
    pub material: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxCmdStretchPic {
    pub material: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub s0: f32,
    pub t0: f32,
    pub s1: f32,
    pub t1: f32,
    pub color_bgra: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddStretchPicCmd {
    Wrote { used: u32 },
    Refused,
}

#[must_use]
pub fn unpack_color_bgra(bgra: [u8; 4]) -> [f32; 4] {
    [
        bgra[2] as f32 / COLOR_TO_BYTE_SCALE,
        bgra[1] as f32 / COLOR_TO_BYTE_SCALE,
        bgra[0] as f32 / COLOR_TO_BYTE_SCALE,
        bgra[3] as f32 / COLOR_TO_BYTE_SCALE,
    ]
}

#[must_use]
pub fn r_convert_color_to_bytes(color: [f32; 4]) -> [u8; 4] {
    [
        pack_color_channel(color[2]),
        pack_color_channel(color[1]),
        pack_color_channel(color[0]),
        pack_color_channel(color[3]),
    ]
}

fn pack_color_channel(x: f32) -> u8 {
    let i = (x * COLOR_TO_BYTE_SCALE + COLOR_TO_BYTE_BIAS) as i32;
    if i >= 0xff {
        0xff
    } else if i < 1 {
        0
    } else {
        i as u8
    }
}

#[must_use]
pub fn r_add_cmd_draw_stretch_pic(
    buf: &mut [u8],
    used: u32,
    cap: u32,
    args: GfxCmdStretchPicArgs,
    color_ptr_null: bool,
) -> AddStretchPicCmd {
    let size = u32::from(GFX_CMD_DRAW_STRETCHPIC_SIZE);
    if used
        .saturating_add(size)
        .saturating_add(GFX_RENDER_CMD_TAIL_RESERVE)
        > cap
        || (used as usize).saturating_add(size as usize) > buf.len()
    {
        return AddStretchPicCmd::Refused;
    }
    let start = used as usize;
    let end = start + size as usize;
    write_u16(buf, start, GFX_CMD_DRAW_STRETCHPIC);
    write_u16(buf, start + 2, GFX_CMD_DRAW_STRETCHPIC_SIZE);
    write_u32(buf, start + GFX_CMD_STRETCHPIC_MATERIAL, args.material);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_X, args.x);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_Y, args.y);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_W, args.w);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_H, args.h);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_S0, args.s0);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_T0, args.t0);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_S1, args.s1);
    write_f32(buf, start + GFX_CMD_STRETCHPIC_T1, args.t1);
    let packed = if color_ptr_null {
        [0xff, 0xff, 0xff, 0xff]
    } else {
        r_convert_color_to_bytes(args.color)
    };
    buf[start + GFX_CMD_STRETCHPIC_COLOR..end].copy_from_slice(&packed);
    AddStretchPicCmd::Wrote { used: used + size }
}

#[must_use]
pub fn parse_gfx_cmd_stretch_pic(cmd: &[u8]) -> Option<GfxCmdStretchPic> {
    if cmd.len() < usize::from(GFX_CMD_DRAW_STRETCHPIC_SIZE) {
        return None;
    }
    let ty = read_u16(cmd, 0)?;
    let size = read_u16(cmd, 2)?;
    if ty != GFX_CMD_DRAW_STRETCHPIC || size != GFX_CMD_DRAW_STRETCHPIC_SIZE {
        return None;
    }
    Some(GfxCmdStretchPic {
        material: read_u32(cmd, GFX_CMD_STRETCHPIC_MATERIAL)?,
        x: read_f32(cmd, GFX_CMD_STRETCHPIC_X)?,
        y: read_f32(cmd, GFX_CMD_STRETCHPIC_Y)?,
        w: read_f32(cmd, GFX_CMD_STRETCHPIC_W)?,
        h: read_f32(cmd, GFX_CMD_STRETCHPIC_H)?,
        s0: read_f32(cmd, GFX_CMD_STRETCHPIC_S0)?,
        t0: read_f32(cmd, GFX_CMD_STRETCHPIC_T0)?,
        s1: read_f32(cmd, GFX_CMD_STRETCHPIC_S1)?,
        t1: read_f32(cmd, GFX_CMD_STRETCHPIC_T1)?,
        color_bgra: [
            *cmd.get(GFX_CMD_STRETCHPIC_COLOR)?,
            *cmd.get(GFX_CMD_STRETCHPIC_COLOR + 1)?,
            *cmd.get(GFX_CMD_STRETCHPIC_COLOR + 2)?,
            *cmd.get(GFX_CMD_STRETCHPIC_COLOR + 3)?,
        ],
    })
}

#[must_use]
pub fn rb_draw_stretch_pic_corners(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s0: f32,
    t0: f32,
    s1: f32,
    t1: f32,
) -> ([[f32; 2]; 4], [[f32; 2]; 4]) {
    (
        [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        [[s0, t0], [s1, t0], [s1, t1], [s0, t1]],
    )
}

pub const GFX_TESS_VERTEX_STRIDE: usize = 0x20;

pub const TESS_STRETCHPIC_FLUSH_VERT_PLUS_FOUR: u32 = 0x154a;

pub const TESS_STRETCHPIC_FLUSH_INDEX_PLUS_SIX: u32 = 0x100000;

pub const GFX_TESS_2D_PACKED_NORMAL: u32 = 0x3ffe7f7f;

pub const RB_DRAW_STRETCHPIC_INDICES: [u16; 6] = [3, 0, 2, 2, 0, 1];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxTessVertex2d {
    pub xyzw: [f32; 4],
    pub color: u32,
    pub texcoord: [f32; 2],
    pub packed_normal: u32,
}

impl GfxTessVertex2d {
    #[must_use]
    pub fn to_bytes(self) -> [u8; GFX_TESS_VERTEX_STRIDE] {
        let mut out = [0u8; GFX_TESS_VERTEX_STRIDE];
        out[0..4].copy_from_slice(&self.xyzw[0].to_le_bytes());
        out[4..8].copy_from_slice(&self.xyzw[1].to_le_bytes());
        out[8..12].copy_from_slice(&self.xyzw[2].to_le_bytes());
        out[12..16].copy_from_slice(&self.xyzw[3].to_le_bytes());
        out[16..20].copy_from_slice(&self.color.to_le_bytes());
        out[20..24].copy_from_slice(&self.texcoord[0].to_le_bytes());
        out[24..28].copy_from_slice(&self.texcoord[1].to_le_bytes());
        out[28..32].copy_from_slice(&self.packed_normal.to_le_bytes());
        out
    }
}

#[must_use]
pub fn rb_set_vertex_2d(x: f32, y: f32, s: f32, t: f32, color: u32) -> GfxTessVertex2d {
    GfxTessVertex2d {
        xyzw: [x, y, 0.0, 1.0],
        color,
        texcoord: [s, t],
        packed_normal: GFX_TESS_2D_PACKED_NORMAL,
    }
}

#[must_use]
pub fn rb_draw_stretch_pic_pack(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s0: f32,
    t0: f32,
    s1: f32,
    t1: f32,
    color: u32,
) -> [GfxTessVertex2d; 4] {
    [
        rb_set_vertex_2d(x, y, s0, t0, color),
        rb_set_vertex_2d(x + w, y, s1, t0, color),
        rb_set_vertex_2d(x + w, y + h, s1, t1, color),
        rb_set_vertex_2d(x, y + h, s0, t1, color),
    ]
}

#[must_use]
pub fn tess_stretchpic_must_flush(vertex_count: u32, index_count: u32) -> bool {
    TESS_STRETCHPIC_FLUSH_VERT_PLUS_FOUR < vertex_count.saturating_add(4)
        || TESS_STRETCHPIC_FLUSH_INDEX_PLUS_SIX < index_count.saturating_add(6)
}

fn write_u16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn write_u32(buf: &mut [u8], off: usize, v: u32) {
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

fn read_f32(buf: &[u8], off: usize) -> Option<f32> {
    let b = buf.get(off..off + 4)?;
    Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
