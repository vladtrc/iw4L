use crate::world::entities::{
    flag_descriptors_t5, map_script_structs_t5, map_use_triggers_t5,
    script_brush_model_placements_t5, script_model_placements_t5,
};
use crate::world::models::build_t5_static_model_instances;
use asset_material::{AssetPointerIdentity, MaterialCatalog};
use asset_world::{
    MapXModelAssetKey, MapXModelSceneAsset, PreparedMapModels, StaticModelDrawError,
};
use assets::{
    MapXModelCatalog, T5DestructibleInitial, link_model_placements, normalize_bomb_sites,
};
fn address(p: ext_t5_fastfile::Ptr) -> AssetPointerIdentity {
    AssetPointerIdentity {
        block: p.block,
        offset: p.offset,
    }
}
pub trait NativeMapXModelCatalog {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
        strings: &ext_t5_fastfile::ScriptStrings,
    );
    fn capture_t5_destructible(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        header: ext_t5_fastfile::Ptr,
        strings: &ext_t5_fastfile::ScriptStrings,
        fx: &asset_game::FxCatalog,
    ) -> ext_t5_fastfile::Result<()>;
    fn attach_t5_clip_models(
        &self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::ClipMapGeometry,
        clip: &mut asset_world::ClipCollision,
    ) -> Result<(), asset_world::ClipCollisionError>;
}

