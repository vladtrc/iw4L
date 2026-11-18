use crate::world::entities::{
    flag_descriptors_iw5, map_script_structs_iw5, map_use_triggers_iw5,
    script_brush_model_placements_iw5, script_model_placements_iw5,
};
use crate::world::models::build_iw5_static_model_instances;
use asset_material::{AssetPointerIdentity, MaterialCatalog};
use asset_world::{
    MapXModelAssetKey, MapXModelSceneAsset, PreparedMapModels, StaticModelDrawError,
};
use assets::{MapXModelCatalog, link_model_placements, normalize_bomb_sites};
fn address(p: ext_iw5_fastfile::Ptr) -> AssetPointerIdentity {
    AssetPointerIdentity {
        block: p.block,
        offset: p.offset,
    }
}
pub trait NativeMapXModelCatalog {
    fn capture_iw5(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
        strings: &ext_iw5_fastfile::ScriptStrings,
    );
}

impl NativeMapXModelCatalog for MapXModelCatalog {
    fn capture_iw5(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
        strings: &ext_iw5_fastfile::ScriptStrings,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            self.note_failed_mesh();
            return;
        };
        let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok()) else {
            self.note_failed_mesh();
            return;
        };
        let scene_key = MapXModelAssetKey(name.to_owned());
        let scene_asset = crate::model::capture::capture_xmodel_skel_iw5_with_shared(
            stream,
            strings,
            geometry,
            materials,
            &self.shared_surfaces,
        )
        .map(|skel| MapXModelSceneAsset::Captured {
            family: crate::ID,
            skel: std::sync::Arc::new(skel),
        })
        .unwrap_or(MapXModelSceneAsset::Unavailable {
            reason: "IW5 XModel scene skeleton capture failed",
        });
        self.insert_scene_asset(scene_key, scene_asset);
        let Ok(mesh) =
            crate::world::models::build_iw5_xmodel_mesh(stream, geometry, Some(materials))
        else {
            self.note_failed_mesh();
            return;
        };
        self.insert_mesh(mesh, address(slot), insert_slot.map(address));
    }
}
pub fn build_iw5_static_model_draw(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: ext_iw5_fastfile::GfxWorldGeometry,
    catalog: MapXModelCatalog,
) -> PreparedMapModels {
    let placements = build_iw5_static_model_instances(stream, geometry).map_err(|_| {
        StaticModelDrawError::InstancesUnavailable {
            smodel_count: geometry.smodel_count,
        }
    });
    let mut scripts = script_model_placements_iw5(stream);
    let mut brushes = script_brush_model_placements_iw5(stream);
    normalize_bomb_sites(
        &mut scripts,
        &mut brushes,
        &map_use_triggers_iw5(stream),
        "dd_bombzone",
    );
    let descriptors = flag_descriptors_iw5(stream);
    let structs = map_script_structs_iw5(stream);
    link_model_placements(
        placements,
        &scripts,
        brushes,
        descriptors,
        structs,
        catalog,
        geometry.smodel_count,
    )
}
