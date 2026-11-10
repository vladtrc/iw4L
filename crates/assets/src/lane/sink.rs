use std::collections::{BTreeMap, BTreeSet, HashMap};

use fastfile_iw4::{
    AssetLinkSink, AssetSink, AssetType, FxEffectDefGeometry, Ptr, ScriptStrings,
    XAnimPartsGeometry, ZonePtr, ZoneStream, load_asset_at_observed,
};

use crate::MapXModelCatalog;

fn is_cac_table(name: &str) -> bool {
    asset_game::is_stats_table_name(name)
        || name.eq_ignore_ascii_case("mp/perkTable.csv")
        || name.eq_ignore_ascii_case("mp/rankTable.csv")
        || name.eq_ignore_ascii_case("mp/rankIconTable.csv")
        || name.eq_ignore_ascii_case("mp/unlockTable.csv")
        || name.eq_ignore_ascii_case("mp/unlockTableMP.csv")
        || name.eq_ignore_ascii_case("mp/attachmentTable.csv")
        || name.eq_ignore_ascii_case("mp/attachmentCombos.csv")
        || name.eq_ignore_ascii_case("mp/weaponoptions.csv")
        || name.eq_ignore_ascii_case("mp/camoTable.csv")
}

fn iw4_string_table(s: &ZoneStream<'_>, header: Ptr) -> Option<asset_game::CapturedStringTable> {
    let name = match s.ptr_at(header, 0).ok() {
        Some(ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned(),
        _ => return None,
    };
    let columns = s.i32_at(header, s.layout(4, 8)).unwrap_or(0).max(0) as usize;
    let rows = s.i32_at(header, s.layout(8, 12)).unwrap_or(0).max(0) as usize;
    let cells_n = columns.saturating_mul(rows);
    let arr = match s.ptr_at(header, s.layout(12, 16)).ok() {
        Some(ZonePtr::Offset(p)) => s.resolve_alias(p),
        _ => {
            return Some(asset_game::CapturedStringTable {
                name,
                columns,
                rows,
                cells: Vec::new(),
            });
        }
    };
    let mut cells = Vec::with_capacity(cells_n);
    let cell_sz = s.layout(asset_iw4::size::STRING_TABLE_CELL, 16);
    for i in 0..cells_n {
        let cell = arr.at(i * cell_sz);
        let value = match s.ptr_at(cell, 0).ok() {
            Some(ZonePtr::Offset(p)) => s.cstr(s.resolve_alias(p)).ok().unwrap_or("").to_owned(),
            _ => String::new(),
        };
        cells.push(value);
    }
    Some(asset_game::CapturedStringTable {
        name,
        columns,
        rows,
        cells,
    })
}
use crate::PreparedXModelWalkCensus;
use asset_anim::XAnimBuild;
use asset_core::ZoneOwner;
use asset_game::{FxCatalog, ImpactFxCatalog, WeaponCatalog};
use asset_material::MaterialCatalog;
use asset_model::{BodyMeshBuild, FpvMeshBuild, SoldierKit, WorldWeaponBuild, soldier_kits};
use asset_transport::StageHandle;
use asset_world::{ModelKind, build_xmodel_mesh, model_kind};

pub(super) struct DeferredModel {
    pub geometry: fastfile_iw4::XModelGeometry,
    pub slot: Ptr,
    pub insert_slot: Option<Ptr>,
}

#[derive(Default)]
pub(crate) struct ZoneWalkSink {
    pub(super) region_only: bool,
    pub(super) capture_weapons: bool,
    pub weapons: WeaponCatalog,
    pub world_weapons: WorldWeaponBuild,
    pub projectile_meshes: asset_model::ProjectileMeshBuild,
    pub stats_tables: BTreeMap<String, asset_game::CapturedStringTable>,
    pub(super) deferred_models: Vec<DeferredModel>,
    pub(super) deferred_aliases: Vec<(Ptr, Ptr)>,
    pub scripts: crate::ScriptSources,
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub models: ModelCensus,
    pub materials: MaterialCatalog,
    pub fx: FxCatalog,
    pub fx_models: asset_game::FxModelCatalog,
    pub impact_fx: ImpactFxCatalog,
    pub tracers: asset_game::TracerCatalog,
    pub map_xmodels: MapXModelCatalog,
    pub phys_presets: asset_world::PhysPresetCatalog,
    pub fx_glass_def_materials: Vec<(String, String)>,
    pub(super) script_strings: ScriptStrings,
    pub bodies: BodyMeshBuild,
    pub fpv_meshes: FpvMeshBuild,
    pub xanims: XAnimBuild,
    pub xmodel_coll: asset_world::XModelCollCatalog,
    pub compass: asset_world::MapCompassSource,
    pub script_sound: asset_audio::MapScriptSoundSource,
    pub exp_fog: Option<asset_world::ExpFog>,
    pub film_visions:
        BTreeMap<String, Result<asset_world::FilmVision, asset_world::FilmVisionParseError>>,
    pub createart_name: Option<String>,
    pub light_def_table: usize,
    pub light_def_bodies: usize,
    xmodel_names: HashMap<Ptr, Ptr>,
    xmodel_surfaces: HashMap<Ptr, Ptr>,
    xmodel_surface_names: HashMap<Ptr, Ptr>,
    pub sound: Option<asset_audio::ZoneSoundCapture>,
}

#[derive(Default)]
pub(crate) struct CommonWalkSink {
    pub runtime_profile: asset_core::RuntimeProfile,
    pub scripts: crate::ScriptSources,
    pub scene_models: asset_world::MapXModelSceneCatalog,
    pub shared_surfaces: asset_model::SharedXModelSurfaces,
    script_strings: ScriptStrings,
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub models: ModelCensus,
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
    xmodel_names: HashMap<Ptr, Ptr>,
    xmodel_surfaces: HashMap<Ptr, Ptr>,
    xmodel_surface_names: HashMap<Ptr, Ptr>,
    pub light_def_table: usize,
    pub light_def_bodies: usize,
    pub pen_table: Option<weapon_iw4::PenetrationDepthTable>,
    pub lochit_table: Option<[f32; weapon_iw4::HITLOC_COUNT]>,
    pub stats_tables: BTreeMap<String, asset_game::CapturedStringTable>,
    pub film_visions:
        BTreeMap<String, Result<asset_world::FilmVision, asset_world::FilmVisionParseError>>,
    pub sound: Option<asset_audio::ZoneSoundCapture>,
}

impl ZoneWalkSink {
    pub(crate) fn with_stage(stage: StageHandle) -> Self {
        Self {
            stage: Some(stage),
            ..Default::default()
        }
    }

    pub(crate) fn seed_materials(&mut self, mut materials: MaterialCatalog) {
        materials.prepare_for_next_zone();
        self.materials = materials;
    }

    fn asset_walked(&mut self) {
        self.walked += 1;
        if let Some(stage) = self.stage.as_ref() {
            stage.set_completed(self.walked as u64);
        }
    }

    pub(crate) fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.materials.set_capture_zone(zone);
        self.fx.set_capture_zone(zone);
        self.fx_models.set_capture_zone(zone);
        self.tracers.set_capture_zone(zone);
        self.xanims.set_capture_zone(zone);
        self.map_xmodels.set_capture_zone(zone);
        self.fpv_meshes.set_capture_zone(zone);
        self.world_weapons.set_capture_zone(zone);
    }

    pub(crate) fn set_capture_ns(&mut self, ns: asset_core::FamilyId) {
        self.materials.set_capture_ns(ns);
        self.xanims.set_capture_ns(ns);
        self.fpv_meshes.set_capture_ns(ns);
        self.weapons.set_capture_ns(ns);
        self.world_weapons.set_capture_ns(ns);
        self.projectile_meshes.set_capture_ns(ns);
        self.tracers.set_capture_ns(ns);
        self.fx.set_capture_ns(ns);
        self.fx_models.set_capture_ns(ns);
    }
}

