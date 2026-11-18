use super::models::NativeMapXModelCatalog;
use super::tables::is_cac_table;
use crate::material::capture::NativeMaterialCatalog;
use crate::model::catalogues::{BodyMeshBuildCapture, FpvMeshBuildCapture};
use crate::weapons::weapon_catalog::NativeWeaponCapture;
use asset_anim::XAnimBuild;
use asset_core::ZoneOwner;
use asset_game::{FxCatalog, WeaponCatalog};
use asset_material::MaterialCatalog;
use asset_model::BodyMeshBuild;
use asset_model::{FpvMeshBuild, WorldWeaponBuild};
use asset_transport::StageHandle;
use assets::MapXModelCatalog;
use assets::ScriptSources;
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub struct CommonWalkSink {
    pub scene_models: asset_world::MapXModelSceneCatalog,
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub materials: MaterialCatalog,
    pub weapons: WeaponCatalog,
    pub fpv_meshes: FpvMeshBuild,
    pub world_weapons: WorldWeaponBuild,
    pub projectile_meshes: asset_model::ProjectileMeshBuild,
    pub xanims: XAnimBuild,
    pub player_anim_sources: asset_anim::PlayerAnimSources,
    pub fx: FxCatalog,
    pub fx_models: asset_game::FxModelCatalog,
    pub tracers: asset_game::TracerCatalog,
    strings_iw5: ext_iw5_fastfile::ScriptStrings,
    iw5_surfaces: HashMap<
        ext_iw5_fastfile::Ptr,
        (Option<ext_iw5_fastfile::Ptr>, Option<ext_iw5_fastfile::Ptr>),
    >,
    xmodel_names_iw5: HashMap<ext_iw5_fastfile::Ptr, ext_iw5_fastfile::Ptr>,
    material_names_iw5: HashMap<ext_iw5_fastfile::Ptr, ext_iw5_fastfile::Ptr>,
    fx_names_iw5: HashMap<ext_iw5_fastfile::Ptr, String>,
    fx_aliases_iw5: HashMap<ext_iw5_fastfile::Ptr, ext_iw5_fastfile::Ptr>,
    last_fx_name_iw5: Option<String>,
    pub light_def_table: usize,
    pub light_def_bodies: usize,
    pub stats_tables: BTreeMap<String, asset_game::CapturedStringTable>,
    pub film_visions:
        BTreeMap<String, Result<asset_world::FilmVision, asset_world::FilmVisionParseError>>,
    pub sound: Option<asset_audio::ZoneSoundCapture>,
}

#[derive(Default)]
pub struct MaterialPopulationSink {
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub materials: MaterialCatalog,

    pub stats_tables: BTreeMap<String, asset_game::CapturedStringTable>,
    pub scripts: ScriptSources,

    pub sound: Option<asset_audio::ZoneSoundCapture>,
}

impl CommonWalkSink {
    pub fn seed_materials(&mut self, mut materials: MaterialCatalog) {
        materials.prepare_for_next_zone();
        self.materials = materials;
    }

    fn asset_walked(&mut self) {
        self.walked += 1;
        if let Some(stage) = self.stage.as_ref() {
            stage.set_completed(self.walked as u64);
        }
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.scene_models.set_capture_zone(zone);
        self.materials.set_capture_zone(zone);
        self.xanims.set_capture_zone(zone);
        self.fx.set_capture_zone(zone);
        self.fx_models.set_capture_zone(zone);
        self.tracers.set_capture_zone(zone);
        self.fpv_meshes.set_capture_zone(zone);
        self.world_weapons.set_capture_zone(zone);
    }

    pub fn set_capture_ns(&mut self, ns: asset_core::FamilyId) {
        self.materials.set_capture_ns(ns);
        self.xanims.set_capture_ns(ns);
        self.fpv_meshes.set_capture_ns(ns);
        self.world_weapons.set_capture_ns(ns);
        self.projectile_meshes.set_capture_ns(ns);
        self.tracers.set_capture_ns(ns);
        self.weapons.set_capture_ns(ns);
        self.fx.set_capture_ns(ns);
        self.fx_models.set_capture_ns(ns);
    }

    fn keep_stats_table(&mut self, table: asset_game::CapturedStringTable) {
        if is_cac_table(&table.name) {
            self.stats_tables.insert(table.name.clone(), table);
        }
    }
}

