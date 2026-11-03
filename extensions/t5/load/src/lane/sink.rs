use super::models::NativeMapXModelCatalog;
use super::tables::is_cac_table;
use crate::material::capture::NativeMaterialCatalog;
use crate::model::catalogues::{BodyMeshBuildCapture, FpvMeshBuildCapture};
use crate::weapons::weapon_catalog::NativeWeaponCapture;
use asset_anim::XAnimBuild;
use asset_core::ZoneOwner;
use asset_game::{FxCatalog, ImpactFxCatalog, WeaponCatalog};
use asset_material::MaterialCatalog;
use asset_model::BodyMeshBuild;
use asset_model::{FpvMeshBuild, WorldWeaponBuild};
use asset_transport::StageHandle;
use assets::MapXModelCatalog;
use assets::ScriptSources;
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub struct CommonWalkSink {
    pub scripts: ScriptSources,
    pub scene_models: asset_world::MapXModelSceneCatalog,
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
    pub impact_fx: ImpactFxCatalog,
    pub tracers: asset_game::TracerCatalog,
    strings_t5: ext_t5_fastfile::ScriptStrings,
    xmodel_names_t5: HashMap<ext_t5_fastfile::Ptr, ext_t5_fastfile::Ptr>,
    material_names_t5: HashMap<ext_t5_fastfile::Ptr, ext_t5_fastfile::Ptr>,
    pub teamsets: HashMap<String, asset_game::MapTeamSettings>,
    pub stats_tables: BTreeMap<String, asset_game::CapturedStringTable>,
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

impl ext_t5_fastfile::AssetSink for CommonWalkSink {
    fn set_script_strings(&mut self, strings: ext_t5_fastfile::ScriptStrings) {
        self.strings_t5 = strings;
    }

    fn load_asset(
        &mut self,
        s: &mut ext_t5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        let loaded = ext_t5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        if loaded && ty == ext_t5_fastfile::AssetType::Weapon {
            self.weapons.capture_t5(s, &self.strings_t5);
        }
        self.asset_walked();
        Ok(())
    }
}

impl ext_t5_fastfile::AssetLinkSink for CommonWalkSink {
    fn capture_impact_fx(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        name: ext_t5_fastfile::Ptr,
        entries: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        crate::presentation::impact_fx::capture(&mut self.impact_fx, s, name, entries, &self.fx)
    }

    fn loaded(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_loaded(stream, ty, slot, insert_slot);
        if ty == ext_t5_fastfile::AssetType::Fx {
            self.fx
                .note_loaded(link_address(slot), insert_slot.map(link_address));
        }
        if ty == ext_t5_fastfile::AssetType::XModel
            && let Some(geometry) = stream.latest_xmodel()
            && let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok())
        {
            let asset = crate::model::capture::capture_xmodel_skel_t5(
                stream,
                &self.strings_t5,
                geometry,
                &self.materials,
            )
            .map(|skel| {
                let skel = std::sync::Arc::new(skel);
                let ns = crate::ID;
                self.fpv_meshes
                    .capture_shared(Some(ns), &skel, &self.materials);
                self.world_weapons
                    .capture_shared(Some(ns), &skel, &self.materials);
                self.projectile_meshes
                    .capture_shared(ns, &skel, &self.materials);
                asset_world::MapXModelSceneAsset::Captured {
                    family: crate::ID,
                    skel,
                }
            })
            .unwrap_or(asset_world::MapXModelSceneAsset::Unavailable {
                reason: "T5 common XModel skeleton capture failed",
            });
            self.scene_models
                .insert(asset_world::MapXModelAssetKey(name.to_owned()), asset);
        }
        Ok(())
    }

    fn alias(
        &mut self,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_alias(ty, slot, target);
        if ty == ext_t5_fastfile::AssetType::Fx {
            self.fx.note_alias(link_address(slot), link_address(target));
        }
        if ty == ext_t5_fastfile::AssetType::Material {
            if let Some(&name) = self.material_names_t5.get(&target) {
                self.material_names_t5.insert(slot, name);
            } else {
                self.material_names_t5.remove(&slot);
            }
        }
        if ty == ext_t5_fastfile::AssetType::XModel
            && let Some(&name) = self.xmodel_names_t5.get(&target)
        {
            self.xmodel_names_t5.insert(slot, name);
        }
        Ok(())
    }

    fn remember_material_name(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
        name: ext_t5_fastfile::Ptr,
    ) {
        self.material_names_t5.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.material_names_t5.insert(insert_slot, name);
        }
    }

    fn material_name_ptr(&self, slot: ext_t5_fastfile::Ptr) -> Option<ext_t5_fastfile::Ptr> {
        self.material_names_t5.get(&slot).copied()
    }

    fn remember_xmodel_name(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
        name: ext_t5_fastfile::Ptr,
    ) {
        self.xmodel_names_t5.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.xmodel_names_t5.insert(insert_slot, name);
        }
    }

    fn xmodel_name_ptr(&self, slot: ext_t5_fastfile::Ptr) -> Option<ext_t5_fastfile::Ptr> {
        self.xmodel_names_t5.get(&slot).copied()
    }

    fn capture_xanim(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::XAnimPartsGeometry,
    ) -> ext_t5_fastfile::Result<()> {
        crate::anim::capture::capture(&mut self.xanims, s, &self.strings_t5, geometry);
        Ok(())
    }

    fn capture_fx(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::FxEffectDefGeometry,
    ) -> ext_t5_fastfile::Result<()> {
        crate::presentation::fx_catalog::capture(
            &mut self.fx,
            s,
            geometry,
            &self.materials,
            &|slot| {
                let name = self.material_names_t5.get(&slot).copied().or_else(|| {
                    let ext_t5_fastfile::ZonePtr::Offset(target) = s.ptr_at(slot, 0).ok()? else {
                        return None;
                    };
                    self.material_names_t5
                        .get(&s.resolve_alias(target))
                        .copied()
                })?;
                s.cstr(s.resolve_alias(name)).ok().map(str::to_owned)
            },
        );
        Ok(())
    }

    fn nested_shader(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_shader(s, kind, slot);
        Ok(())
    }

    fn nested_shader_alias(
        &mut self,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_shader_alias(kind, slot, target);
        Ok(())
    }

    fn nested_vertex_decl(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_vertex_decl(s, slot);
        Ok(())
    }

    fn nested_vertex_decl_alias(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_vertex_decl_alias(slot, target);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_t5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        if let Some((key, icons)) = crate::presentation::teams::t5_settings_from_teamset_rawfile(
            name,
            data,
            zlib_compressed,
        ) {
            self.teamsets.insert(key, icons);
        }
        self.player_anim_sources
            .capture(crate::ID, name, data, zlib_compressed);
        self.scripts.capture(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        header: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        if let Some(table) = super::tables::capture(s, header) {
            self.scripts.capture_table(&table);
            if is_cac_table(&table.name) {
                self.keep_stats_table(table);
            }
        }
        Ok(())
    }

    crate::sound::forward_t5_sound!();
}