impl CommonWalkSink {
    pub(crate) fn with_stage(stage: StageHandle) -> Self {
        Self {
            stage: Some(stage),
            ..Default::default()
        }
    }

    pub(crate) fn seed_materials(&mut self, mut materials: MaterialCatalog) {
        materials.prepare_for_next_zone();
        self.materials = materials;
    }

    fn asset_walked(&mut self) {
        self.walked += 1;
        if let Some(stage) = self.stage.as_ref() {
            stage.set_completed(self.walked as u64);
        }
    }

    pub(crate) fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.scene_models.set_capture_zone(zone);
        self.materials.set_capture_zone(zone);
        self.xanims.set_capture_zone(zone);
        self.fx.set_capture_zone(zone);
        self.fx_models.set_capture_zone(zone);
        self.tracers.set_capture_zone(zone);
        self.fpv_meshes.set_capture_zone(zone);
        self.world_weapons.set_capture_zone(zone);
    }

    pub(crate) fn set_capture_ns(&mut self, ns: asset_core::FamilyId) {
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

impl AssetSink for ZoneWalkSink {
    fn set_script_strings(&mut self, strings: ScriptStrings) {
        self.fx_models.set_strings(strings);
        self.script_strings = strings;
        self.bodies.set_strings(strings);
        self.fpv_meshes.set_strings(strings);
        self.xanims.set_strings(strings);
        self.weapons.set_strings(strings);
        self.world_weapons.set_strings(strings);
        self.projectile_meshes.set_strings(strings);
    }

    fn begin_assets(&mut self, count: usize) {
        if let Some(stage) = self.stage.as_ref() {
            stage.set_total(count as u64);
        }
    }

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        let loaded = load_asset_at_observed(s, ty, slot, self)?;
        if ty == AssetType::LightDef {
            self.light_def_table += 1;
            if loaded {
                self.light_def_bodies += 1;
            }
        }
        self.asset_walked();
        Ok(())
    }
}

