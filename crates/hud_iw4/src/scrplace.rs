pub const SCRPLACE_VIRTUAL_WIDTH: f32 = 640.0;

pub const SCRPLACE_VIRTUAL_HEIGHT: f32 = 480.0;

const ASPECT_4_3: f32 = 1.333_333_3;

const HALF: f32 = 0.5;

pub const SAFE_AREA_DEFAULT: f32 = 0.85;

pub const SAFE_AREA_ADJUSTED_DEFAULT: f32 = 1.0;

pub const ALIGN_SUB: i32 = 0;
pub const ALIGN_VIEWABLE: i32 = 1;
pub const ALIGN_CENTER: i32 = 2;
pub const ALIGN_VIEWABLE_MAX: i32 = 3;
pub const ALIGN_FULLSCREEN: i32 = 4;
pub const ALIGN_NOSCALE: i32 = 5;
pub const ALIGN_TO_VIRTUAL: i32 = 6;
pub const ALIGN_CENTER_SAFE: i32 = 7;
pub const ALIGN_USER_MIN: i32 = 8;
pub const ALIGN_USER_CENTER: i32 = 9;
pub const ALIGN_USER_MAX: i32 = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenPlacement {
    pub scale_virtual_to_real: [f32; 2],

    pub scale_virtual_to_full: [f32; 2],

    pub scale_real_to_virtual: [f32; 2],

    pub real_viewport_position: [f32; 2],

    pub real_viewport_size: [f32; 2],

    pub virtual_viewable_min: [f32; 2],

    pub virtual_viewable_max: [f32; 2],

    pub real_viewable_min: [f32; 2],

    pub real_viewable_max: [f32; 2],

    pub virtual_adjustable_min: [f32; 2],

    pub virtual_adjustable_max: [f32; 2],

    pub real_adjustable_min: [f32; 2],

    pub real_adjustable_max: [f32; 2],

    pub sub_screen_left: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppliedRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl ScreenPlacement {
    pub fn setup(
        viewport_x: f32,
        viewport_y: f32,
        viewport_w: f32,
        viewport_h: f32,
        display_w: f32,
        display_h: f32,
        safe_horz: f32,
        safe_vert: f32,
        adj_horz: f32,
        adj_vert: f32,
        aspect_pixel: f32,
    ) -> Self {
        if aspect_pixel == 0.0 {
            panic!("ScrPlace: display pixel aspect is 0; video init has not written it");
        }
        let mut sub_w = viewport_h * ASPECT_4_3 / aspect_pixel;
        if viewport_w < sub_w {
            sub_w = viewport_w;
        }
        let horz_aspect_scale = viewport_w / sub_w;
        let (real_viewable_min, real_viewable_max, virtual_viewable_min, virtual_viewable_max) =
            setup_viewable(
                viewport_x,
                viewport_y,
                viewport_w,
                viewport_h,
                horz_aspect_scale,
                safe_horz,
                safe_vert,
                display_w,
                display_h,
            );
        let (
            real_adjustable_min,
            real_adjustable_max,
            virtual_adjustable_min,
            virtual_adjustable_max,
        ) = setup_viewable(
            viewport_x,
            viewport_y,
            viewport_w,
            viewport_h,
            horz_aspect_scale,
            adj_horz,
            adj_vert,
            display_w,
            display_h,
        );
        Self {
            scale_virtual_to_real: [
                sub_w / SCRPLACE_VIRTUAL_WIDTH,
                viewport_h / SCRPLACE_VIRTUAL_HEIGHT,
            ],
            scale_virtual_to_full: [
                viewport_w / SCRPLACE_VIRTUAL_WIDTH,
                viewport_h / SCRPLACE_VIRTUAL_HEIGHT,
            ],
            scale_real_to_virtual: [
                SCRPLACE_VIRTUAL_WIDTH / sub_w,
                SCRPLACE_VIRTUAL_HEIGHT / viewport_h,
            ],
            real_viewport_position: [viewport_x, viewport_y],
            real_viewport_size: [viewport_w, viewport_h],
            virtual_viewable_min,
            virtual_viewable_max,
            real_viewable_min,
            real_viewable_max,
            virtual_adjustable_min,
            virtual_adjustable_max,
            real_adjustable_min,
            real_adjustable_max,
            sub_screen_left: (viewport_w - sub_w) * HALF,
        }
    }

    pub fn setup_fullscreen(width: f32, height: f32, aspect_pixel: f32) -> Self {
        Self::setup(
            0.0,
            0.0,
            width,
            height,
            width,
            height,
            SAFE_AREA_DEFAULT,
            SAFE_AREA_DEFAULT,
            SAFE_AREA_ADJUSTED_DEFAULT,
            SAFE_AREA_ADJUSTED_DEFAULT,
            aspect_pixel,
        )
    }

    pub fn apply_rect(
        &self,
        mut x: f32,
        mut y: f32,
        mut w: f32,
        mut h: f32,
        horz_align: i32,
        vert_align: i32,
    ) -> AppliedRect {
        match horz_align {
            ALIGN_VIEWABLE => {
                x = self.scale_virtual_to_real[0] * x + self.real_viewable_min[0];
                w *= self.scale_virtual_to_real[0];
            }
            ALIGN_CENTER => {
                x = self.real_viewport_size[0] * HALF + self.scale_virtual_to_real[0] * x;
                w *= self.scale_virtual_to_real[0];
            }
            ALIGN_VIEWABLE_MAX => {
                x = self.scale_virtual_to_real[0] * x + self.real_viewable_max[0];
                w *= self.scale_virtual_to_real[0];
            }
            ALIGN_FULLSCREEN => {
                x *= self.scale_virtual_to_full[0];
                w *= self.scale_virtual_to_full[0];
            }
            ALIGN_NOSCALE => {}
            ALIGN_TO_VIRTUAL => {
                x *= self.scale_real_to_virtual[0];
                w *= self.scale_real_to_virtual[0];
            }
            ALIGN_CENTER_SAFE => {
                x = self.scale_virtual_to_real[0] * x
                    + (self.real_viewable_max[0] + self.real_viewable_min[0]) * HALF;
                w *= self.scale_virtual_to_real[0];
            }
            ALIGN_USER_MIN => {
                x = self.scale_virtual_to_real[0] * x + self.real_adjustable_min[0];
                w *= self.scale_virtual_to_real[0];
            }
            ALIGN_USER_CENTER => {
                x = self.scale_virtual_to_real[0] * x
                    + (self.real_adjustable_max[0] + self.real_adjustable_min[0]) * HALF;
                w *= self.scale_virtual_to_real[0];
            }
            ALIGN_USER_MAX => {
                x = self.scale_virtual_to_real[0] * x + self.real_adjustable_max[0];
                w *= self.scale_virtual_to_real[0];
            }
            _ => {
                x = self.scale_virtual_to_real[0] * x + self.sub_screen_left;
                w *= self.scale_virtual_to_real[0];
            }
        }
        match vert_align {
            ALIGN_VIEWABLE => {
                y = self.scale_virtual_to_real[1] * y + self.real_viewable_min[1];
                h *= self.scale_virtual_to_real[1];
            }
            ALIGN_CENTER => {
                y = self.scale_virtual_to_real[1] * y + self.real_viewport_size[1] * HALF;
                h *= self.scale_virtual_to_real[1];
            }
            ALIGN_VIEWABLE_MAX => {
                y = self.scale_virtual_to_real[1] * y + self.real_viewable_max[1];
                h *= self.scale_virtual_to_real[1];
            }
            ALIGN_FULLSCREEN => {
                y *= self.scale_virtual_to_full[1];
                h *= self.scale_virtual_to_full[1];
            }
            ALIGN_NOSCALE => {}
            ALIGN_TO_VIRTUAL => {
                y *= self.scale_real_to_virtual[1];
                h *= self.scale_real_to_virtual[1];
            }
            ALIGN_CENTER_SAFE => {
                y = self.scale_virtual_to_real[1] * y
                    + (self.real_viewable_max[1] + self.real_viewable_min[1]) * HALF;
                h *= self.scale_virtual_to_real[1];
            }
            ALIGN_USER_MIN => {
                y = self.scale_virtual_to_real[1] * y + self.real_adjustable_min[1];
                h *= self.scale_virtual_to_real[1];
            }
            ALIGN_USER_CENTER => {
                y = self.scale_virtual_to_real[1] * y
                    + (self.real_adjustable_max[1] + self.real_adjustable_min[1]) * HALF;
                h *= self.scale_virtual_to_real[1];
            }
            ALIGN_USER_MAX => {
                y = self.scale_virtual_to_real[1] * y + self.real_adjustable_max[1];
                h *= self.scale_virtual_to_real[1];
            }
            _ => {
                y *= self.scale_virtual_to_real[1];
                h *= self.scale_virtual_to_real[1];
            }
        }
        AppliedRect { x, y, w, h }
    }
}

fn setup_viewable(
    viewport_x: f32,
    viewport_y: f32,
    viewport_w: f32,
    viewport_h: f32,
    horz_aspect_scale: f32,
    safe_horz: f32,
    safe_vert: f32,
    display_w: f32,
    display_h: f32,
) -> ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) {
    let mut inset_x = libm::floorf(HALF + display_w * (1.0 - safe_horz) * HALF);
    let mut inset_y = libm::floorf(HALF + display_h * (1.0 - safe_vert) * HALF);
    let mut right = display_w - inset_x;
    let mut bottom = display_h - inset_y;
    if inset_x <= viewport_x {
        inset_x = viewport_x;
    }
    if inset_y <= viewport_y {
        inset_y = viewport_y;
    }
    let vx1 = viewport_x + viewport_w;
    if vx1 < right {
        right = vx1;
    }
    let vy1 = viewport_y + viewport_h;
    if vy1 < bottom {
        bottom = vy1;
    }
    let real_min = [inset_x - viewport_x, inset_y - viewport_y];
    let real_max = [right - viewport_x, bottom - viewport_y];
    let virtual_min = [
        horz_aspect_scale * real_min[0] * (SCRPLACE_VIRTUAL_WIDTH / viewport_w),
        real_min[1] * (SCRPLACE_VIRTUAL_HEIGHT / viewport_h),
    ];
    let virtual_max = [
        horz_aspect_scale * real_max[0] * (SCRPLACE_VIRTUAL_WIDTH / viewport_w),
        real_max[1] * (SCRPLACE_VIRTUAL_HEIGHT / viewport_h),
    ];
    (real_min, real_max, virtual_min, virtual_max)
}
