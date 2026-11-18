pub const VIRTUAL_HEIGHT: f32 = 480.0;

const CHROME_BIAS: f32 = 0.5;

const LOWER_CHROME_BOOST: f32 = 1.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScaleClass {
    Chrome,

    LowerChrome,

    ProjectionBound,

    ViewportFill,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PresentationScale {
    width: f32,
    height: f32,
}

impl PresentationScale {
    pub fn from_window(width: f32, height: f32) -> Self {
        Self {
            width: width.max(1.0),
            height: height.max(1.0),
        }
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn factor(&self, class: ScaleClass) -> f32 {
        let projection = self.height / VIRTUAL_HEIGHT;
        match class {
            ScaleClass::Chrome => projection * CHROME_BIAS,
            ScaleClass::LowerChrome => projection * CHROME_BIAS * LOWER_CHROME_BOOST,
            ScaleClass::ProjectionBound => projection,
            ScaleClass::ViewportFill => projection,
        }
    }
}