impl AssetLinkSink for ZoneWalkSink {
    fn loaded(
        &mut self,
        stream: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_loaded(stream, ty, slot, insert_slot);
        }
        if ty == AssetType::StructuredDataDef
            && let Some(header) = stream.structured_data_def_set()
        {
            match asset_game::structured_data::capture_iw4_structured_data_def_set(stream, header) {
                Ok((name, schema)) => self.scripts.capture_schema(name, schema),
                Err(error) => diag::warn!(World, "structured-data capture: {error}"),
            }
        }
        self.materials.loaded(stream, ty, slot, insert_slot)?;
        if self.capture_weapons {
            if ty == AssetType::Weapon {
                self.weapons.capture(stream);
            }
            if ty == AssetType::Vehicle {
                self.weapons.capture_vehicle(stream);
            }
        }
        if ty == AssetType::Fx {
            self.fx.note_loaded(slot, insert_slot.map(Into::into));
        }
        if ty == AssetType::Tracer {
            self.tracers.note_loaded(slot, insert_slot);
        }
        if ty == AssetType::XModel && self.region_only {
            if let Some(geometry) = stream.xmodel() {
                self.deferred_models.push(DeferredModel {
                    geometry,
                    slot,
                    insert_slot,
                });
            }
        } else if ty == AssetType::XModel {
            self.map_xmodels.capture(
                stream,
                &self.materials,
                slot,
                insert_slot,
                &self.script_strings,
                &self.phys_presets,
            );
            self.fpv_meshes.capture(stream, &self.materials);
            if self.capture_weapons {
                self.world_weapons.capture(stream, &self.materials);
                self.projectile_meshes
                    .capture_unclassified(stream, &self.materials);
            }
            self.bodies.capture(stream, &self.materials);
            self.models.capture(stream);
            self.fx_models.capture(stream, &self.materials);
            self.xmodel_coll.insert(stream, slot, insert_slot);
        }
        if ty == AssetType::PhysPreset {
            self.phys_presets.capture(stream, slot, insert_slot);
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_alias(ty, slot, target);
        }
        self.materials.alias(ty, slot, target)?;
        if ty == AssetType::Fx {
            self.fx.note_alias(slot, target);
        }
        if ty == AssetType::Tracer {
            self.tracers.note_alias(slot, target);
        }
        if ty == AssetType::XModel {
            if self.region_only {
                self.deferred_aliases.push((slot, target));
            }
            self.map_xmodels.alias(slot, target);
            self.xmodel_coll.alias(slot, target);
            if let Some(&name) = self.xmodel_names.get(&target) {
                self.xmodel_names.insert(slot, name);
            }
        }
        if ty == AssetType::PhysPreset {
            self.phys_presets.alias(slot, target);
        }
        Ok(())
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        self.materials.linked_asset_name(slot)
    }

    fn capture_string_table(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
    ) -> fastfile_iw4::Result<()> {
        if let Some(table) = iw4_string_table(s, header) {
            self.scripts.capture_table(&table);
            if self.capture_weapons && is_cac_table(&table.name) {
                self.stats_tables.insert(table.name.clone(), table);
            }
        }
        Ok(())
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw4::Result<()> {
        self.scripts.capture(name, data, zlib_compressed);
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        self.compass.capture(name, data, zlib_compressed);
        self.script_sound.capture(name, data, zlib_compressed);
        if asset_world::is_createart_source(name) {
            self.createart_name = Some(name.to_owned());
        }
        if let Some(fog) = asset_world::parse_createart_rawfile(name, data, zlib_compressed) {
            self.exp_fog = Some(fog);
            self.createart_name = Some(name.to_owned());
        }
        match asset_world::parse_film_vision_rawfile(name, data, zlib_compressed) {
            Ok(Some(vision)) => {
                self.film_visions
                    .insert(name.replace('\\', "/").to_ascii_lowercase(), Ok(vision));
            }
            Ok(None) => {}
            Err(error) => {
                self.film_visions
                    .insert(name.replace('\\', "/").to_ascii_lowercase(), Err(error));
            }
        }
        Ok(())
    }

    fn capture_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: FxEffectDefGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.fx
            .capture(s, geometry, &self.materials, &self.xmodel_names)
    }

    fn capture_impact_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: fastfile_iw4::FxImpactTableGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.impact_fx.capture(s, geometry, &self.fx)
    }

    fn capture_tracer(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: fastfile_iw4::TracerDefGeometry,
    ) -> fastfile_iw4::Result<()> {
        capture_tracer_named(&mut self.tracers, &self.materials, s, geometry)
    }

    fn capture_fx_glass_def(
        &mut self,
        def_index: usize,
        material: &str,
        material_shattered: &str,
    ) -> fastfile_iw4::Result<()> {
        if self.fx_glass_def_materials.len() <= def_index {
            self.fx_glass_def_materials
                .resize(def_index + 1, (String::new(), String::new()));
        }
        self.fx_glass_def_materials[def_index] =
            (material.to_owned(), material_shattered.to_owned());
        Ok(())
    }

    fn capture_xanim(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: XAnimPartsGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.xanims.capture_xanim(s, geometry)
    }

    fn remember_xmodel_surface_name(&mut self, slot: Ptr, name: Ptr) {
        self.xmodel_surface_names.insert(slot, name);
    }
    fn xmodel_surface_name(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_surface_names.get(&slot).copied()
    }
    fn remember_xmodel_surfaces(&mut self, slot: Ptr, surfaces: Ptr) {
        self.xmodel_surfaces.insert(slot, surfaces);
    }

    fn xmodel_surfaces(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_surfaces.get(&slot).copied()
    }

    fn remember_xmodel_name(&mut self, slot: Ptr, insert_slot: Option<Ptr>, name: Ptr) {
        self.xmodel_names.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.xmodel_names.insert(insert_slot, name);
        }
    }

    fn xmodel_name_ptr(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_names.get(&slot).copied()
    }

    asset_audio::forward_iw4_sound!();
}