impl MaterialPopulationSink {
    pub fn seed_materials(&mut self, mut materials: MaterialCatalog) {
        materials.prepare_for_next_zone();
        self.materials = materials;
    }

    fn asset_walked(&mut self) {
        self.walked += 1;
        if let Some(stage) = self.stage.as_ref() {
            stage.set_completed(self.walked as u64);
        }
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.materials.set_capture_zone(zone);
    }

    pub fn set_capture_ns(&mut self, ns: asset_core::FamilyId) {
        self.materials.set_capture_ns(ns);
    }

    fn keep_stats_table(&mut self, table: Option<asset_game::CapturedStringTable>) {
        if let Some(table) = table {
            self.stats_tables.insert(table.name.clone(), table);
        }
    }
}

impl ext_iw5_fastfile::AssetSink for CommonWalkSink {
    fn set_script_strings(&mut self, strings: ext_iw5_fastfile::ScriptStrings) {
        self.strings_iw5 = strings;
    }

    fn load_asset(
        &mut self,
        s: &mut ext_iw5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        let loaded = ext_iw5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        if ty == ext_iw5_fastfile::AssetType::LightDef {
            self.light_def_table += 1;
            if loaded {
                self.light_def_bodies += 1;
            }
        }
        if loaded && ty == ext_iw5_fastfile::AssetType::Weapon {
            let fx_name_at_slot = |mut slot| {
                for _ in 0..8 {
                    if let Some(name) = self.fx_names_iw5.get(&slot) {
                        return Some(name.clone());
                    }
                    slot = *self.fx_aliases_iw5.get(&slot)?;
                }
                None
            };
            self.weapons
                .capture_iw5(s, &self.strings_iw5, &fx_name_at_slot);
        }
        self.asset_walked();
        Ok(())
    }
}

impl ext_iw5_fastfile::AssetLinkSink for CommonWalkSink {
    fn remember_xmodel_surfaces(
        &mut self,
        slot: ext_iw5_fastfile::Ptr,
        surfaces: Option<ext_iw5_fastfile::Ptr>,
        name: Option<ext_iw5_fastfile::Ptr>,
    ) {
        self.iw5_surfaces.insert(slot, (surfaces, name));
    }

    fn xmodel_surfaces(
        &self,
        slot: ext_iw5_fastfile::Ptr,
    ) -> (Option<ext_iw5_fastfile::Ptr>, Option<ext_iw5_fastfile::Ptr>) {
        self.iw5_surfaces
            .get(&slot)
            .copied()
            .unwrap_or((None, None))
    }

    fn capture_fx(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: ext_iw5_fastfile::FxEffectDefGeometry,
    ) -> ext_iw5_fastfile::Result<()> {
        self.last_fx_name_iw5 = geometry
            .name
            .and_then(|ptr| stream.cstr(ptr).ok())
            .map(str::to_owned);
        crate::presentation::fx_catalog::capture(&mut self.fx, stream, geometry, &self.materials)
    }

    fn capture_attachment(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: &ext_iw5_fastfile::AttachmentGeometry,
    ) -> ext_iw5_fastfile::Result<()> {
        let fx_name_at_slot = |mut slot| {
            for _ in 0..8 {
                if let Some(name) = self.fx_names_iw5.get(&slot) {
                    return Some(name.clone());
                }
                slot = *self.fx_aliases_iw5.get(&slot)?;
            }
            None
        };
        crate::weapons::weapon_catalog::attachments::capture(
            self.weapons.attachment_data_mut(),
            stream,
            geometry,
            &fx_name_at_slot,
        );
        Ok(())
    }

