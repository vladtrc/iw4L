pub const WATER_SHEETING_MATERIAL: &str = "watersheeting_color_distort_blur";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSheetingFrame {
    pub distortion: [f32; 4],
    pub fade: f32,
}
