use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::world::world_t6::Reader;
use ext_t6_fastfile::{AssetType, LoadedAsset, Ptr, ZoneLoad};
use xmodel_runtime::{T5DestructibleDef, T5DestructiblePiece, T5DestructibleStage};

pub(super) fn capture(load: &ZoneLoad) -> Result<BTreeMap<String, Arc<T5DestructibleDef>>, String> {
    load.assets
        .iter()
        .filter(|asset| asset.ty == AssetType::DestructibleDef)
        .map(|asset| {
            let reader = Reader(load);
            let name =
                super::header_str(load, &asset.header, 0).ok_or("T6 destructible name missing")?;
            let model = asset
                .field(4)
                .and_then(|index| load.assets.get(index))
                .and_then(|model| super::header_str(load, &model.header, 0))
                .ok_or_else(|| format!("T6 destructible {name}: model missing"))?;
            let count = Reader::word(&asset.header, 12)? as i32;
            if count <= 0 {
                return Err(format!("T6 destructible {name}: invalid piece count"));
            }
            let address = Reader::ptr(&asset.header, 16)?;
            reader.bytes(
                address,
                (count as usize)
                    .checked_mul(312)
                    .ok_or("T6 destructible pieces overflow")?,
            )?;
            let mut pieces = Vec::with_capacity(count as usize);
            for index in 0..count as u32 {
                let p = address.at(index * 312);
                let mut stages = Vec::with_capacity(5);
                for stage in 0..5 {
                    let st = p.at(stage * 48);
                    let bone = reader.u16(st)?;
                    let show_bone = if bone == 0 {
                        None
                    } else {
                        Some(
                            load.script_string(bone)
                                .ok_or("T6 destructible bone missing")?
                                .to_owned(),
                        )
                    };
                    let mut spawn_models = [None, None, None];
                    for (slot, model) in spawn_models.iter_mut().enumerate() {
                        *model = asset_name(load, asset, st.at(32 + slot as u32 * 4))?;
                    }
                    stages.push(T5DestructibleStage {
                        show_bone,
                        break_health: reader.f32(st.at(4))?,
                        max_time: reader.f32(st.at(8))?,
                        flags: reader.u32(st.at(12))?,
                        break_effect: asset_name(load, asset, st.at(16))?,
                        break_effect_looping: load
                            .asset_in(asset, st.at(16))
                            .and_then(|effect| super::header_u32(&effect.header, 20))
                            .is_some_and(|duration| duration == i32::MAX as u32),
                        break_sound: text(load, st.at(20))?.map(|name| format!("t6:{name}")),
                        break_notify: text(load, st.at(24))?,
                        loop_sound: text(load, st.at(28))?.map(|name| format!("t6:{name}")),
                        spawn_models,
                        has_phys_preset: reader.u32(st.at(44))? != 0,
                    });
                }
                let mut hide_bones = [0; 5];
                for (word, bits) in hide_bones.iter_mut().enumerate() {
                    *bits = reader.u32(p.at(292 + word as u32 * 4))?;
                }
                let launch = if let Some(c) = load
                    .blocks
                    .ptr_at(p.at(268))
                    .map_err(|e| format!("T6 destructible constraints: {e:?}"))?
                {
                    let data = c.at(8);
                    if reader.u32(c.at(4))? > 0 && reader.u32(data.at(4))? == 7 {
                        Some((reader.xyz(data.at(116))?, reader.f32(data.at(112))?))
                    } else {
                        None
                    }
                } else {
                    None
                };
                pieces.push(T5DestructiblePiece {
                    stages: stages.try_into().unwrap(),
                    parent_piece: reader.bytes(p.at(240), 1)?[0],
                    parent_damage_percent: reader.f32(p.at(244))?,
                    bullet_damage_scale: reader.f32(p.at(248))?,
                    explosive_damage_scale: reader.f32(p.at(252))?,
                    health: reader.u32(p.at(272))? as i32,
                    launch,
                    hide_bones,
                });
            }
            Ok((
                name.to_owned(),
                Arc::new(T5DestructibleDef {
                    name: name.to_owned(),
                    model: model.to_owned(),
                    pieces,
                    client_only: Reader::word(&asset.header, 20)? != 0,
                }),
            ))
        })
        .collect()
}

pub(super) fn model_names(
    definitions: &BTreeMap<String, Arc<T5DestructibleDef>>,
) -> BTreeSet<&str> {
    definitions
        .values()
        .flat_map(|def| {
            std::iter::once(def.model.as_str()).chain(def.pieces.iter().flat_map(|piece| {
                piece
                    .stages
                    .iter()
                    .flat_map(|stage| stage.spawn_models.iter().filter_map(Option::as_deref))
            }))
        })
        .collect()
}

pub(super) fn effect_names(load: &ZoneLoad) -> Result<BTreeSet<String>, String> {
    Ok(capture(load)?
        .values()
        .flat_map(|def| {
            def.pieces.iter().flat_map(|piece| {
                piece
                    .stages
                    .iter()
                    .filter_map(|stage| stage.break_effect.clone())
            })
        })
        .collect())
}

fn asset_name(load: &ZoneLoad, owner: &LoadedAsset, slot: Ptr) -> Result<Option<String>, String> {
    if Reader(load).u32(slot)? == 0 {
        return Ok(None);
    }
    load.asset_in(owner, slot)
        .and_then(|asset| super::header_str(load, &asset.header, 0))
        .map(|name| Some(name.to_owned()))
        .ok_or_else(|| "T6 destructible asset reference missing".into())
}

fn text(load: &ZoneLoad, slot: Ptr) -> Result<Option<String>, String> {
    load.blocks
        .ptr_at(slot)
        .map_err(|e| format!("T6 destructible string: {e:?}"))?
        .map(|p| Reader(load).text(p).map(str::to_owned))
        .transpose()
}