    fn loaded(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result =
                ext_iw5_fastfile::AssetLinkSink::loaded(capture, stream, ty, slot, insert_slot);
            sound.guard(result);
        }
        self.materials.iw5_loaded(stream, ty, slot, insert_slot);
        if ty == ext_iw5_fastfile::AssetType::Fx
            && let Some(name) = self.last_fx_name_iw5.take()
        {
            self.fx_names_iw5.insert(slot, name.clone());
            if let Some(insert_slot) = insert_slot {
                self.fx_names_iw5.insert(insert_slot, name);
            }
        }
        if ty == ext_iw5_fastfile::AssetType::XModel
            && let Some(geometry) = stream.latest_xmodel()
            && let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok())
        {
            let asset = crate::model::capture::capture_xmodel_skel_iw5(
                stream,
                &self.strings_iw5,
                geometry,
                &self.materials,
            )
            .map(|skel| {
                let skel = std::sync::Arc::new(skel);
                crate::model::capture::retain_surfaces(
                    &mut self.shared_surfaces,
                    stream,
                    geometry,
                    skel.clone(),
                );
                let ns = Some(crate::ID);
                self.fpv_meshes.capture_shared(ns, &skel, &self.materials);
                self.world_weapons
                    .capture_shared(ns, &skel, &self.materials);
                asset_world::MapXModelSceneAsset::Captured {
                    family: crate::ID,
                    skel,
                }
            })
            .unwrap_or(asset_world::MapXModelSceneAsset::Unavailable {
                reason: "IW5 common XModel skeleton capture failed",
            });
            self.scene_models
                .insert(asset_world::MapXModelAssetKey(name.to_owned()), asset);
        }
        Ok(())
    }

    fn capture_xanim(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: ext_iw5_fastfile::XAnimPartsGeometry,
    ) -> ext_iw5_fastfile::Result<()> {
        crate::anim::capture::capture(&mut self.xanims, s, &self.strings_iw5, geometry);
        Ok(())
    }

    fn alias(
        &mut self,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        target: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result = ext_iw5_fastfile::AssetLinkSink::alias(capture, ty, slot, target);
            sound.guard(result);
        }
        self.materials.iw5_alias(ty, slot, target);
        if ty == ext_iw5_fastfile::AssetType::XModelSurfs
            && let Some(surfaces) = self.iw5_surfaces.get(&target).copied()
        {
            self.iw5_surfaces.insert(slot, surfaces);
        }
        if ty == ext_iw5_fastfile::AssetType::Fx {
            self.fx_aliases_iw5.insert(slot, target);
        }
        if ty == ext_iw5_fastfile::AssetType::Material {
            if let Some(&name) = self.material_names_iw5.get(&target) {
                self.material_names_iw5.insert(slot, name);
            } else {
                self.material_names_iw5.remove(&slot);
            }
        }
        if ty == ext_iw5_fastfile::AssetType::XModel
            && let Some(&name) = self.xmodel_names_iw5.get(&target)
        {
            self.xmodel_names_iw5.insert(slot, name);
        }
        Ok(())
    }

    fn remember_material_name(
        &mut self,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
        name: ext_iw5_fastfile::Ptr,
    ) {
        self.material_names_iw5.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.material_names_iw5.insert(insert_slot, name);
        }
    }

    fn material_name_ptr(&self, slot: ext_iw5_fastfile::Ptr) -> Option<ext_iw5_fastfile::Ptr> {
        self.material_names_iw5.get(&slot).copied()
    }

    fn remember_xmodel_name(
        &mut self,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
        name: ext_iw5_fastfile::Ptr,
    ) {
        self.xmodel_names_iw5.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.xmodel_names_iw5.insert(insert_slot, name);
        }
    }

    fn xmodel_name_ptr(&self, slot: ext_iw5_fastfile::Ptr) -> Option<ext_iw5_fastfile::Ptr> {
        self.xmodel_names_iw5.get(&slot).copied()
    }

    fn capture_string_table(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        header: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(table) = super::tables::capture(s, header) {
            self.keep_stats_table(table);
        }
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        self.player_anim_sources
            .capture(crate::ID, name, data, zlib_compressed);
        capture_film_vision(&mut self.film_visions, name, data, zlib_compressed);
        Ok(())
    }

    crate::sound::forward_iw5_sound!();
}

impl ext_iw5_fastfile::AssetSink for MaterialPopulationSink {
    fn set_script_strings(&mut self, _strings: ext_iw5_fastfile::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ext_iw5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        ext_iw5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        self.asset_walked();
        Ok(())
    }
}