impl AssetSink for CommonWalkSink {
    fn begin_assets(&mut self, count: usize) {
        if let Some(stage) = self.stage.as_ref() {
            stage.set_total(count as u64);
        }
    }

    fn set_script_strings(&mut self, strings: ScriptStrings) {
        self.script_strings = strings;
        self.fx_models.set_strings(strings);
        self.fpv_meshes.set_strings(strings);
        self.world_weapons.set_strings(strings);
        self.projectile_meshes.set_strings(strings);
        self.xanims.set_strings(strings);
        self.weapons.set_strings(strings);
    }

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        let loaded = load_asset_at_observed(s, ty, slot, self)?;
        if ty == AssetType::LightDef {
            self.light_def_table += 1;
            if loaded {
                self.light_def_bodies += 1;
            }
        }
        self.asset_walked();
        Ok(())
    }
}

impl AssetLinkSink for CommonWalkSink {
    fn loaded(
        &mut self,
        stream: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_loaded(stream, ty, slot, insert_slot);
        }
        if ty == AssetType::StructuredDataDef
            && let Some(header) = stream.structured_data_def_set()
        {
            match asset_game::structured_data::capture_iw4_structured_data_def_set(stream, header) {
                Ok((name, schema)) => self.scripts.capture_schema(name, schema),
                Err(error) => diag::warn!(World, "structured-data capture: {error}"),
            }
        }
        self.materials.loaded(stream, ty, slot, insert_slot)?;
        if ty == AssetType::Weapon {
            self.weapons.capture(stream);
        }
        if ty == AssetType::Vehicle {
            self.weapons.capture_vehicle(stream);
        }
        if ty == AssetType::Fx {
            self.fx.note_loaded(slot, insert_slot.map(Into::into));
        }
        if ty == AssetType::Tracer {
            self.tracers.note_loaded(slot, insert_slot);
        }
        if ty == AssetType::XModel {
            if let Some(geometry) = stream.xmodel()
                && let Some(name) = geometry.name.and_then(|p| stream.cstr(p).ok())
            {
                let skel = asset_model::capture_xmodel_skel(
                    stream,
                    &self.script_strings,
                    geometry,
                    Some(&self.materials),
                )
                .map(|skel| self.shared_surfaces.share_geometry(skel));
                if let Some(skel) = &skel {
                    self.shared_surfaces.retain(stream, geometry, skel.clone());
                    self.fx_models.capture_shared(skel.clone(), &self.materials);
                    self.fpv_meshes.capture_shared(None, skel, &self.materials);
                    self.world_weapons
                        .capture_shared(None, skel, &self.materials);
                    self.projectile_meshes
                        .capture_shared_unclassified(skel, &self.materials);
                }
                let asset = skel
                    .map(|skel| asset_world::MapXModelSceneAsset::Captured {
                        family: asset_core::FamilyId::IW4,
                        skel,
                    })
                    .unwrap_or(asset_world::MapXModelSceneAsset::Unavailable {
                        reason: "common XModel skeleton capture failed",
                    });
                self.scene_models
                    .insert(asset_world::MapXModelAssetKey(name.to_owned()), asset);
            }
            self.models.capture(stream);
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_alias(ty, slot, target);
        }
        self.materials.alias(ty, slot, target)?;
        if ty == AssetType::Fx {
            self.fx.note_alias(slot, target);
        }
        if ty == AssetType::Tracer {
            self.tracers.note_alias(slot, target);
        }
        if ty == AssetType::XModel
            && let Some(&name) = self.xmodel_names.get(&target)
        {
            self.xmodel_names.insert(slot, name);
        }
        Ok(())
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        self.materials.linked_asset_name(slot)
    }

    fn capture_xanim(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: XAnimPartsGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.xanims.capture_xanim(s, geometry)
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw4::Result<()> {
        self.scripts.capture(name, data, zlib_compressed);
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        self.player_anim_sources
            .capture(self.materials.capture_ns(), name, data, zlib_compressed);
        if let Some(table) =
            asset_game::capture_pen_table(self.runtime_profile, name, data, zlib_compressed)
        {
            self.pen_table = Some(table);
        }
        if let Some(table) = asset_game::capture_lochit_table(name, data, zlib_compressed) {
            self.lochit_table = Some(table);
        }
        match asset_world::parse_film_vision_rawfile(name, data, zlib_compressed) {
            Ok(Some(vision)) => {
                self.film_visions
                    .insert(name.replace('\\', "/").to_ascii_lowercase(), Ok(vision));
            }
            Ok(None) => {}
            Err(error) => {
                self.film_visions
                    .insert(name.replace('\\', "/").to_ascii_lowercase(), Err(error));
            }
        }
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
    ) -> fastfile_iw4::Result<()> {
        if let Some(table) = iw4_string_table(s, header) {
            self.scripts.capture_table(&table);
            if is_cac_table(&table.name) {
                self.keep_stats_table(table);
            }
        }
        Ok(())
    }

    fn capture_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: FxEffectDefGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.fx
            .capture(s, geometry, &self.materials, &self.xmodel_names)
    }

    fn capture_impact_fx(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: fastfile_iw4::FxImpactTableGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.impact_fx.capture(s, geometry, &self.fx)
    }

    fn capture_tracer(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: fastfile_iw4::TracerDefGeometry,
    ) -> fastfile_iw4::Result<()> {
        capture_tracer_named(&mut self.tracers, &self.materials, s, geometry)
    }

    fn remember_xmodel_surface_name(&mut self, slot: Ptr, name: Ptr) {
        self.xmodel_surface_names.insert(slot, name);
    }
    fn xmodel_surface_name(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_surface_names.get(&slot).copied()
    }
    fn remember_xmodel_surfaces(&mut self, slot: Ptr, surfaces: Ptr) {
        self.xmodel_surfaces.insert(slot, surfaces);
    }

    fn xmodel_surfaces(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_surfaces.get(&slot).copied()
    }

    fn remember_xmodel_name(&mut self, slot: Ptr, insert_slot: Option<Ptr>, name: Ptr) {
        self.xmodel_names.insert(slot, name);
        if let Some(insert_slot) = insert_slot {
            self.xmodel_names.insert(insert_slot, name);
        }
    }

    fn xmodel_name_ptr(&self, slot: Ptr) -> Option<Ptr> {
        self.xmodel_names.get(&slot).copied()
    }

    asset_audio::forward_iw4_sound!();
}