impl ext_t5_fastfile::AssetSink for MaterialPopulationSink {
    fn set_script_strings(&mut self, _strings: ext_t5_fastfile::ScriptStrings) {}

    fn load_asset(
        &mut self,
        s: &mut ext_t5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        ext_t5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        self.asset_walked();
        Ok(())
    }
}

impl ext_t5_fastfile::AssetLinkSink for MaterialPopulationSink {
    fn loaded(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_loaded(stream, ty, slot, insert_slot);
        Ok(())
    }

    fn alias(
        &mut self,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_alias(ty, slot, target);
        Ok(())
    }

    fn nested_shader(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_shader(s, kind, slot);
        Ok(())
    }

    fn nested_shader_alias(
        &mut self,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_shader_alias(kind, slot, target);
        Ok(())
    }

    fn nested_vertex_decl(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_vertex_decl(s, slot);
        Ok(())
    }

    fn nested_vertex_decl_alias(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_vertex_decl_alias(slot, target);
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_t5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        self.scripts.capture(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        header: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        let table = super::tables::capture(s, header);
        if let Some(table) = &table {
            self.scripts.capture_table(table);
        }
        self.keep_stats_table(table.filter(|t| is_cac_table(&t.name)));
        Ok(())
    }

    crate::sound::forward_t5_sound!();
}

fn link_address(p: ext_t5_fastfile::Ptr) -> asset_material::AssetPointerIdentity {
    asset_material::AssetPointerIdentity {
        block: p.block,
        offset: p.offset,
    }
}

#[derive(Default)]
pub struct ZoneWalkSink {
    pub scripts: assets::ScriptSources,
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub materials: MaterialCatalog,
    pub fx: FxCatalog,
    material_names_t5: HashMap<ext_t5_fastfile::Ptr, ext_t5_fastfile::Ptr>,
    pub fx_models: asset_game::FxModelCatalog,
    pub impact_fx: ImpactFxCatalog,
    pub tracers: asset_game::TracerCatalog,
    pub map_xmodels: MapXModelCatalog,
    pub bodies: BodyMeshBuild,
    pub fpv_meshes: FpvMeshBuild,
    pub xanims: XAnimBuild,
    pub compass: asset_world::MapCompassSource,
    pub script_sound: asset_audio::MapScriptSoundSource,
    pub t5_teamset: Option<String>,
    pub exp_fog: Option<asset_world::ExpFog>,
    pub createart_name: Option<String>,
    strings_t5: ext_t5_fastfile::ScriptStrings,
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
impl ext_t5_fastfile::AssetSink for ZoneWalkSink {
    fn set_script_strings(&mut self, strings: ext_t5_fastfile::ScriptStrings) {
        self.strings_t5 = strings;
    }