impl ext_iw5_fastfile::AssetLinkSink for MaterialPopulationSink {
    fn loaded(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result =
                ext_iw5_fastfile::AssetLinkSink::loaded(capture, stream, ty, slot, insert_slot);
            sound.guard(result);
        }
        self.materials.iw5_loaded(stream, ty, slot, insert_slot);
        Ok(())
    }

    fn alias(
        &mut self,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        target: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result = ext_iw5_fastfile::AssetLinkSink::alias(capture, ty, slot, target);
            sound.guard(result);
        }
        self.materials.iw5_alias(ty, slot, target);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        header: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        self.keep_stats_table(super::tables::capture(s, header));
        Ok(())
    }

    crate::sound::forward_iw5_sound!();
}

fn capture_film_vision(
    catalog: &mut assets::FilmVisionCatalog,
    name: &str,
    data: &[u8],
    compressed: bool,
) {
    let result = match asset_world::parse_film_vision_rawfile(name, data, compressed) {
        Ok(Some(vision)) => Ok(vision),
        Ok(None) => return,
        Err(error) => Err(error),
    };
    catalog.insert(name.replace('\\', "/").to_ascii_lowercase(), result);
}

#[derive(Default)]
pub struct ZoneWalkSink {
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub materials: MaterialCatalog,
    pub fx: FxCatalog,
    pub fx_models: asset_game::FxModelCatalog,
    pub tracers: asset_game::TracerCatalog,
    pub map_xmodels: MapXModelCatalog,
    pub bodies: BodyMeshBuild,
    pub fpv_meshes: FpvMeshBuild,
    pub xanims: XAnimBuild,
    pub xmodel_coll: asset_world::XModelCollCatalog,
    pub compass: asset_world::MapCompassSource,
    pub script_sound: asset_audio::MapScriptSoundSource,
    pub iw5_map: asset_world::MapDeclarations,
    pub exp_fog: Option<asset_world::ExpFog>,
    pub film_visions:
        BTreeMap<String, Result<asset_world::FilmVision, asset_world::FilmVisionParseError>>,
    pub createart_name: Option<String>,
    pub light_def_table: usize,
    pub light_def_bodies: usize,
    strings_iw5: ext_iw5_fastfile::ScriptStrings,
    iw5_surfaces: HashMap<
        ext_iw5_fastfile::Ptr,
        (Option<ext_iw5_fastfile::Ptr>, Option<ext_iw5_fastfile::Ptr>),
    >,
    pub sound: Option<asset_audio::ZoneSoundCapture>,
}

impl ZoneWalkSink {
    pub fn seed_materials(&mut self, mut materials: MaterialCatalog) {
        materials.prepare_for_next_zone();
        self.materials = materials;
    }

    fn asset_walked(&mut self) {
        self.walked += 1;
        if let Some(stage) = self.stage.as_ref() {
            stage.set_completed(self.walked as u64);
        }
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.materials.set_capture_zone(zone);
        self.fx.set_capture_zone(zone);
        self.fx_models.set_capture_zone(zone);
        self.tracers.set_capture_zone(zone);
        self.xanims.set_capture_zone(zone);
        self.map_xmodels.set_capture_zone(zone);
        self.fpv_meshes.set_capture_zone(zone);
    }

    pub fn set_capture_ns(&mut self, ns: asset_core::FamilyId) {
        self.materials.set_capture_ns(ns);
        self.xanims.set_capture_ns(ns);
        self.fpv_meshes.set_capture_ns(ns);
        self.tracers.set_capture_ns(ns);
        self.fx.set_capture_ns(ns);
        self.fx_models.set_capture_ns(ns);
    }
}
impl ext_iw5_fastfile::AssetSink for ZoneWalkSink {
    fn set_script_strings(&mut self, strings: ext_iw5_fastfile::ScriptStrings) {
        self.strings_iw5 = strings;
    }

    fn load_asset(
        &mut self,
        s: &mut ext_iw5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        let loaded = ext_iw5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        if ty == ext_iw5_fastfile::AssetType::LightDef {
            self.light_def_table += 1;
            if loaded {
                self.light_def_bodies += 1;
            }
        }
        self.asset_walked();
        Ok(())
    }
}
impl ext_iw5_fastfile::AssetLinkSink for ZoneWalkSink {
    fn capture_fx(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: ext_iw5_fastfile::FxEffectDefGeometry,
    ) -> ext_iw5_fastfile::Result<()> {
        crate::presentation::fx_catalog::capture(&mut self.fx, stream, geometry, &self.materials)
    }

