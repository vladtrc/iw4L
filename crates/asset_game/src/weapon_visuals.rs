use asset_core::{AssetEdge, MaterialSpace};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeaponReticleAssets {
    pub center_material: Option<String>,

    pub side_material: Option<String>,

    pub center_edge: AssetEdge<MaterialSpace>,

    pub side_edge: AssetEdge<MaterialSpace>,

    pub center_image: Option<String>,

    pub side_image: Option<String>,

    pub center_size: i32,

    pub side_size: i32,

    /// Whether the zone authored a reticle material at all. The slot pointers
    /// that answered this during the walk stay on the build row: the HUD, and
    /// the edge stamping beside it, only ever asked whether there was one.
    pub center_authored: bool,

    pub side_authored: bool,
}
