use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use bevy::prelude::Image;

use super::{CommonCensus, LoadedWorld, MaterialPopulation, ZoneLane};
use crate::lane_capability::{LaneStatus, PreparedCapability};
use asset_core::ZoneGame;
use asset_transport::ZoneImage;
use asset_transport::progress::{LoadProgress, StageId};
use asset_world::WorldDrawPolicy;

/// A T6 model a captured weapon shows, with its surfaces' material names;
/// `surface_materials` in the skeleton is filled once those materials exist.
pub struct T6ModelCapture {
    pub skel: asset_model::ModelSkel,
    pub surface_materials: Vec<Option<String>>,
    /// First-person (`_view`) rather than world model.
    pub view: bool,
    /// The first-person arms rather than a gun: its materials borrow the
    /// technique sets of the stand-in's arms, not of its gun.
    pub hands: bool,
    /// The IW4 weapon whose materials lend these their technique sets.
    pub stand_in: &'static str,
}

/// A T6 material's texels, decoded from the image packages.
pub struct T6MaterialCapture {
    pub color: Option<(String, Arc<Image>)>,
    pub normal: Option<(String, Arc<Image>)>,
    pub specular: Option<(String, Arc<Image>)>,
    /// The material as its own technique set draws it.
    pub native: Option<T6NativeMaterial>,
}

/// A T6 material's technique set, textures (as T6 shaders read them) and
/// constants.
pub struct T6NativeMaterial {
    pub technique_set: String,
    pub textures: Vec<asset_material::t6_techset::T6Texture>,
    pub constants: Vec<asset_material::MaterialConstant>,
    /// The first load-bits word of the material's `lit` state: its blend
    /// and alpha test, laid out as IW4's.
    pub lit_state: Option<u32>,
    /// Its `emissive` state: an optic's reticle draws only emissive.
    pub emissive_state: Option<u32>,
}

#[derive(Default)]
pub struct T6Content {
    pub models: Vec<T6ModelCapture>,
    pub materials: BTreeMap<String, T6MaterialCapture>,
    /// The weapon aliases the sound walk read and deposited.
    pub sound_names: std::collections::BTreeSet<String>,
    /// The first-person animations of the captured weapons, in the IW4
    /// namespace where T6 weapons resolve their content, named with
    /// [`asset_game::T6_XANIM_PREFIX`].
    pub xanims: asset_anim::XAnimBuild,
    /// The sound aliases those animations name on their notetracks
    /// (`sndnt#fly_an94_mag_out`).
    pub note_sounds: std::collections::BTreeSet<String>,
    /// The first-person arms T6 weapons are held in, when they were read.
    pub hands: Option<String>,
    /// The technique sets the captured materials draw with, by name.
    pub techsets: BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    /// The knife and swings a gun without melee clips borrows.
    pub melee: Option<asset_game::T6Melee>,
    /// The effects [`asset_game::T6_EFFECTS`] names, and the materials
    /// their sprites draw with.
    pub fx: Vec<asset_game::T6FxCapture>,
    pub fx_materials: BTreeMap<String, T6MaterialCapture>,
    pub report: Vec<String>,
}

/// [`asset_game::T6_MELEE_WEAPON`]'s first-person knife and its melee and
/// charged-melee clips, named as the captured clips are.
fn melee_weapon(load: &fastfile_t6::ZoneLoad) -> Option<asset_game::T6Melee> {
    use fastfile_t6::weapon::{WeaponView, def, weap_anim};
    let weapon = load
        .assets
        .iter()
        .filter_map(|asset| WeaponView::new(load, asset))
        .find(|weapon| weapon.name() == Some(asset_game::T6_MELEE_WEAPON))?;
    let clip = |slot: usize| {
        weapon
            .xanim(slot as u32)
            .filter(|name| !name.is_empty())
            .map(|name| {
                format!(
                    "{}{}",
                    asset_game::T6_XANIM_PREFIX,
                    name.to_ascii_lowercase()
                )
            })
    };
    Some(asset_game::T6Melee {
        knife: asset_game::t6_model_name(weapon.def_asset_array_name(def::GUN_XMODEL, 0)?),
        melee: clip(weap_anim::MELEE)?,
        charge: clip(weap_anim::MELEE_CHARGE),
    })
}

/// The faction zone beside `common_mp` whose arms hold every T6 weapon,
/// and those arms. T6 weapons name no arms of their own; the player's
/// faction supplies them.
const HANDS_ZONE: &str = "faction_seals_mp.ff";
const HANDS_MODEL: &str = "c_usa_mp_seal6_longsleeve_viewhands";

fn hands_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let path = common.with_file_name(HANDS_ZONE);
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 hands: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 hands: {} walk: {error:?}", path.display()));
    }
    Some(load)
}

/// The notetrack names of a T6 `XAnimParts` (`notifyCount` at 34, the
/// `{ u16 name; float time; }` array at 96).
fn xanim_notes<'z>(load: &'z fastfile_t6::ZoneLoad, header: &[u8]) -> Vec<&'z str> {
    let count = u32::from(header.get(34).copied().unwrap_or(0));
    let Some(raw) = header_u32(header, 96).filter(|&raw| raw != 0 && raw < 0xFFFF_FFFE) else {
        return Vec::new();
    };
    let e = raw - 1;
    let notify = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    (0..count)
        .filter_map(|i| {
            let id = load.blocks.bytes(notify.at(8 * i), 2).ok()?;
            load.script_string(u16::from_le_bytes([id[0], id[1]]))
        })
        .collect()
}

/// The animations every weapon with an IW4 stand-in names, and the sound
/// aliases on their notetracks. A clip `load` lacks is looked for in
/// `others` (the melee knife's swings ship outside `common_mp`).
fn capture_xanims(
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    content: &mut T6Content,
) {
    use fastfile_t6::weapon::WeaponView;
    let mut wanted = std::collections::BTreeSet::new();
    for asset in &load.assets {
        let Some(weapon) = WeaponView::new(load, asset) else {
            continue;
        };
        if weapon.name().is_some_and(|name| {
            asset_game::t6_stand_in_for(name).is_some() || name == asset_game::T6_MELEE_WEAPON
        }) {
            wanted.extend(asset_game::t6_weapon_xanim_names(weapon));
            wanted.extend(asset_game::t6_attachment_xanim_names(weapon));
        }
    }
    let (total, mut captured) = (wanted.len(), 0usize);
    let assets = std::iter::once(load)
        .chain(others)
        .flat_map(|load| load.assets.iter().map(move |asset| (load, asset)));
    for (load, asset) in assets {
        if asset.ty != fastfile_t6::AssetType::XAnimParts
            || !header_str(load, &asset.header, 0)
                .is_some_and(|name| wanted.remove(&name.to_ascii_lowercase()))
        {
            continue;
        }
        if content.xanims.capture_xanim_t6(
            asset_core::AssetNamespace::Iw4,
            asset_game::T6_XANIM_PREFIX,
            load,
            asset,
        ) {
            captured += 1;
            content.note_sounds.extend(
                xanim_notes(load, &asset.header)
                    .into_iter()
                    .filter_map(|note| {
                        asset_game::t5_inline_note_alias(note, asset_game::T5_NOTE_SOUND_PREFIX)
                    })
                    .map(str::to_owned),
            );
        }
    }
    content.report.push(format!(
        "t6 xanims: {captured} of {total} weapon animations captured; {} notetrack sounds",
        content.note_sounds.len()
    ));
}