    fn load_asset(
        &mut self,
        s: &mut ext_t5_fastfile::ZoneStream<'_>,
        _index: usize,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        ext_t5_fastfile::load_asset_at_observed(s, ty, slot, self)?;
        self.asset_walked();
        Ok(())
    }
}
impl ext_t5_fastfile::AssetLinkSink for ZoneWalkSink {
    fn capture_impact_fx(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        name: ext_t5_fastfile::Ptr,
        entries: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        crate::presentation::impact_fx::capture(&mut self.impact_fx, s, name, entries, &self.fx)
    }

    fn capture_fx(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::FxEffectDefGeometry,
    ) -> ext_t5_fastfile::Result<()> {
        crate::presentation::fx_catalog::capture(
            &mut self.fx,
            s,
            geometry,
            &self.materials,
            &|slot| {
                let name = self.material_names_t5.get(&slot).copied().or_else(|| {
                    let ext_t5_fastfile::ZonePtr::Offset(target) = s.ptr_at(slot, 0).ok()? else {
                        return None;
                    };
                    self.material_names_t5
                        .get(&s.resolve_alias(target))
                        .copied()
                })?;
                s.cstr(s.resolve_alias(name)).ok().map(str::to_owned)
            },
        );
        Ok(())
    }

    fn capture_destructible(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        header: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.map_xmodels
            .capture_t5_destructible(stream, header, &self.strings_t5, &self.fx)
    }

    fn loaded(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_loaded(stream, ty, slot, insert_slot);
        if ty == ext_t5_fastfile::AssetType::Fx {
            self.fx
                .note_loaded(link_address(slot), insert_slot.map(link_address));
        }
        if ty == ext_t5_fastfile::AssetType::XModel {
            self.map_xmodels.capture_t5(
                stream,
                &self.materials,
                slot,
                insert_slot,
                &self.strings_t5,
            );
            self.fpv_meshes
                .capture_t5(stream, &self.strings_t5, &self.materials);
            self.bodies
                .capture_t5(stream, &self.strings_t5, &self.materials);
        }
        Ok(())
    }
    fn alias(
        &mut self,
        ty: ext_t5_fastfile::AssetType,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_alias(ty, slot, target);
        if ty == ext_t5_fastfile::AssetType::Material {
            if let Some(name) = self.material_names_t5.get(&target).copied() {
                self.material_names_t5.insert(slot, name);
            } else {
                self.material_names_t5.remove(&slot);
            }
        }
        if ty == ext_t5_fastfile::AssetType::Fx {
            self.fx.note_alias(link_address(slot), link_address(target));
        }
        if ty == ext_t5_fastfile::AssetType::XModel {
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
        }
        Ok(())
    }

    fn remember_material_name(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
        name: ext_t5_fastfile::Ptr,
    ) {
        self.material_names_t5.insert(slot, name);
        if let Some(slot) = insert_slot {
            self.material_names_t5.insert(slot, name);
        }
    }

    fn material_name_ptr(&self, slot: ext_t5_fastfile::Ptr) -> Option<ext_t5_fastfile::Ptr> {
        self.material_names_t5.get(&slot).copied()
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> ext_t5_fastfile::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        self.compass.capture(name, data, zlib_compressed);
        if let Some(text) = asset_world::decode_rawfile_text(data, zlib_compressed) {
            self.script_sound.capture(name, text.as_bytes(), false);
        }
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
        if let Some(teamset) =
            crate::presentation::teams::t5_teamset_from_rawfile(name, data, zlib_compressed)
        {
            self.t5_teamset = Some(teamset);
        }
        self.scripts.capture(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        header: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        if let Some(table) = super::tables::capture(s, header) {
            self.scripts.capture_table(&table);
        }
        Ok(())
    }

    fn capture_xanim(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::XAnimPartsGeometry,
    ) -> ext_t5_fastfile::Result<()> {
        crate::anim::capture::capture(&mut self.xanims, s, &self.strings_t5, geometry);
        Ok(())
    }

    fn nested_shader(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_shader(s, kind, slot);
        Ok(())
    }

    fn nested_shader_alias(
        &mut self,
        kind: ext_t5_fastfile::NestedShaderKind,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_shader_alias(kind, slot, target);
        Ok(())
    }

    fn nested_vertex_decl(
        &mut self,
        s: &ext_t5_fastfile::ZoneStream<'_>,
        slot: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_vertex_decl(s, slot);
        Ok(())
    }

    fn nested_vertex_decl_alias(
        &mut self,
        slot: ext_t5_fastfile::Ptr,
        target: ext_t5_fastfile::Ptr,
    ) -> ext_t5_fastfile::Result<()> {
        self.materials.t5_nested_vertex_decl_alias(slot, target);
        Ok(())
    }

    crate::sound::forward_t5_sound!();
}