impl NativeMapXModelCatalog for MapXModelCatalog {
    fn capture_t5(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        materials: &MaterialCatalog,
        slot: ext_t5_fastfile::Ptr,
        insert_slot: Option<ext_t5_fastfile::Ptr>,
        strings: &ext_t5_fastfile::ScriptStrings,
    ) {
        let Some(geometry) = stream.latest_xmodel() else {
            self.note_failed_mesh();
            return;
        };
        let Ok(mesh) =
            crate::world::models::build_t5_xmodel_mesh(stream, geometry, Some(materials))
        else {
            self.note_failed_mesh();
            return;
        };
        let scene_key = MapXModelAssetKey(mesh.name.clone());
        let scene_asset =
            crate::model::capture::capture_xmodel_skel_t5(stream, strings, geometry, materials)
                .map(|skel| MapXModelSceneAsset::Captured {
                    family: crate::ID,
                    skel: std::sync::Arc::new(skel),
                })
                .unwrap_or(MapXModelSceneAsset::Unavailable {
                    reason: "T5 XModel scene skeleton capture failed",
                });
        self.insert_scene_asset(scene_key, scene_asset);
        self.insert_mesh(mesh, address(slot), insert_slot.map(address));
    }
    fn capture_t5_destructible(
        &mut self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        header: ext_t5_fastfile::Ptr,
        strings: &ext_t5_fastfile::ScriptStrings,
        fx: &asset_game::FxCatalog,
    ) -> ext_t5_fastfile::Result<()> {
        use ext_t5_fastfile::ZonePtr;
        let ZonePtr::Offset(name) = stream.ptr_at(header, 0)? else {
            return Ok(());
        };
        let name = stream.cstr(stream.resolve_alias(name))?.to_owned();
        let initial = (|| {
            let pristine = stream.ptr_at(header, 8).ok()? != ZonePtr::Null;
            let model_slot = header.at(if pristine { 8 } else { 4 });
            let model = self
                .name_at_slot(AssetPointerIdentity {
                    block: model_slot.block,
                    offset: model_slot.offset,
                })?
                .to_owned();
            let read_string = |p, offset| -> Option<Option<String>> {
                match stream.ptr_at(p, offset).ok()? {
                    ZonePtr::Null => Some(None),
                    ZonePtr::Offset(p) => {
                        Some(Some(stream.cstr(stream.resolve_alias(p)).ok()?.to_owned()))
                    }
                    _ => None,
                }
            };
            let count = usize::try_from(stream.i32_at(header, 12).ok()?).ok()?;
            let ZonePtr::Offset(pieces_ptr) = stream.ptr_at(header, 16).ok()? else {
                return None;
            };
            let pieces_ptr = stream.resolve_alias(pieces_ptr);
            let mut pieces = Vec::with_capacity(count);
            for i in 0..count {
                let p = pieces_ptr.at(i * 0x138);
                let mut stages = Vec::with_capacity(5);
                for j in 0..5 {
                    let st = p.at(j * 0x30);
                    let bone = stream.u16_at(st, 0).ok()?;
                    let break_effect = if stream.ptr_at(st, 16).ok()? == ZonePtr::Null {
                        None
                    } else {
                        Some(
                            fx.name_at_slot(AssetPointerIdentity {
                                block: st.block,
                                offset: st.offset + 16,
                            })?
                            .to_owned(),
                        )
                    };
                    let mut spawn_models = [None, None, None];
                    for (k, value) in spawn_models.iter_mut().enumerate() {
                        if stream.ptr_at(st, 32 + k * 4).ok()? != ZonePtr::Null {
                            *value = Some(
                                self.name_at_slot(AssetPointerIdentity {
                                    block: st.block,
                                    offset: st.at(32 + k * 4).offset,
                                })?
                                .to_owned(),
                            );
                        }
                    }
                    stages.push(xmodel_runtime::T5DestructibleStage {
                        show_bone: if bone == 0 {
                            None
                        } else {
                            Some(strings.get(stream, bone)?.to_owned())
                        },
                        break_health: stream.f32_at(st, 4).ok()?,
                        max_time: stream.f32_at(st, 8).ok()?,
                        flags: stream.u32_at(st, 12).ok()?,
                        break_effect_looping: break_effect
                            .as_deref()
                            .and_then(|name| fx.get_in(crate::ID, name))
                            .is_some_and(|effect| effect.view.msec_looping_life == i32::MAX),
                        break_effect,
                        break_sound: read_string(st, 20)?.map(|name| format!("t5:{name}")),
                        break_notify: read_string(st, 24)?,
                        loop_sound: read_string(st, 28)?.map(|name| format!("t5:{name}")),
                        has_phys_preset: stream.ptr_at(st, 44).ok()? != ZonePtr::Null,
                        spawn_models,
                    });
                }
                let mut hide_bones = [0; 5];
                for (j, word) in hide_bones.iter_mut().enumerate() {
                    *word = stream.u32_at(p, 0x124 + j * 4).ok()?;
                }
                pieces.push(xmodel_runtime::T5DestructiblePiece {
                    stages: stages.try_into().ok()?,
                    parent_piece: stream.slice_at(p, 0xf0, 1).ok()?[0],
                    parent_damage_percent: stream.f32_at(p, 0xf4).ok()?,
                    bullet_damage_scale: stream.f32_at(p, 0xf8).ok()?,
                    explosive_damage_scale: stream.f32_at(p, 0xfc).ok()?,
                    health: stream.i32_at(p, 0x110).ok()?,
                    launch: match stream.ptr_at(p, 0x10c).ok()? {
                        ZonePtr::Offset(c) => {
                            let c = stream.resolve_alias(c);
                            let data = c.at(8);
                            if stream.u32_at(c, 4).ok()? > 0 && stream.i32_at(data, 4).ok()? == 7 {
                                Some((
                                    [
                                        stream.f32_at(data, 116).ok()?,
                                        stream.f32_at(data, 120).ok()?,
                                        stream.f32_at(data, 124).ok()?,
                                    ],
                                    stream.f32_at(data, 112).ok()?,
                                ))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    },
                    hide_bones,
                });
            }
            let base_slot = header.at(4);
            let definition = std::sync::Arc::new(xmodel_runtime::T5DestructibleDef {
                name: name.clone(),
                model: self
                    .name_at_slot(AssetPointerIdentity {
                        block: base_slot.block,
                        offset: base_slot.offset,
                    })?
                    .to_owned(),
                pieces,
                client_only: stream.i32_at(header, 20).ok()? != 0,
            });
            let hide_parts = if pristine {
                xmodel_runtime::HidePartBits::from_words([0; 6])
            } else {
                let MapXModelSceneAsset::Captured { family, skel } = self.scene_asset(&model)?
                else {
                    return None;
                };
                if *family != crate::ID {
                    return None;
                }
                let health: Vec<_> = definition.pieces.iter().map(|p| p.health as i16).collect();
                definition.hide_parts(&health, &skel.bone_names)
            };
            Some(T5DestructibleInitial {
                definition,
                model: MapXModelAssetKey(model),
                hide_parts,
            })
        })()
        .ok_or("T5 DestructibleDef initial model/parts capture failed");
        self.insert_destructible_initial(name, initial);
        Ok(())
    }
    fn attach_t5_clip_models(
        &self,
        stream: &ext_t5_fastfile::ZoneStream<'_>,
        geometry: ext_t5_fastfile::ClipMapGeometry,
        clip: &mut asset_world::ClipCollision,
    ) -> Result<(), asset_world::ClipCollisionError> {
        use asset_world::ClipCollisionError::{MissingTables, Truncated};
        let Some(rows) = geometry.static_models else {
            return if geometry.static_model_count == 0 {
                Ok(())
            } else {
                Err(MissingTables)
            };
        };
        for index in 0..geometry.static_model_count {
            let row = rows.at(index * ext_t5_fastfile::size::C_STATIC_MODEL);
            let slot = row.at(ext_t5_fastfile::size::C_STATIC_MODEL_XMODEL_OFF);
            let name = self
                .name_at_slot(AssetPointerIdentity {
                    block: slot.block,
                    offset: slot.offset,
                })
                .ok_or(MissingTables)?;
            let Some(MapXModelSceneAsset::Captured { family, skel }) = self.scene_asset(name)
            else {
                return Err(MissingTables);
            };
            if *family != crate::ID {
                return Err(MissingTables);
            }
            let read = |offset| stream.f32_at(row, offset).map_err(|_| Truncated);
            let origin = [read(8)?, read(12)?, read(16)?];
            let mut inv_scaled_axis = [[0.0; 3]; 3];
            let mut bounds_mid = [0.0; 3];
            let mut bounds_half = [0.0; 3];

            for axis in 0..3 {
                for (column, value) in inv_scaled_axis[axis].iter_mut().enumerate() {
                    *value = read(20 + (axis * 3 + column) * 4)?;
                }
                let min = read(56 + axis * 4)?;
                let max = read(68 + axis * 4)?;
                if !min.is_finite() || !max.is_finite() || min > max {
                    return Err(Truncated);
                }
                bounds_mid[axis] = (min + max) * 0.5;
                bounds_half[axis] = (max - min) * 0.5;
            }
            let coll = clipmap_iw4::XModelColl {
                coll_lod: skel.coll_lod,
                contents: skel.contents.ok_or(MissingTables)?,
                surfs: skel
                    .coll_surfs
                    .iter()
                    .map(|surf| clipmap_iw4::XModelCollSurf {
                        midpoint: surf.midpoint,
                        half_size: surf.half_size,
                        bone_idx: i32::from(surf.bone),
                        contents: surf.contents,
                        surf_flags: surf.surf_flags,
                        tris: surf
                            .tris
                            .iter()
                            .map(|tri| clipmap_iw4::XModelCollTri {
                                plane: tri.plane,
                                svec: tri.svec,
                                tvec: tri.tvec,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            clip.static_models.push(asset_world::ClipPlacedStaticModel {
                index: index as u32,
                name: name.to_owned(),
                model: clipmap_iw4::ClipStaticModel {
                    origin,
                    inv_scaled_axis,
                    bounds_mid,
                    bounds_half,
                    coll,
                },
            });
        }
        Ok(())
    }
}
pub fn build_t5_static_model_draw(
    stream: &ext_t5_fastfile::ZoneStream<'_>,
    geometry: ext_t5_fastfile::GfxWorldGeometry,
    catalog: MapXModelCatalog,
) -> PreparedMapModels {
    let placements = build_t5_static_model_instances(stream, geometry).map_err(|_| {
        StaticModelDrawError::InstancesUnavailable {
            smodel_count: geometry.smodel_count,
        }
    });
    let mut scripts = script_model_placements_t5(stream);
    let mut brushes = script_brush_model_placements_t5(stream);
    normalize_bomb_sites(
        &mut scripts,
        &mut brushes,
        &map_use_triggers_t5(stream),
        "bombzone_dem",
    );
    let descriptors = flag_descriptors_t5(stream);
    let structs = map_script_structs_t5(stream);
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