/// Reads the aliases the captured weapons name and deposits them as this
/// zone's sound source, through `claim` (taken when the walk began).
fn capture_sounds(
    path: &Path,
    load: &fastfile_t6::ZoneLoad,
    claim: Option<asset_audio::ZoneSoundCapture>,
    content: &mut T6Content,
) {
    let mut names = std::collections::BTreeSet::new();
    for asset in &load.assets {
        let Some(weapon) = fastfile_t6::weapon::WeaponView::new(load, asset) else {
            continue;
        };
        if weapon
            .name()
            .and_then(asset_game::t6_stand_in_for)
            .is_some()
        {
            names.extend(asset_game::t6_weapon_sound_names(weapon));
            names.extend(asset_game::t6_attachment_sound_names(weapon));
        }
    }
    names.extend(content.note_sounds.iter().cloned());
    names.extend(
        asset_game::T6_EQUIPMENT_SOUNDS
            .iter()
            .map(|&name| name.to_owned()),
    );
    let (banks, mut report) = asset_audio::t6_sound_banks(path);
    let foley = foley_zone(path, &mut report);
    let loads: Vec<&fastfile_t6::ZoneLoad> = std::iter::once(load).chain(&foley).collect();
    let (catalog, filled, gaps) =
        asset_audio::capture_t6_sounds(path, &loads, &banks, names.iter().map(String::as_str));
    report.push(format!(
        "t6 sounds: {} of {} weapon aliases read (+{} secondary layers, {} audio assets) from {} sound banks; {} gaps",
        filled.iter().filter(|name| names.contains(*name)).count(),
        names.len(),
        filled.iter().filter(|name| !names.contains(*name)).count(),
        catalog.loaded.len(),
        banks.len(),
        gaps.len()
    ));
    report.extend(gaps.into_iter().take(32));
    if let Some(mut capture) = claim {
        capture.set_t6(catalog);
        capture.deposit(Ok(()));
    } else {
        report.push("t6 sounds: common_mp sound source already claimed".to_owned());
    }
    content.sound_names = filled.into_iter().collect();
    content.report.extend(report);
}

/// `patch_mp` beside `common_mp`: it defines models `common_mp` only
/// references (`,t6_wpn_grenade_frag_view`).
fn patch_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let path = common.with_file_name("patch_mp.ff");
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 content: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
    if let Err(error) = walked {
        report.push(format!("t6 content: {} walk: {error:?}", path.display()));
    }
    Some(load)
}

/// The sound banks of the smallest map zone next to `common_mp`, walked no
/// further than its first `SndBank`. T6 keeps the generic weapon foley
/// (raise, put away, dry fire, gear plant) in every map's bank
/// (`mpl_<map>.all`), not in `mpl_common.all`.
fn foley_zone(common: &Path, report: &mut Vec<String>) -> Option<fastfile_t6::ZoneLoad> {
    let dir = common.parent()?;
    let smallest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("mp_") && name.ends_with(".ff")
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.len(), entry.path())))
        .min()
        .map(|(_, path)| path);
    let Some(path) = smallest else {
        report.push(format!(
            "t6 sounds: no map zone in {} for weapon foley",
            dir.display()
        ));
        return None;
    };
    let image = match asset_transport::open_t6_zone(&path) {
        Ok(image) => image,
        Err(error) => {
            report.push(format!("t6 sounds: {}: {error:?}", path.display()));
            return None;
        }
    };
    let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, ty| {
        ty != fastfile_t6::AssetType::SoundBank
    });
    if let Err(error) = walked {
        report.push(format!("t6 sounds: {} walk: {error:?}", path.display()));
    }
    report.push(format!("t6 sounds: weapon foley from {}", path.display()));
    Some(load)
}

/// T6 contributes weapons only: its zones are walked for their weapon
/// definitions, which borrow IW4 stand-ins for everything drawn or heard.
/// T6 worlds, materials (DX11), models and sound banks are not read.
pub struct T6Lane;

impl T6Lane {
    pub const GAME: ZoneGame = ZoneGame::T6;
    pub const CAPABILITIES: &'static [(PreparedCapability, LaneStatus)] = &[
        (PreparedCapability::Envelope, LaneStatus::SupportedPopulated),
        (
            PreparedCapability::PreparedWorld,
            LaneStatus::MissingDecoder,
        ),
        (
            PreparedCapability::CollisionSpawns,
            LaneStatus::MissingDecoder,
        ),
        (
            PreparedCapability::WeaponCatalog,
            LaneStatus::SupportedPopulated,
        ),
        (PreparedCapability::BodySkeleton, LaneStatus::MissingDecoder),
        (
            PreparedCapability::PlayableFfa,
            LaneStatus::UnsupportedByRuntimeProfile,
        ),
    ];
}

fn schema() -> Result<&'static fastfile_t6::schema::Schema, String> {
    static SCHEMA: OnceLock<Result<fastfile_t6::schema::Schema, String>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| fastfile_t6::schema::parse().map_err(|e| format!("T6 load plan: {e:?}")))
        .as_ref()
        .map_err(Clone::clone)
}

/// `MaterialTextureDef::nameHash` of the `colorMap` and `normalMap` samplers.
const COLOR_MAP_HASH: u32 = 0xa0ab_1041;
const NORMAL_MAP_HASH: u32 = 0x59d3_0d0f;
/// The samplers a material's colour, normal and specular texels come from.
/// Weapons name them `colorMap`, `normalMap` and `specularMap`; the
/// first-person arms' shaders give them names of their own (`f039ec2d`,
/// `942cbff0`, `8c297e80`).
const COLOR_SAMPLERS: [u32; 2] = [COLOR_MAP_HASH, 0xf039_ec2d];
const NORMAL_SAMPLERS: [u32; 2] = [NORMAL_MAP_HASH, 0x942c_bff0];
/// T6 specular maps are coloured, with gloss in alpha, as IW4's are. Its
/// colour maps are dark — the metal reads through this colour — so without
/// it a gun drawn with an IW4 shader is near black.
const SPECULAR_SAMPLERS: [u32; 2] = [0x34ec_ccb3, 0x8c29_7e80];
const MATERIAL_TEXTURE_COUNT: usize = 84;
const MATERIAL_TEXTURE_TABLE: usize = 96;
const MATERIAL_TEXTURE_DEF: u32 = 16;
const IMAGE_STREAMED_PART_HASH: usize = 40;
const IMAGE_STREAMED_PART_COUNT: usize = 60;
const IMAGE_NAME: usize = 72;
const IMAGE_HASH: usize = 76;