#[derive(Default)]
pub(crate) struct MaterialPopulationSink {
    pub xanims: XAnimBuild,
    pub walked: usize,
    pub stage: Option<StageHandle>,
    pub materials: MaterialCatalog,

    pub stats_tables: BTreeMap<String, asset_game::CapturedStringTable>,
    pub scripts: crate::ScriptSources,

    pub sound: Option<asset_audio::ZoneSoundCapture>,
}

impl MaterialPopulationSink {
    pub(crate) fn with_stage(stage: StageHandle) -> Self {
        Self {
            stage: Some(stage),
            ..Default::default()
        }
    }

    pub(crate) fn seed_materials(&mut self, mut materials: MaterialCatalog) {
        materials.prepare_for_next_zone();
        self.materials = materials;
    }

    fn asset_walked(&mut self) {
        self.walked += 1;
        if let Some(stage) = self.stage.as_ref() {
            stage.set_completed(self.walked as u64);
        }
    }

    pub(crate) fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.materials.set_capture_zone(zone);
        self.xanims.set_capture_zone(zone);
    }

    pub(crate) fn set_capture_ns(&mut self, ns: asset_core::FamilyId) {
        self.materials.set_capture_ns(ns);
        self.xanims.set_capture_ns(ns);
    }

    fn keep_stats_table(&mut self, table: Option<asset_game::CapturedStringTable>) {
        if let Some(table) = table {
            self.stats_tables.insert(table.name.clone(), table);
        }
    }
}

