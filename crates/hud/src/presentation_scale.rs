pub const VIRTUAL_HEIGHT: f32 = 480.0;

pub const VIRTUAL_WIDTH: f32 = 640.0;

const CHROME_BIAS: f32 = 0.5;

const LOWER_CHROME_BOOST: f32 = 1.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HorizontalAlign {
    SubLeft = 0,

    Left = 1,

    Center = 2,

    Right = 3,

    Fullscreen = 4,

    NoScale = 5,

    To640 = 6,

    CenterSafeArea = 7,

    UserLeft = 8,

    UserCenter = 9,

    UserRight = 10,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VerticalAlign {
    SubTop = 0,

    Top = 1,

    Center = 2,

    Bottom = 3,

    Fullscreen = 4,

    NoScale = 5,

    To480 = 6,

    CenterSafeArea = 7,

    UserTop = 8,

    UserCenter = 9,

    UserBottom = 10,
}

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

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlacedRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
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

    fn sub_rect(&self) -> (f32, f32) {
        let by_height = self.height * (VIRTUAL_WIDTH / VIRTUAL_HEIGHT);
        let width = by_height.min(self.width);
        let height = (self.width * (VIRTUAL_HEIGHT / VIRTUAL_WIDTH)).min(self.height);
        (width, height)
    }

    pub fn anchor_x(&self, align: HorizontalAlign) -> f32 {
        match align {
            HorizontalAlign::SubLeft => (self.width - self.sub_rect().0) * 0.5,
            HorizontalAlign::Left
            | HorizontalAlign::Fullscreen
            | HorizontalAlign::NoScale
            | HorizontalAlign::UserLeft => 0.0,
            HorizontalAlign::Center
            | HorizontalAlign::CenterSafeArea
            | HorizontalAlign::UserCenter => self.width * 0.5,
            HorizontalAlign::Right | HorizontalAlign::UserRight => self.width,
            HorizontalAlign::To640 => 0.0,
        }
    }

    pub fn anchor_y(&self, align: VerticalAlign) -> f32 {
        match align {
            VerticalAlign::SubTop => (self.height - self.sub_rect().1) * 0.5,
            VerticalAlign::Top
            | VerticalAlign::Fullscreen
            | VerticalAlign::NoScale
            | VerticalAlign::UserTop => 0.0,
            VerticalAlign::Center | VerticalAlign::CenterSafeArea | VerticalAlign::UserCenter => {
                self.height * 0.5
            }
            VerticalAlign::Bottom | VerticalAlign::UserBottom => self.height,
            VerticalAlign::To480 => 0.0,
        }
    }

    pub fn place(
        &self,
        class: ScaleClass,
        h_align: HorizontalAlign,
        v_align: VerticalAlign,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> PlacedRect {
        if class == ScaleClass::ViewportFill {
            return PlacedRect {
                left: 0.0,
                top: 0.0,
                width: self.width,
                height: self.height,
            };
        }
        let factor = self.factor(class);
        PlacedRect {
            left: self.anchor_x(h_align) + x * factor,
            top: self.anchor_y(v_align) + y * factor,
            width: width * factor,
            height: height * factor,
        }
    }
}