    fn remember_xmodel_surfaces(
        &mut self,
        slot: ext_iw5_fastfile::Ptr,
        surfaces: Option<ext_iw5_fastfile::Ptr>,
        name: Option<ext_iw5_fastfile::Ptr>,
    ) {
        self.iw5_surfaces.insert(slot, (surfaces, name));
    }

    fn xmodel_surfaces(
        &self,
        slot: ext_iw5_fastfile::Ptr,
    ) -> (Option<ext_iw5_fastfile::Ptr>, Option<ext_iw5_fastfile::Ptr>) {
        self.iw5_surfaces
            .get(&slot)
            .copied()
            .unwrap_or((None, None))
    }

    fn loaded(
        &mut self,
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        insert_slot: Option<ext_iw5_fastfile::Ptr>,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result =
                ext_iw5_fastfile::AssetLinkSink::loaded(capture, stream, ty, slot, insert_slot);
            sound.guard(result);
        }
        self.materials.iw5_loaded(stream, ty, slot, insert_slot);
        if ty == ext_iw5_fastfile::AssetType::XModel {
            self.map_xmodels.capture_iw5(
                stream,
                &self.materials,
                slot,
                insert_slot,
                &self.strings_iw5,
            );
            self.fpv_meshes
                .capture_iw5(stream, &self.strings_iw5, &self.materials);
            self.bodies
                .capture_iw5(stream, &self.strings_iw5, &self.materials);
            crate::world::collision::capture_xmodel_collision(
                &mut self.xmodel_coll,
                stream,
                slot,
                insert_slot,
            );
        }
        Ok(())
    }

    fn capture_xanim(
        &mut self,
        s: &ext_iw5_fastfile::ZoneStream<'_>,
        geometry: ext_iw5_fastfile::XAnimPartsGeometry,
    ) -> ext_iw5_fastfile::Result<()> {
        crate::anim::capture::capture(&mut self.xanims, s, &self.strings_iw5, geometry);
        Ok(())
    }

    fn alias(
        &mut self,
        ty: ext_iw5_fastfile::AssetType,
        slot: ext_iw5_fastfile::Ptr,
        target: ext_iw5_fastfile::Ptr,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut()
            && let Some(capture) = sound.native_mut::<crate::sound::Iw5SoundCapture>()
        {
            let result = ext_iw5_fastfile::AssetLinkSink::alias(capture, ty, slot, target);
            sound.guard(result);
        }
        self.materials.iw5_alias(ty, slot, target);
        if ty == ext_iw5_fastfile::AssetType::XModelSurfs
            && let Some(surfaces) = self.iw5_surfaces.get(&target).copied()
        {
            self.iw5_surfaces.insert(slot, surfaces);
        }
        if ty == ext_iw5_fastfile::AssetType::XModel {
            self.map_xmodels.alias(
                asset_material::AssetPointerIdentity {
                    block: slot.block,
                    offset: slot.offset,
                },
                asset_material::AssetPointerIdentity {
                    block: target.block,
                    offset: target.offset,
                },
            );
            self.xmodel_coll.alias_captured(
                asset_material::AssetPointerIdentity {
                    block: slot.block,
                    offset: slot.offset,
                },
                asset_material::AssetPointerIdentity {
                    block: target.block,
                    offset: target.offset,
                },
            );
        }
        Ok(())
    }

    fn capture_script_file(
        &mut self,
        name: &str,
        stack: &[u8],
        bytecode: &[u8],
    ) -> ext_iw5_fastfile::Result<()> {
        self.script_sound.capture_iw5(name, stack, bytecode);
        self.compass.capture_iw5(name, stack);
        self.iw5_map.capture(name, stack);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_iw5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        capture_film_vision(&mut self.film_visions, name, data, zlib_compressed);
        self.compass.capture(name, data, zlib_compressed);
        if asset_world::is_createart_source(name) {
            self.createart_name = Some(name.to_owned());
        }
        if let Some(fog) = asset_world::parse_createart_rawfile(name, data, zlib_compressed) {
            let fog_file = asset_world::is_createart_fog_file(name);
            if self.exp_fog.is_none() || fog_file {
                self.exp_fog = Some(fog);
                self.createart_name = Some(name.to_owned());
            }
        }
        Ok(())
    }

    crate::sound::forward_iw5_sound!();
}