fn header_u32(header: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(header.get(at..at + 4)?.try_into().ok()?))
}

fn header_str<'z>(load: &'z fastfile_t6::ZoneLoad, header: &[u8], at: usize) -> Option<&'z str> {
    let raw = header_u32(header, at)?;
    if raw == 0 || raw >= 0xFFFF_FFFE {
        return None;
    }
    let e = raw - 1;
    let p = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    core::str::from_utf8(load.blocks.cstr(p).ok()?).ok()
}

/// The image packages beside a T6 zone.
fn open_ipaks(zone: &Path, report: &mut Vec<String>) -> Vec<asset_transport::IPak> {
    let Some(dir) = zone.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        report.push(format!("t6 content: cannot read {}", dir.display()));
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ipak"))
        })
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|path| match asset_transport::IPak::open(path) {
            Ok(ipak) => Some(ipak),
            Err(error) => {
                report.push(format!("t6 content: {error}"));
                None
            }
        })
        .collect()
}

/// T6 sampler bits are IW4's (`filter:3 mipMap:2 clampU clampV clampW`),
/// but its anisotropic filters (3, 4) sampled these textures black in play;
/// they are drawn with plain linear filtering and the rest kept.
fn t6_sampler_state(state: u8) -> u8 {
    const FILTER_LINEAR: u8 = 2;
    let filter = state & 0b111;
    (state & !0b111) | filter.min(FILTER_LINEAR)
}

/// A material's sampler as stored: the image name, its IWI bytes from the
/// image packages and the sampler state.
struct T6Texels {
    name: String,
    iwi: Vec<u8>,
    sampler_state: u8,
}

/// The first sampler of a material whose `nameHash` is one of `samplers`,
/// read from the image packages.
fn read_texture(
    load: &fastfile_t6::ZoneLoad,
    material: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
    samplers: &[u32],
) -> Result<Option<T6Texels>, String> {
    let header = &material.header;
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT).ok_or("short material")?);
    let raw = header_u32(header, MATERIAL_TEXTURE_TABLE).ok_or("short material")?;
    if raw == 0 || count == 0 {
        return Ok(None);
    }
    let e = raw - 1;
    let table = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    for index in 0..count {
        let def = table.at(index * MATERIAL_TEXTURE_DEF);
        let bytes = load
            .blocks
            .bytes(def, MATERIAL_TEXTURE_DEF as usize)
            .map_err(|e| format!("{e:?}"))?;
        if !header_u32(bytes, 0).is_some_and(|hash| samplers.contains(&hash)) {
            continue;
        }
        let sampler_state = bytes[6];
        let Some(image) = load.asset_in(material, def.at(12)) else {
            return Ok(None);
        };
        let (name, iwi) = read_streamed_image(load, image, ipaks)?;
        return Ok(Some(T6Texels {
            name,
            iwi,
            sampler_state: t6_sampler_state(sampler_state),
        }));
    }
    Ok(None)
}

/// Textures already decoded for a capture, by image name (a folded colour
/// map by both its images' names): a weapon's camo materials share one set.
type DecodedTextures = BTreeMap<String, Arc<Image>>;

/// A material's colour, normal and specular texels as IW4-shaded textures;
/// the colour map carries the specular map folded in (see
/// [`asset_material::decode_iwi_texture_t6_folded`]).
fn capture_material(
    load: &fastfile_t6::ZoneLoad,
    material: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
    decoded: &mut DecodedTextures,
    report: &mut Vec<String>,
) -> T6MaterialCapture {
    let mut read = |samplers: &[u32]| {
        read_texture(load, material, ipaks, samplers).unwrap_or_else(|error| {
            report.push(format!("t6 content: {error}"));
            None
        })
    };
    let (color, normal, specular) = (
        read(&COLOR_SAMPLERS),
        read(&NORMAL_SAMPLERS),
        read(&SPECULAR_SAMPLERS),
    );
    let mut decode =
        |texels: &T6Texels, key: String, decode: &dyn Fn() -> Result<Image, String>| {
            if let Some(image) = decoded.get(&key) {
                return Some((texels.name.clone(), image.clone()));
            }
            match decode() {
                Ok(image) => {
                    let image = Arc::new(image);
                    decoded.insert(key, image.clone());
                    Some((texels.name.clone(), image))
                }
                Err(error) => {
                    report.push(format!("t6 content: {}: {error}", texels.name));
                    None
                }
            }
        };
    let plain = |t: &T6Texels, is_normal: bool| {
        asset_material::decode_iwi_texture(&t.iwi, t.sampler_state, is_normal, !is_normal)
    };
    T6MaterialCapture {
        native: None,
        color: color.as_ref().and_then(|c| match &specular {
            Some(s) => decode(c, format!("{}+{}", c.name, s.name), &|| {
                asset_material::decode_iwi_texture_t6_folded(&c.iwi, &s.iwi, c.sampler_state)
            }),
            None => decode(c, c.name.clone(), &|| plain(c, false)),
        }),
        normal: normal
            .as_ref()
            .and_then(|n| decode(n, n.name.clone(), &|| plain(n, true))),
        specular: specular
            .as_ref()
            .and_then(|s| decode(s, s.name.clone(), &|| plain(s, false))),
    }
}

fn decode_ptr(raw: u32) -> Option<fastfile_t6::Ptr> {
    (raw != 0 && raw < 0xFFFF_FFFE).then(|| fastfile_t6::Ptr {
        block: ((raw - 1) >> 29) as u8,
        offset: (raw - 1) & 0x1FFF_FFFF,
    })
}

/// Offsets into T6 `Material`, `MaterialTechniqueSet`, `MaterialTechnique`,
/// `MaterialPass` and `MaterialShaderArgument`.
const MATERIAL_CONSTANT_COUNT: usize = 85;
const MATERIAL_TECHNIQUE_SET: u32 = 92;
const MATERIAL_CONSTANT_TABLE: usize = 100;
/// `stateBitsEntry[36]`: a state-table index per technique, 0xff for none.
const MATERIAL_STATE_BITS_ENTRY: usize = 48;
const MATERIAL_STATE_BITS_TABLE: usize = 104;
/// `GfxStateBits`: `loadBits[2]` and the three D3D11 state objects.
const MATERIAL_STATE_BITS: u32 = 20;
const MATERIAL_CONSTANT_DEF: u32 = 32;
const TECHNIQUE_SET_TECHNIQUES: usize = 8;
const TECHNIQUE_HEADER: u32 = 8;
const MATERIAL_PASS: u32 = 24;
const SHADER_ARGUMENT: u32 = 12;

