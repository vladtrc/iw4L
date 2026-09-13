use std::sync::Arc;

use render_frame::FrameProductsSnapshot;

#[derive(Clone, Debug)]
pub struct ExtractedRenderFrameProducts(pub Arc<FrameProductsSnapshot>);

impl Default for ExtractedRenderFrameProducts {
    fn default() -> Self {
        Self(Arc::new(FrameProductsSnapshot::empty()))
    }
}