impl AssetSink for MaterialPopulationSink {
    fn begin_assets(&mut self, count: usize) {
        if let Some(stage) = self.stage.as_ref() {
            stage.set_total(count as u64);
        }
    }

    fn set_script_strings(&mut self, strings: ScriptStrings) {
        self.xanims.set_strings(strings);
    }

    fn load_asset(
        &mut self,
        s: &mut ZoneStream<'_>,
        _index: usize,
        ty: AssetType,
        slot: Ptr,
    ) -> fastfile_iw4::Result<()> {
        load_asset_at_observed(s, ty, slot, self)?;
        self.asset_walked();
        Ok(())
    }
}

impl AssetLinkSink for MaterialPopulationSink {
    fn loaded(
        &mut self,
        stream: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_loaded(stream, ty, slot, insert_slot);
        }
        if ty == AssetType::StructuredDataDef
            && let Some(header) = stream.structured_data_def_set()
        {
            match asset_game::structured_data::capture_iw4_structured_data_def_set(stream, header) {
                Ok((name, schema)) => self.scripts.capture_schema(name, schema),
                Err(error) => diag::warn!(World, "structured-data capture: {error}"),
            }
        }
        self.materials.loaded(stream, ty, slot, insert_slot)
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.iw4_alias(ty, slot, target);
        }
        self.materials.alias(ty, slot, target)
    }

    fn linked_asset_name(&self, slot: Ptr) -> Option<&str> {
        self.materials.linked_asset_name(slot)
    }

    fn capture_xanim(
        &mut self,
        s: &ZoneStream<'_>,
        geometry: XAnimPartsGeometry,
    ) -> fastfile_iw4::Result<()> {
        self.xanims.capture_xanim(s, geometry)
    }

    fn capture_raw_file(
        &mut self,
        name: &str,
        data: &[u8],
        zlib_compressed: bool,
    ) -> fastfile_iw4::Result<()> {
        if let Some(sound) = self.sound.as_mut() {
            sound.raw_file(name, data, zlib_compressed);
        }
        self.scripts.capture(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_string_table(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
    ) -> fastfile_iw4::Result<()> {
        let table = iw4_string_table(s, header);
        if let Some(table) = &table {
            self.scripts.capture_table(table);
        }
        self.keep_stats_table(table.filter(|t| is_cac_table(&t.name)));
        Ok(())
    }

    asset_audio::forward_iw4_sound!();
}

#[derive(Default)]
pub(crate) struct ModelCensus {
    fpv: ModelTotals,
    soldier: ModelTotals,
    soldier_names: Vec<String>,
    failed: usize,

    walked_names: BTreeSet<String>,

    unclassified_names: BTreeSet<String>,
}

#[derive(Default)]
struct ModelTotals {
    count: usize,
    vertices: usize,
    triangles: usize,
    surfaces: usize,
    first_name: Option<String>,
}

impl ModelCensus {
    pub(crate) fn capture(&mut self, stream: &ZoneStream<'_>) {
        let Some(geometry) = stream.xmodel() else {
            self.failed += 1;
            return;
        };
        let Some(name) = geometry.name.and_then(|ptr| stream.cstr(ptr).ok()) else {
            self.failed += 1;
            return;
        };
        self.walked_names.insert(name.to_owned());
        let Some(kind) = model_kind(name) else {
            self.unclassified_names.insert(name.to_owned());
            return;
        };
        let Ok(model) = build_xmodel_mesh(stream, geometry, None) else {
            self.failed += 1;
            return;
        };
        let totals = match kind {
            ModelKind::Fpv => &mut self.fpv,
            ModelKind::Soldier => &mut self.soldier,
            ModelKind::WorldWeapon => return,
        };
        totals.count += 1;
        totals.vertices += model.vertices;
        totals.triangles += model.triangles;
        totals.surfaces += model.surface_count();
        totals.first_name.get_or_insert(model.name.clone());
        if kind == ModelKind::Soldier {
            self.soldier_names.push(model.name);
        }
    }

    pub(crate) fn report(&self, source: &str) -> Vec<String> {
        let mut report = vec![
            format!(
                "{source} FPV models: {} ({} vertices, {} triangles; first: {})",
                self.fpv.count,
                self.fpv.vertices,
                self.fpv.triangles,
                self.fpv.first_name.as_deref().unwrap_or("none")
            ),
            format!(
                "{source} soldier models: {} ({} vertices, {} triangles; first: {})",
                self.soldier.count,
                self.soldier.vertices,
                self.soldier.triangles,
                self.soldier.first_name.as_deref().unwrap_or("none")
            ),
            format!("{source} selected model decode failures: {}", self.failed),
            self.walk_census().report_line(source),
        ];
        if source == "map" {
            let kits = soldier_kits(&self.soldier_names);
            report.push(format!(
                "map soldier kits: allies={} axis={}",
                kit_name(kits.kit(false)),
                kit_name(kits.kit(true))
            ));
        }
        report
    }

    pub(crate) fn walk_census(&self) -> PreparedXModelWalkCensus {
        PreparedXModelWalkCensus {
            walked_n: self.walked_names.len(),
            unclassified_n: self.unclassified_names.len(),
            projectile_names: self
                .walked_names
                .iter()
                .filter(|name| name.starts_with("projectile_"))
                .cloned()
                .collect(),
        }
    }
}

fn capture_tracer_named(
    tracers: &mut asset_game::TracerCatalog,
    materials: &asset_material::MaterialCatalog,
    s: &ZoneStream<'_>,
    geometry: fastfile_iw4::TracerDefGeometry,
) -> fastfile_iw4::Result<()> {
    tracers.capture(s, geometry)?;
    if let Some(name) = tracer_material_name(s, materials, geometry) {
        tracers.bind_last_material(name);
    }
    if tracer_material_alias(s, geometry).is_some() {
        tracers.note_last_material_alias();
    }
    Ok(())
}

fn tracer_material_name(
    s: &ZoneStream<'_>,
    materials: &asset_material::MaterialCatalog,
    geometry: fastfile_iw4::TracerDefGeometry,
) -> Option<String> {
    if let Some(ptr) = geometry.material_name {
        let name = s.cstr(ptr).unwrap_or("");
        if !name.is_empty() {
            return Some(name.to_owned());
        }
    }
    let slot = geometry.material_slot?;
    if let Some(name) = materials
        .material_index(slot)
        .and_then(|i| materials.materials.get(i.get()))
        .map(|m| m.name.to_string())
        .filter(|n| !n.is_empty())
    {
        return Some(name);
    }

    let Ok(fastfile_iw4::ZonePtr::Offset(target)) = s.ptr_at(slot, 0) else {
        return None;
    };
    if let Some(name) = materials
        .material_index(target)
        .and_then(|i| materials.materials.get(i.get()))
        .map(|m| m.name.to_string())
        .filter(|n| !n.is_empty())
    {
        return Some(name);
    }
    let body = s.resolve_alias(target);
    if body != target
        && let Some(name) = materials
            .material_index(body)
            .and_then(|i| materials.materials.get(i.get()))
            .map(|m| m.name.to_string())
            .filter(|n| !n.is_empty())
    {
        return Some(name);
    }
    read_material_name_at_body(s, body)
}

fn tracer_material_alias(
    s: &ZoneStream<'_>,
    geometry: fastfile_iw4::TracerDefGeometry,
) -> Option<fastfile_iw4::Ptr> {
    let slot = geometry.material_slot?;
    match s.ptr_at(slot, 0) {
        Ok(fastfile_iw4::ZonePtr::Offset(target)) => Some(target),
        _ => None,
    }
}

fn read_material_name_at_body(s: &ZoneStream<'_>, body: fastfile_iw4::Ptr) -> Option<String> {
    match s.ptr_at(body, 0).ok()? {
        fastfile_iw4::ZonePtr::Offset(name) => s
            .cstr(s.resolve_alias(name))
            .ok()
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
        _ => s
            .cstr(body)
            .ok()
            .filter(|n| !n.is_empty())
            .map(str::to_owned),
    }
}

fn kit_name(kit: Option<&SoldierKit>) -> String {
    match kit {
        Some(kit) => match &kit.head {
            Some(head) => format!("{} + {head}", kit.body),
            None => kit.body.clone(),
        },
        None => "none".into(),
    }
}
