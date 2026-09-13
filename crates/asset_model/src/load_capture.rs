use crate::{BodyMeshCatalog, FpvMeshCatalog};

#[derive(Default)]
pub struct ModelLoadCapture {
    pub bodies: BodyMeshCatalog,
    pub fpv_meshes: FpvMeshCatalog,
}
