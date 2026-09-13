use crate::stretch_pic_cmd::{
    AddStretchPicCmd, GFX_RENDER_CMD_BUF_SIZE, GfxCmdStretchPicArgs, r_add_cmd_draw_stretch_pic,
};
use crate::{AddDrawTextCmd, GfxCmdDrawText2DArgs, r_add_cmd_draw_text};

#[derive(Debug)]
pub struct GfxRenderCommandBuf<'a> {
    pub buf: &'a mut [u8],

    pub used: u32,

    pub cap: u32,

    pub last: u32,
}

impl<'a> GfxRenderCommandBuf<'a> {
    #[must_use]
    pub fn new(buf: &'a mut [u8]) -> Self {
        let cap = core::cmp::min(buf.len() as u32, GFX_RENDER_CMD_BUF_SIZE);
        Self {
            buf,
            used: 0,
            cap,
            last: 0,
        }
    }

    pub fn add_stretch_pic(
        &mut self,
        args: GfxCmdStretchPicArgs,
        color_ptr_null: bool,
    ) -> AddStretchPicCmd {
        match r_add_cmd_draw_stretch_pic(self.buf, self.used, self.cap, args, color_ptr_null) {
            AddStretchPicCmd::Refused => {
                self.last = 0;
                AddStretchPicCmd::Refused
            }
            AddStretchPicCmd::Wrote { used } => {
                self.last = self.used;
                self.used = used;
                AddStretchPicCmd::Wrote { used }
            }
        }
    }

    pub fn add_draw_text(
        &mut self,
        args: GfxCmdDrawText2DArgs<'_>,
        color_ptr_null: bool,
    ) -> AddDrawTextCmd {
        match r_add_cmd_draw_text(self.buf, self.used, self.cap, args, color_ptr_null) {
            AddDrawTextCmd::Refused => {
                self.last = 0;
                AddDrawTextCmd::Refused
            }
            other => {
                if let AddDrawTextCmd::Wrote { used } = other {
                    self.last = self.used;
                    self.used = used;
                }
                other
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxCmdView<'a> {
    pub ty: u16,
    pub size: u16,
    pub offset: u32,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxCmdWalkRefuse {
    TruncatedHeader { offset: u32 },
    SizeBelowHeader { offset: u32, size: u16 },
    SlicePastUsed { offset: u32, size: u16 },
}

pub struct GfxCmdWalk<'a> {
    buf: &'a [u8],
    used: u32,
    off: u32,
}

impl<'a> GfxCmdWalk<'a> {
    #[must_use]
    pub fn new(buf: &'a [u8], used: u32) -> Self {
        let used = core::cmp::min(used, buf.len() as u32);
        Self { buf, used, off: 0 }
    }
}

impl<'a> Iterator for GfxCmdWalk<'a> {
    type Item = Result<GfxCmdView<'a>, GfxCmdWalkRefuse>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.off >= self.used {
            return None;
        }
        let start = self.off as usize;
        if self.used.saturating_sub(self.off) < 4 {
            let offset = self.off;
            self.off = self.used;
            return Some(Err(GfxCmdWalkRefuse::TruncatedHeader { offset }));
        }
        let ty = u16::from_le_bytes([self.buf[start], self.buf[start + 1]]);
        let size = u16::from_le_bytes([self.buf[start + 2], self.buf[start + 3]]);
        if size < 4 {
            let offset = self.off;
            self.off = self.used;
            return Some(Err(GfxCmdWalkRefuse::SizeBelowHeader { offset, size }));
        }
        let end = self.off.saturating_add(u32::from(size));
        if end > self.used || end as usize > self.buf.len() {
            let offset = self.off;
            self.off = self.used;
            return Some(Err(GfxCmdWalkRefuse::SlicePastUsed { offset, size }));
        }
        let bytes = &self.buf[start..end as usize];
        let view = GfxCmdView {
            ty,
            size,
            offset: self.off,
            bytes,
        };
        self.off = end;
        Some(Ok(view))
    }
}

#[must_use]
pub fn r_walk_render_commands(buf: &[u8], used: u32) -> GfxCmdWalk<'_> {
    GfxCmdWalk::new(buf, used)
}