/// A T6 technique set's techniques, passes, programs and arguments.
fn read_technique_set(
    load: &fastfile_t6::ZoneLoad,
    header: &[u8],
) -> Option<asset_material::t6_techset::T6TechniqueSet> {
    use asset_material::t6_techset::{
        T6Argument, T6Pass, T6Technique, T6TechniqueSet, argument_type,
    };
    let blocks = &load.blocks;
    let u16_le = |b: &[u8], at: usize| u16::from_le_bytes([b[at], b[at + 1]]);
    let shader = |pointer: Option<fastfile_t6::Ptr>| -> Option<(String, Vec<u8>)> {
        let header = blocks.bytes(pointer?, 16).ok()?;
        let program = decode_ptr(header_u32(header, 8)?)?;
        let size = header_u32(header, 12)? as usize;
        Some((
            header_str(load, header, 0).unwrap_or("").to_owned(),
            blocks.bytes(program, size).ok()?.to_vec(),
        ))
    };
    let techniques = (0..T6_TECHNIQUE_COUNT)
        .map(|index| {
            let technique = decode_ptr(header_u32(header, TECHNIQUE_SET_TECHNIQUES + 4 * index)?)?;
            let head = blocks.bytes(technique, TECHNIQUE_HEADER as usize).ok()?;
            let (flags, pass_count) = (u16_le(head, 4), u16_le(head, 6));
            let passes = (0..u32::from(pass_count))
                .map(|p| {
                    let pass = blocks
                        .bytes(
                            technique.at(TECHNIQUE_HEADER + MATERIAL_PASS * p),
                            MATERIAL_PASS as usize,
                        )
                        .ok()?;
                    let (vertex_name, vertex) = shader(decode_ptr(header_u32(pass, 4)?))?;
                    let (pixel_name, pixel) = shader(decode_ptr(header_u32(pass, 8)?))?;
                    let count = u32::from(pass[12]) + u32::from(pass[13]) + u32::from(pass[14]);
                    let table = decode_ptr(header_u32(pass, 20)?);
                    let arguments = (0..count)
                        .filter_map(|a| {
                            let raw = blocks.bytes(table?.at(SHADER_ARGUMENT * a), 12).ok()?;
                            let kind = u16_le(raw, 0);
                            let def = header_u32(raw, 8)?;
                            let literal = matches!(
                                kind,
                                argument_type::LITERAL_VERTEX_CONST
                                    | argument_type::LITERAL_PIXEL_CONST
                            )
                            .then(|| {
                                let words = blocks.bytes(decode_ptr(def)?, 16).ok()?;
                                Some(core::array::from_fn(|i| {
                                    header_u32(words, 4 * i).unwrap_or(0)
                                }))
                            })
                            .flatten();
                            Some(T6Argument {
                                kind,
                                offset: u16_le(raw, 2),
                                size: raw[4],
                                buffer: u16_le(raw, 6),
                                def,
                                literal,
                            })
                        })
                        .collect();
                    Some(T6Pass {
                        vertex_name,
                        vertex,
                        pixel_name,
                        pixel,
                        custom_sampler_flags: pass[15],
                        arguments,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(T6Technique { flags, passes })
        })
        .collect();
    Some(T6TechniqueSet {
        name: header_str(load, header, 0)?
            .trim_start_matches(',')
            .to_owned(),
        techniques,
    })
}

const T6_TECHNIQUE_COUNT: usize = 36;

/// The material as its own technique set draws it: the technique set
/// (read once per name into `techsets`), every texture decoded as T6
/// shaders sample it (`decoded` shares them across materials), and the
/// constants.
#[allow(clippy::too_many_arguments)]
fn capture_native(
    load: &fastfile_t6::ZoneLoad,
    zones: &[&fastfile_t6::ZoneLoad],
    address: fastfile_t6::Ptr,
    material: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
    decoded: &mut DecodedTextures,
    techsets: &mut BTreeMap<String, asset_material::t6_techset::T6TechniqueSet>,
    report: &mut Vec<String>,
) -> Option<T6NativeMaterial> {
    // Read through the material's own record: the slot's memory may since
    // hold another material's.
    let techset = material
        .field(MATERIAL_TECHNIQUE_SET)
        .map(|index| &load.assets[index])
        .or_else(|| load.asset_at(address.at(MATERIAL_TECHNIQUE_SET)))?;
    // A `,name` technique set is another zone's, referenced by name.
    let technique_set = header_str(load, &techset.header, 0)?
        .trim_start_matches(',')
        .to_owned();
    if !techsets.contains_key(&technique_set) {
        // A zone that only references the set (`,name`) carries no
        // techniques: the zone defining it does.
        let defined = |set: &asset_material::t6_techset::T6TechniqueSet| {
            set.techniques.iter().any(Option::is_some)
        };
        let read = read_technique_set(load, &techset.header)
            .filter(|set| defined(set))
            .or_else(|| {
                zones.iter().find_map(|zone| {
                    zone.assets
                        .iter()
                        .filter(|asset| asset.ty == fastfile_t6::AssetType::TechniqueSet)
                        .filter(|asset| {
                            header_str(zone, &asset.header, 0)
                                .is_some_and(|name| name.trim_start_matches(',') == technique_set)
                        })
                        .filter_map(|asset| read_technique_set(zone, &asset.header))
                        .find(|set| defined(set))
                })
            });
        match read {
            Some(set) => {
                techsets.insert(technique_set.clone(), set);
            }
            None => {
                report.push(format!(
                    "t6 native: {technique_set}: technique set unreadable"
                ));
                return None;
            }
        }
    }
    let header = &material.header;
    // The first load-bits word of the material's state in a technique;
    // `None` where the material does not draw with it.
    let state = |technique: usize| {
        header
            .get(MATERIAL_STATE_BITS_ENTRY + technique)
            .filter(|&&entry| entry != 0xff)
            .and_then(|&entry| {
                let table = decode_ptr(header_u32(header, MATERIAL_STATE_BITS_TABLE)?)?;
                let bytes = load
                    .blocks
                    .bytes(table.at(u32::from(entry) * MATERIAL_STATE_BITS), 4)
                    .ok()?;
                header_u32(bytes, 0)
            })
    };
    let lit_state = state(asset_material::t6_techset::T6_TECHNIQUE_LIT);
    let emissive_state = state(asset_material::t6_techset::T6_TECHNIQUE_EMISSIVE);
    // T6 colour maps keep the weapon's camo mask in alpha, and T6 lit
    // shaders scale their output by that alpha; an opaque surface reads
    // it as one.
    let opaque = lit_state.is_some_and(|bits| {
        asset_material::MaterialDrawMode::from_state_bits([bits, 0])
            == asset_material::MaterialDrawMode::Opaque
    });
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT)?);
    let table = decode_ptr(header_u32(header, MATERIAL_TEXTURE_TABLE)?);
    let mut textures = Vec::new();
    for index in 0..count {
        let def = table?.at(index * MATERIAL_TEXTURE_DEF);
        let Ok(bytes) = load.blocks.bytes(def, MATERIAL_TEXTURE_DEF as usize) else {
            continue;
        };
        let (name_hash, sampler_state, semantic) = (header_u32(bytes, 0)?, bytes[6], bytes[7]);
        let Some(image) = load.asset_in(material, def.at(12)) else {
            continue;
        };
        let image_name = header_str(load, &image.header, IMAGE_NAME)
            .unwrap_or("")
            .to_owned();
        let texels = match decoded.get(&image_name) {
            Some(texels) => texels.clone(),
            None => {
                let texels = read_streamed_image(load, image, ipaks).and_then(|(_, iwi)| {
                    asset_material::decode_iwi_texture_native(
                        &iwi,
                        t6_sampler_state(sampler_state),
                        false,
                    )
                });
                match texels {
                    Ok(texels) => {
                        let texels = Arc::new(texels);
                        decoded.insert(image_name.clone(), texels.clone());
                        texels
                    }
                    // Images another zone or the renderer supplies (camo
                    // patterns, the emblem) read as white.
                    Err(_) => decoded
                        .entry("$t6_white".to_owned())
                        .or_insert_with(|| Arc::new(asset_material::solid_texture([255; 4], false)))
                        .clone(),
                }
            }
        };
        let (image_name, texels) = if opaque && semantic == asset_material::TS_COLOR_MAP {
            let name = format!("{image_name}$opaque");
            let texels = decoded
                .entry(name.clone())
                .or_insert_with(|| Arc::new(asset_material::with_opaque_alpha(&texels)))
                .clone();
            (name, texels)
        } else {
            (image_name, texels)
        };
        textures.push(asset_material::t6_techset::T6Texture {
            name_hash,
            sampler_state: t6_sampler_state(sampler_state),
            semantic,
            image: image_name,
            texels,
        });
    }
    let constant_count = u32::from(*header.get(MATERIAL_CONSTANT_COUNT)?);
    let constant_table = decode_ptr(header_u32(header, MATERIAL_CONSTANT_TABLE)?);
    let constants = (0..constant_count)
        .filter_map(|index| {
            let bytes = load
                .blocks
                .bytes(
                    constant_table?.at(index * MATERIAL_CONSTANT_DEF),
                    MATERIAL_CONSTANT_DEF as usize,
                )
                .ok()?;
            Some(asset_material::MaterialConstant {
                name_hash: header_u32(bytes, 0)?,
                name: bytes[4..16].try_into().ok()?,
                literal: core::array::from_fn(|i| {
                    f32::from_bits(header_u32(bytes, 16 + 4 * i).unwrap_or(0))
                }),
            })
        })
        .collect();
    Some(T6NativeMaterial {
        technique_set,
        textures,
        constants,
        lit_state,
        emissive_state,
    })
}

/// The name and IWI bytes of a streamed image, read from the image packages.
fn read_streamed_image(
    load: &fastfile_t6::ZoneLoad,
    image: &fastfile_t6::LoadedAsset,
    ipaks: &[asset_transport::IPak],
) -> Result<(String, Vec<u8>), String> {
    let name = header_str(load, &image.header, IMAGE_NAME)
        .ok_or("image without a name")?
        .to_owned();
    if image
        .header
        .get(IMAGE_STREAMED_PART_COUNT)
        .copied()
        .unwrap_or(0)
        == 0
    {
        return Err(format!("{name}: not streamed"));
    }
    let hash = header_u32(&image.header, IMAGE_HASH).ok_or("short image")?;
    let part = header_u32(&image.header, IMAGE_STREAMED_PART_HASH).ok_or("short image")?;
    let bytes = ipaks
        .iter()
        .find_map(|ipak| ipak.read(hash, part))
        .ok_or_else(|| format!("{name}: in no image package"))??;
    Ok((name, bytes))
}

/// The zones beside `common_mp`, besides `common_mp` and `patch_mp`, whose
/// materials the class menu's weapon icons are (`menu_mp_weapons_*`,
/// `hud_*`), the scope overlays and the equipment's HUD icons.
const ICON_ZONES: [&str; 3] = ["code_post_gfx_mp.ff", "patch_ui_mp.ff", "ui_mp.ff"];

/// The colour map of every material the stats table's weapon rows and the
/// attachment table show in the class menu, decoded to RGBA and keyed by material name. The first of
/// `loads` defining a material wins.
fn capture_weapon_icons(
    common: &Path,
    loads: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    report: &mut Vec<String>,
) -> Vec<(String, asset_material::ZoneUiRgba)> {
    let mut wanted = std::collections::BTreeSet::new();
    for load in loads {
        for asset in &load.assets {
            // The scope overlays of the weapons and of their attachments.
            if let Some(weapon) = fastfile_t6::weapon::WeaponView::new(load, asset) {
                let overlays = weapon
                    .attachment_uniques()
                    .filter_map(|unique| {
                        unique.asset_field_name(fastfile_t6::weapon::unique::OVERLAY_MATERIAL)
                    })
                    .chain(
                        weapon.variant_asset_name(fastfile_t6::weapon::variant::OVERLAY_MATERIAL),
                    );
                wanted.extend(overlays.map(str::to_ascii_lowercase));
                // An equipment's HUD icon.
                if let Some(icon) = weapon.def_loaded_asset_name(fastfile_t6::weapon::def::HUD_ICON)
                {
                    wanted.insert(asset_game::t6_model_name(icon).to_ascii_lowercase());
                }
            }
            let Some(table) = asset_game::capture_t6_string_table(load, asset) else {
                continue;
            };
            let attachments = table.name.eq_ignore_ascii_case("mp/attachmentTable.csv");
            if !attachments && !asset_game::is_stats_table_name(&table.name) {
                continue;
            }
            for row in 0..table.rows {
                let cell = |column: usize| {
                    table
                        .cells
                        .get(row * table.columns + column)
                        .map_or("", String::as_str)
                };
                // The attachment table's icons (column 6), every row an
                // attachment's; the stats table's, its weapon rows.
                if (attachments || cell(2).starts_with("weapon_")) && !cell(6).is_empty() {
                    wanted.insert(cell(6).to_ascii_lowercase());
                }
            }
        }
    }
    let extra: Vec<_> = ICON_ZONES
        .iter()
        .filter_map(|name| {
            let path = common.with_file_name(name);
            let image = match asset_transport::open_t6_zone(&path) {
                Ok(image) => image,
                Err(error) => {
                    report.push(format!("t6 icons: {}: {error:?}", path.display()));
                    return None;
                }
            };
            let (load, walked) = fastfile_t6::load_zone(schema().ok()?, &image.bytes, |_, _| true);
            if let Err(error) = walked {
                report.push(format!("t6 icons: {} walk: {error:?}", path.display()));
            }
            Some(load)
        })
        .collect();
    let mut icons = Vec::new();
    let mut failed = Vec::new();
    let all: Vec<&fastfile_t6::ZoneLoad> = loads.iter().copied().chain(&extra).collect();
    for load in loads.iter().copied().chain(&extra) {
        for asset in &load.assets {
            if asset.ty != fastfile_t6::AssetType::Material {
                continue;
            }
            let Some(name) = header_str(load, &asset.header, 0).map(str::to_ascii_lowercase) else {
                continue;
            };
            if !wanted.remove(&name) {
                continue;
            }
            match capture_icon(load, asset, &all, ipaks) {
                Ok((width, height, rgba)) => icons.push((name, (width, height, Arc::new(rgba)))),
                Err(error) => failed.push(format!("{name}: {error}")),
            }
        }
    }
    report.push(format!(
        "t6 icons: {} weapon icons decoded; {} failed {failed:?}; {} in no zone read {wanted:?}",
        icons.len(),
        failed.len(),
        wanted.len()
    ));
    icons
}

/// A UI material's colour map as RGBA8.
fn capture_icon(
    load: &fastfile_t6::ZoneLoad,
    material: &fastfile_t6::LoadedAsset,
    zones: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> Result<(u32, u32, Vec<u8>), String> {
    let header = &material.header;
    let count = u32::from(*header.get(MATERIAL_TEXTURE_COUNT).ok_or("short material")?);
    let raw = header_u32(header, MATERIAL_TEXTURE_TABLE).ok_or("short material")?;
    if raw == 0 {
        return Err("no textures".to_owned());
    }
    let e = raw - 1;
    let table = fastfile_t6::Ptr {
        block: (e >> 29) as u8,
        offset: e & 0x1FFF_FFFF,
    };
    for index in 0..count {
        let def = table.at(index * MATERIAL_TEXTURE_DEF);
        if load.blocks.u32_at(def).ok() != Some(COLOR_MAP_HASH) {
            continue;
        }
        let image = load
            .asset_in(material, def.at(12))
            .ok_or("colour map without an image")?;
        let (name, bytes) = match read_streamed_image(load, image, ipaks) {
            Ok(read) => read,
            // A `,name` image is another zone's: read that zone's.
            Err(error) => header_str(load, &image.header, IMAGE_NAME)
                .and_then(|name| name.strip_prefix(','))
                .and_then(|name| {
                    zones.iter().find_map(|zone| {
                        zone.assets
                            .iter()
                            .filter(|asset| asset.ty == fastfile_t6::AssetType::Image)
                            .filter(|asset| {
                                header_str(zone, &asset.header, IMAGE_NAME) == Some(name)
                            })
                            .find_map(|asset| read_streamed_image(zone, asset, ipaks).ok())
                    })
                })
                .ok_or(error)?,
        };
        return asset_material::decode_iwi_rgba(&bytes).map_err(|e| format!("{name}: {e}"));
    }
    Err("no colour map".to_owned())
}

/// The view, world and projectile models of every weapon that has an IW4
/// stand-in, and the colour maps of their materials. A model `load` only
/// references (`,name`) is taken from the first of `others` defining it.
/// An attached model's rest on its gun's root bone, from an offset and
/// `(pitch, yaw, roll)` degrees (yaw about z, then pitch about y, then roll
/// about x).
fn attachment_rest(offset: [f32; 3], angles: [f32; 3]) -> (bevy::math::Quat, bevy::math::Vec3) {
    let [pitch, yaw, roll] = angles.map(f32::to_radians);
    (
        bevy::math::Quat::from_euler(bevy::math::EulerRot::ZYX, yaw, pitch, roll),
        bevy::math::Vec3::from_array(offset),
    )
}

fn capture_content(
    load: &fastfile_t6::ZoneLoad,
    others: &[fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
) -> T6Content {
    use fastfile_t6::weapon::{WeaponView, def};
    let mut content = T6Content::default();
    let zones: Vec<&fastfile_t6::ZoneLoad> = std::iter::once(load).chain(others).collect();
    let mut models: BTreeMap<&str, (&fastfile_t6::ZoneLoad, &fastfile_t6::LoadedAsset)> =
        BTreeMap::new();
    for load in std::iter::once(load).chain(others) {
        for asset in &load.assets {
            if let Some(name) = asset_model::T6Model::new(load, asset).and_then(|m| m.name())
                && !name.starts_with(',')
            {
                models.entry(name).or_insert((load, asset));
            }
        }
    }
    let mut wanted: BTreeMap<String, (bool, bool, &'static str)> = BTreeMap::new();
    // Where each attached model (a magazine, a sniper's scope) rests on its
    // gun's root bone.
    let mut placements: BTreeMap<String, ([f32; 3], [f32; 3])> = BTreeMap::new();
    // An attachment model's copy placed for one weapon: the model it
    // copies and the gun bone it hangs from.
    let mut copies: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    for asset in &load.assets {
        let Some(weapon) = WeaponView::new(load, asset) else {
            continue;
        };
        let Some(stand_in) = weapon.name().and_then(asset_game::t6_stand_in_for) else {
            continue;
        };

        for (name, view) in [
            (weapon.def_asset_array_name(def::GUN_XMODEL, 0), true),
            (weapon.def_asset_array_name(def::WORLD_MODEL, 0), false),
            (weapon.def_asset_name(def::PROJECTILE_MODEL), false),
            (weapon.name().and_then(asset_game::t6_planted_model), false),
        ] {
            if let Some(name) = name {
                wanted
                    .entry(asset_game::t6_model_name(name))
                    .or_insert((view, false, stand_in));
            }
        }
        for unique in weapon.attachment_uniques() {
            let placed = [true, false]
                .into_iter()
                .flat_map(|view| {
                    asset_game::t6_attachment_models(unique, view)
                        .into_iter()
                        .map(move |placed| (placed, view))
                })
                .chain(asset_game::t6_attachment_ads_model(unique).map(|placed| (placed, true)));
            for (placed, view) in placed {
                placements.insert(placed.copy.clone(), (placed.offset, placed.angles));
                wanted
                    .entry(placed.copy.clone())
                    .or_insert((view, false, stand_in));
                copies.insert(placed.copy, (placed.model, placed.tag));
            }
        }
        for view in [true, false] {
            for slot in 0..fastfile_t6::weapon::variant::ATTACH_MODEL_COUNT {
                if let Some((name, offset, angles)) = weapon.attached_model(slot, view) {
                    let name = asset_game::t6_model_name(name);
                    placements.entry(name.clone()).or_insert((offset, angles));
                    wanted.entry(name).or_insert((view, false, stand_in));
                }
            }
        }
    }
    // Any stand-in lends the arms their materials: IW4 weapons share arms.
    if let Some(&(_, _, stand_in)) = wanted.values().next() {
        wanted.insert(HANDS_MODEL.to_owned(), (true, true, stand_in));
        // So it does the melee knife, a weapon model of no stand-in's.
        if let Some(knife) = melee_weapon(load).map(|melee| melee.knife) {
            wanted.entry(knife).or_insert((true, false, stand_in));
        }
    }
    content.melee = melee_weapon(load);
    let (mut failed, mut decoded, mut missing) = (0usize, 0usize, 0usize);
    let mut textures = DecodedTextures::new();
    let mut native_textures = DecodedTextures::new();
    for (name, (view, hands, stand_in)) in wanted {
        let copy = copies.get(&name);
        let source = copy.map_or(name.as_str(), |(model, _)| model.as_str());
        let Some(&(load, asset)) = models.get(source) else {
            content
                .report
                .push(format!("t6 content: {name}: in no zone read"));
            continue;
        };
        let Some(model) = asset_model::T6Model::new(load, asset) else {
            continue;
        };
        let Some(mut skel) = asset_model::capture_model_skel_t6(model, |_| None) else {
            failed += 1;
            content
                .report
                .push(format!("t6 content: {name}: model capture failed"));
            continue;
        };
        if let Some(&(offset, angles)) = placements.get(&name)
            && let Some(pose) = skel.pose.as_mut()
        {
            pose.root_rest = Some(attachment_rest(offset, angles));
        }
        if let Some((_, tag)) = copy {
            skel.name = name.clone();
            skel.mount_tag = tag.clone();
        }
        let mut surface_materials = Vec::with_capacity(model.surface_count());
        for surface in 0..model.surface_count() {
            let material = model
                .material_slot(surface)
                .and_then(|slot| load.asset_in(asset, slot));
            let material_name = material
                .and_then(|m| header_str(load, &m.header, 0))
                .map(str::to_owned);
            if let (Some(material), Some(material_name)) = (material, &material_name)
                && !content.materials.contains_key(material_name)
            {
                let reported = content.report.len();
                let mut capture =
                    capture_material(load, material, ipaks, &mut textures, &mut content.report);
                capture.native = model
                    .material_slot(surface)
                    .and_then(|slot| load.blocks.ptr_at(slot).ok().flatten())
                    .and_then(|address| {
                        capture_native(
                            load,
                            &zones,
                            address,
                            material,
                            ipaks,
                            &mut native_textures,
                            &mut content.techsets,
                            &mut content.report,
                        )
                    });
                missing += content.report.len() - reported;
                decoded += [&capture.color, &capture.normal, &capture.specular]
                    .iter()
                    .filter(|texture| texture.is_some())
                    .count();
                content.materials.insert(material_name.clone(), capture);
            }
            surface_materials.push(material_name);
        }
        if hands {
            content.hands = Some(name.clone());
        }
        content.models.push(T6ModelCapture {
            skel,
            surface_materials,
            view,
            hands,
            stand_in,
        });
    }
    content.report.push(format!(
        "t6 content: {} models ({failed} failed), {} materials, {decoded} colour, normal and specular maps decoded, {missing} missing; {} image packages",
        content.models.len(),
        content.materials.len(),
        ipaks.len()
    ));
    capture_effects(&zones, ipaks, &mut content);
    content
}

/// T6 `FxElemDef`: element type, sample counts and pointers, visuals and
/// the child effects it names.
const FX_ELEM_DEF: u32 = 292;
const FX_ELEM_TYPE: usize = 184;
const FX_ELEM_VEL_SAMPLES: usize = 188;
const FX_ELEM_VIS_SAMPLES: usize = 192;
const FX_ELEM_VISUALS: u32 = 196;
const FX_ELEM_CHILDREN: [usize; 3] = [224, 228, 232];
const FX_ELEM_VEL_SAMPLE: usize = 96;
const FX_ELEM_VIS_SAMPLE: usize = 48;
const FX_EFFECT_DEF_ELEMS: usize = 28;
/// T6 element types up to `cloud` draw a material; 7 a model, 10 a sound,
/// 12 a runner (an effect by name).
const FX_ELEM_LAST_SPRITE: u8 = 6;
const FX_ELEM_MODEL: u8 = 7;
const FX_ELEM_SOUND: u8 = 10;
const FX_ELEM_RUNNER: u8 = 12;

/// The effects T6 content plays in IW4 matches, with the colour maps of
/// the materials their sprites draw.
fn capture_effects(
    zones: &[&fastfile_t6::ZoneLoad],
    ipaks: &[asset_transport::IPak],
    content: &mut T6Content,
) {
    let mut textures = DecodedTextures::new();
    // The listed effects and every effect they play in turn (runners, and
    // the effects their elements spawn on impact, on death or as they go).
    let mut wanted: std::collections::BTreeSet<String> = asset_game::T6_EFFECTS
        .iter()
        .map(|&name| name.to_owned())
        .collect();
    loop {
        let before = content.fx.len();
        for &load in zones {
            for asset in &load.assets {
                if asset.ty != fastfile_t6::AssetType::Fx {
                    continue;
                }
                let Some(name) = header_str(load, &asset.header, 0) else {
                    continue;
                };
                if !wanted.contains(name) || content.fx.iter().any(|fx| fx.name == name) {
                    continue;
                }
                match capture_effect(load, asset, name, ipaks, &mut textures, content) {
                    Some(fx) => content.fx.push(fx),
                    None => content
                        .report
                        .push(format!("t6 effects: {name}: unreadable")),
                }
            }
        }
        for fx in &content.fx[before..] {
            for elem in &fx.elems {
                if elem.raw[FX_ELEM_TYPE] == FX_ELEM_RUNNER {
                    wanted.extend(elem.visuals.iter().filter(|v| !v.is_empty()).cloned());
                }
                for child in [
                    &elem.effect_on_impact,
                    &elem.effect_on_death,
                    &elem.effect_emitted,
                ] {
                    if !child.is_empty() {
                        wanted.insert(child.clone());
                    }
                }
            }
        }
        if content.fx.len() == before {
            break;
        }
    }
    let missing: Vec<_> = wanted
        .iter()
        .filter(|name| !content.fx.iter().any(|fx| fx.name == **name))
        .collect();
    content.report.push(format!(
        "t6 effects: {} captured, {} materials; in no zone read: {missing:?}",
        content.fx.len(),
        content.fx_materials.len()
    ));
}

fn capture_effect(
    load: &fastfile_t6::ZoneLoad,
    asset: &fastfile_t6::LoadedAsset,
    name: &str,
    ipaks: &[asset_transport::IPak],
    textures: &mut DecodedTextures,
    content: &mut T6Content,
) -> Option<asset_game::T6FxCapture> {
    let header = &asset.header;
    let count = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]) as u32;
    let elem_count = count(8) + count(10) + count(12);
    let mut fx = asset_game::T6FxCapture {
        name: name.to_owned(),
        header: header.clone(),
        elems: Vec::new(),
    };
    if elem_count == 0 {
        return Some(fx);
    }
    let elems = decode_ptr(header_u32(header, FX_EFFECT_DEF_ELEMS)?)?;
    let blocks = &load.blocks;
    for i in 0..elem_count {
        let at = elems.at(i * FX_ELEM_DEF);
        let raw = blocks.bytes(at, FX_ELEM_DEF as usize).ok()?.to_vec();
        let elem_type = raw[FX_ELEM_TYPE];
        let visual_count = usize::from(raw[FX_ELEM_TYPE + 1]);
        let samples = |field: usize, n: usize, stride: usize| -> Vec<u8> {
            decode_ptr(header_u32(&raw, field).unwrap_or(0))
                .and_then(|p| blocks.bytes(p, n * stride).ok())
                .map_or_else(Vec::new, <[u8]>::to_vec)
        };
        let vel_samples = samples(
            FX_ELEM_VEL_SAMPLES,
            usize::from(raw[FX_ELEM_TYPE + 2]) + 1,
            FX_ELEM_VEL_SAMPLE,
        );
        let vis_samples = samples(
            FX_ELEM_VIS_SAMPLES,
            usize::from(raw[FX_ELEM_TYPE + 3]) + 1,
            FX_ELEM_VIS_SAMPLE,
        );
        // One visual sits in the element; more are an array it points at.
        let slots: Vec<fastfile_t6::Ptr> = if visual_count > 1 {
            decode_ptr(header_u32(&raw, FX_ELEM_VISUALS as usize).unwrap_or(0))
                .map_or_else(Vec::new, |arr| {
                    (0..visual_count as u32).map(|v| arr.at(v * 4)).collect()
                })
        } else if visual_count == 1 {
            vec![at.at(FX_ELEM_VISUALS)]
        } else {
            Vec::new()
        };
        let mut visuals = Vec::new();
        for slot in slots {
            let visual = match elem_type {
                0..=FX_ELEM_LAST_SPRITE => {
                    let material = load.asset_in(asset, slot);
                    let name = material.and_then(|m| header_str(load, &m.header, 0));
                    if let (Some(material), Some(name)) = (material, name)
                        && !content.fx_materials.contains_key(name)
                    {
                        let capture =
                            capture_material(load, material, ipaks, textures, &mut content.report);
                        content.fx_materials.insert(name.to_owned(), capture);
                    }
                    name.map(str::to_owned)
                }
                FX_ELEM_MODEL => load
                    .asset_in(asset, slot)
                    .and_then(|m| header_str(load, &m.header, 0))
                    .map(asset_game::t6_model_name),
                FX_ELEM_SOUND | FX_ELEM_RUNNER => blocks
                    .bytes(slot, 4)
                    .ok()
                    .and_then(|b| header_str(load, b, 0))
                    .map(str::to_owned),
                _ => None,
            };
            visuals.push(visual.unwrap_or_else(String::new));
        }
        let child =
            |field: usize| header_str(load, &raw, field).map_or_else(String::new, str::to_owned);
        fx.elems.push(asset_game::T6FxElemCapture {
            vel_samples,
            vis_samples,
            visuals,
            effect_on_impact: child(FX_ELEM_CHILDREN[0]),
            effect_on_death: child(FX_ELEM_CHILDREN[1]),
            effect_emitted: child(FX_ELEM_CHILDREN[2]),
            raw,
        });
    }
    Some(fx)
}

impl ZoneLane for T6Lane {
    fn game(&self) -> ZoneGame {
        Self::GAME
    }

    fn capabilities(&self) -> &'static [(PreparedCapability, LaneStatus)] {
        Self::CAPABILITIES
    }

    fn load_world(
        &self,
        path: &Path,
        _image: &ZoneImage,
        _progress: &LoadProgress,
        _shared_surfaces: asset_model::SharedXModelSurfaces,
        _material_seed: asset_material::MaterialCatalog,
        _common_film_visions: &mut std::collections::BTreeMap<
            String,
            Result<asset_world::FilmVision, asset_world::FilmVisionParseError>,
        >,
    ) -> LoadedWorld {
        LoadedWorld::with_gap(
            WorldDrawPolicy::iw4(),
            PreparedCapability::PreparedWorld,
            format!("T6 maps do not load: {}", path.display()),
            Some("assets::lane::t6::load_world/no_decoder"),
        )
    }

    fn load_common_mp(
        &self,
        path: &Path,
        image: &ZoneImage,
        progress: &LoadProgress,
        _decode_color_maps: bool,
        material_seed: asset_material::MaterialCatalog,
    ) -> CommonCensus {
        let mut report = vec![format!("common_mp (T6): {}", path.display())];
        // Claimed before the walk: the sound bank may ask for this zone while
        // the content is still decoding, and must wait for these aliases
        // rather than read the zone itself.
        let sound_claim = asset_audio::ZoneSoundCapture::claim_common(
            path,
            asset_audio::ZoneGame::T6,
            "t6 common",
        );
        let schema = match schema() {
            Ok(schema) => schema,
            Err(error) => {
                report.push(error);
                return CommonCensus {
                    material_population: material_seed,
                    report,
                    ..Default::default()
                };
            }
        };
        let stage = progress.begin_scoped(StageId::CommonAssets, "t6_walk", None);
        let (load, walked) = fastfile_t6::load_zone(schema, &image.bytes, |_, _| true);
        stage.done();
        if let Err(error) = walked {
            report.push(format!(
                "common_mp T6 walk: stopped after {} assets — {error:?}",
                load.assets.len()
            ));
        }
        let mut catalog = asset_game::WeaponCatalog::default();
        catalog.set_capture_ns(asset_core::AssetNamespace::T6);
        let mut seen = 0;
        for asset in &load.assets {
            if asset.ty != fastfile_t6::AssetType::Weapon {
                continue;
            }
            seen += 1;
            if let Some(view) = fastfile_t6::weapon::WeaponView::new(&load, asset) {
                catalog.capture_t6(view);
            }
        }
        let captured = catalog.len();
        let content_stage = progress.begin_scoped(StageId::CommonAssets, "t6_content", None);
        let patch = patch_zone(path, &mut report);
        let ipaks = open_ipaks(path, &mut report);
        let others: Vec<_> = patch
            .into_iter()
            .chain(hands_zone(path, &mut report))
            .collect();
        let mut content = capture_content(&load, &others, &ipaks);
        capture_xanims(&load, &others, &mut content);
        let icon_loads: Vec<_> = others.iter().chain(std::iter::once(&load)).collect();
        let icons = capture_weapon_icons(path, &icon_loads, &ipaks, &mut report);
        asset_material::store_zone_ui_images(asset_core::AssetNamespace::T6, icons);
        capture_sounds(path, &load, sound_claim, &mut content);
        content_stage.done();
        report.extend(content.report.iter().cloned());
        let mut weapons = catalog.into_build();
        weapons.stamp_namespace(asset_core::AssetNamespace::T6);
        report.push(format!(
            "common_mp T6: walked {} assets; weapons {seen} seen, {captured} with an IW4 stand-in",
            load.assets.len()
        ));
        CommonCensus {
            weapons,
            material_population: material_seed,
            report,
            t6_content: Some(content),
            ..Default::default()
        }
    }

    fn load_material_population(
        &self,
        path: &Path,
        _image: &ZoneImage,
        _progress: &LoadProgress,
        material_seed: asset_material::MaterialCatalog,
    ) -> MaterialPopulation {
        MaterialPopulation {
            materials: material_seed,
            report: vec![format!(
                "startup materials: T6 materials are not read ({})",
                path.display()
            )],
            ..Default::default()
        }
    }
}
