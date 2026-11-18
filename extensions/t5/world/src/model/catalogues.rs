use asset_core::FamilyId;
use asset_material::MaterialCatalog;
use asset_model::*;
pub trait BodyMeshBuildCapture {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    );
}
impl BodyMeshBuildCapture for BodyMeshBuild {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(skel) =
            super::capture::capture_body_skel_t5(stream, strings, geometry, Some(materials))
        else {
            return;
        };
        self.insert_in(asset_core::FamilyId::new("t5"), skel, Some(materials));
    }
}

pub trait FpvMeshBuildCapture {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    );
}
impl FpvMeshBuildCapture for FpvMeshBuild {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(name_ptr) = geometry.name else {
            return;
        };
        let Ok(name) = stream.cstr(name_ptr) else {
            return;
        };
        if super::policy::classify(name) != Some(ModelKind::Fpv) {
            return;
        }
        let Some(skel) =
            super::capture::capture_fpv_skel_t5(stream, strings, geometry, Some(materials))
        else {
            return;
        };
        self.insert_in(FamilyId::new("t5"), skel, Some(materials));
    }
}

pub trait ProjectileMeshBuildCapture {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    );
}
impl ProjectileMeshBuildCapture for ProjectileMeshBuild {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(skel) =
            super::capture::capture_xmodel_skel_t5(stream, strings, geometry, materials)
        else {
            return;
        };
        self.insert_in(FamilyId::new("t5"), skel, Some(materials));
    }
}

pub trait WorldWeaponBuildCapture {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    );
}
impl WorldWeaponBuildCapture for WorldWeaponBuild {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        strings: &ext_t5_fastfile::ScriptStrings,
        materials: &MaterialCatalog,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            return;
        };
        let Some(skel) = super::capture::capture_world_weapon_skel_t5(
            stream,
            strings,
            geometry,
            Some(materials),
        ) else {
            return;
        };
        self.insert_in(FamilyId::new("t5"), skel, Some(materials));
    }
}
