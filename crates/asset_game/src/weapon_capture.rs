use std::any::Any;

/// Native captured weapon records, owned and merged by their format implementation.
pub trait WeaponCaptureData: Any + std::fmt::Debug + Send + Sync {
    fn clone_box(&self) -> Box<dyn WeaponCaptureData>;
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
    fn fill_missing(&mut self, donor: Box<dyn WeaponCaptureData>);
}
impl Clone for Box<dyn WeaponCaptureData> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

#[derive(Clone, Debug, Default)]
pub struct WeaponNativeCapture(Option<Box<dyn WeaponCaptureData>>);
impl WeaponNativeCapture {
    pub fn new(data: impl WeaponCaptureData) -> Self {
        Self(Some(Box::new(data)))
    }
    pub fn get<T: WeaponCaptureData>(&self) -> Option<&T> {
        self.0.as_ref()?.as_any().downcast_ref()
    }
    pub fn fill_missing(&mut self, donor: Self) {
        let Some(donor) = donor.0 else {
            return;
        };
        match self.0.as_mut() {
            Some(data) => data.fill_missing(donor),
            None => self.0 = Some(donor),
        }
    }
}
