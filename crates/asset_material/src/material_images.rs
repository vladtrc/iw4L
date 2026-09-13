use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use asset_iw4::{
    IWI_V8_HEADER_LEN, ImgFormatKind, IwiHeader, WaveletBits, WaveletError, img_format_info,
    wavelet_check_header, wavelet_decompress_level, wavelet_level_size, wavelet_pixel_stride,
    wavelet_top_level,
};
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use bevy::tasks::{ComputeTaskPool, TaskPool};

use crate::material_catalog::TS_2D;
use crate::progress::LoadStage;
use crate::{
    AuthoredImage, MaterialCatalog, TS_COLOR_MAP, TS_FUNCTION, TS_NORMAL_MAP, TS_WATER_MAP,
};

struct DecodedMips {
    width: u32,
    height: u32,

    levels: Vec<Vec<u8>>,
    storage: MipStorage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MipStorage {
    Rgba8,
    Bc1,
    Bc2,
    Bc3,
}

impl DecodedMips {
    fn single(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            levels: vec![pixels],
            storage: MipStorage::Rgba8,
        }
    }

    fn single_compressed(width: u32, height: u32, storage: MipStorage, bytes: Vec<u8>) -> Self {
        Self {
            width,
            height,
            levels: vec![bytes],
            storage,
        }
    }

    fn level_count(&self) -> u32 {
        self.levels.len() as u32
    }

    fn packed(&self) -> Vec<u8> {
        self.levels.concat()
    }

    fn cache_encode(&self) -> Vec<u8> {
        let payload = self.levels.iter().map(Vec::len).sum::<usize>();
        let mut out = Vec::with_capacity(28 + 4 * self.levels.len() + payload);
        out.extend_from_slice(MIP_MAGIC);
        out.extend_from_slice(&MIP_CACHE_FORMAT.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.push(match self.storage {
            MipStorage::Rgba8 => 0,
            MipStorage::Bc1 => 1,
            MipStorage::Bc2 => 2,
            MipStorage::Bc3 => 3,
        });
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&(self.levels.len() as u32).to_le_bytes());
        for level in &self.levels {
            out.extend_from_slice(&(level.len() as u32).to_le_bytes());
            out.extend_from_slice(level);
        }
        out
    }

    fn cache_decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 28 || &bytes[..8] != MIP_MAGIC {
            return None;
        }
        let format = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
        if format != MIP_CACHE_FORMAT {
            return None;
        }
        let width = u32::from_le_bytes(bytes[12..16].try_into().ok()?);
        let height = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
        let storage = match bytes[20] {
            0 => MipStorage::Rgba8,
            1 => MipStorage::Bc1,
            2 => MipStorage::Bc2,
            3 => MipStorage::Bc3,
            _ => return None,
        };
        let level_count = u32::from_le_bytes(bytes[24..28].try_into().ok()?) as usize;
        let mut at = 28;
        let mut levels = Vec::with_capacity(level_count);
        for _ in 0..level_count {
            let len = u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize;
            at += 4;
            let level = bytes.get(at..at + len)?.to_vec();
            at += len;
            levels.push(level);
        }
        Some(Self {
            width,
            height,
            levels,
            storage,
        })
    }

    fn into_top_level_rgba8(self) -> Result<(u32, u32, Vec<u8>), String> {
        let level0 = self
            .levels
            .into_iter()
            .next()
            .ok_or_else(|| "decoded image carries no mip level".to_owned())?;
        let pixels = match self.storage {
            MipStorage::Rgba8 => level0,
            MipStorage::Bc1 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc1)?,
            MipStorage::Bc2 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc2)?,
            MipStorage::Bc3 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc3)?,
        };
        Ok((self.width, self.height, pixels))
    }

    fn texture_format(self_storage: MipStorage, linear: bool) -> TextureFormat {
        match (self_storage, linear) {
            (MipStorage::Rgba8, true) => TextureFormat::Rgba8Unorm,
            (MipStorage::Rgba8, false) => TextureFormat::Rgba8UnormSrgb,
            (MipStorage::Bc1, true) => TextureFormat::Bc1RgbaUnorm,
            (MipStorage::Bc1, false) => TextureFormat::Bc1RgbaUnormSrgb,
            (MipStorage::Bc2, true) => TextureFormat::Bc2RgbaUnorm,
            (MipStorage::Bc2, false) => TextureFormat::Bc2RgbaUnormSrgb,
            (MipStorage::Bc3, true) => TextureFormat::Bc3RgbaUnorm,
            (MipStorage::Bc3, false) => TextureFormat::Bc3RgbaUnormSrgb,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MaterialImageStats {
    pub requested: usize,
    pub decoded: usize,
    pub missing: usize,
    pub unsupported: usize,
    pub archives: usize,
    pub first_gap: Option<String>,
    pub first_unsupported: Option<String>,
}

#[derive(Debug)]
struct IwdIndex(Arc<asset_transport::IwdIndex>);

type CubemapFaces = [Vec<u8>; 6];

pub fn cached_iwd_main_dirs() -> Vec<PathBuf> {
    asset_transport::cached_iwd_dirs()
}

impl IwdIndex {
    fn open(directory: &Path) -> Result<Arc<Self>, String> {
        asset_transport::IwdIndex::open(directory).map(|index| Arc::new(Self(index)))
    }

    fn is_cached(directory: &Path) -> bool {
        asset_transport::IwdIndex::is_cached(directory)
    }

    fn decode(&self, name: &str) -> Option<Result<DecodedMips, String>> {
        let candidates = self.0.image_candidates(name)?;
        let mut first_error = None;
        for candidate in candidates {
            match load_or_decode_mips(candidate) {
                Ok(image) => return Some(Ok(image)),
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        Some(Err(
            first_error.unwrap_or_else(|| "empty IWD candidate list".into())
        ))
    }

    fn decode_cubemap_if_skybox(
        &self,
        name: &str,
        map_type: u8,
    ) -> Option<Result<(u32, CubemapFaces), String>> {
        let candidates = self.0.image_candidates(name)?;
        let mut saw_skybox = false;
        let mut first_error = None;
        for candidate in candidates {
            match candidate.read() {
                Ok(bytes) => {
                    if map_type != 5 && !IwiHeader::parse(&bytes).is_ok_and(|h| h.is_skybox()) {
                        continue;
                    }
                    saw_skybox = true;
                    match decode_iwi_cubemap(&bytes) {
                        Ok(image) => return Some(Ok(image)),
                        Err(error) if first_error.is_none() => first_error = Some(error),
                        Err(_) => {}
                    }
                }
                Err(error) if first_error.is_none() => first_error = Some(error),
                Err(_) => {}
            }
        }
        saw_skybox.then(|| Err(first_error.unwrap_or_else(|| "empty IWD candidate list".into())))
    }
}

fn color_map_force_linear(
    _is_unlit: Option<bool>,
    transform: crate::ColorMapTransform,
    takes_model_lighting: Option<bool>,
) -> bool {
    transform == crate::ColorMapTransform::Square || takes_model_lighting == Some(true)
}

fn texture_semantic_decodes_as_normal(semantic: u8) -> Option<bool> {
    match semantic {
        TS_FUNCTION | TS_WATER_MAP => None,
        TS_NORMAL_MAP => Some(true),
        _ => Some(false),
    }
}

pub fn decode_material_color_maps(
    zone_ff: &Path,
    catalog: &mut MaterialCatalog,
    stage: &LoadStage,
) -> Result<MaterialImageStats, String> {
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    let mut stats = MaterialImageStats {
        archives: index.0.archive_count(),
        ..Default::default()
    };

    let work = requested_color_map_slots(catalog);
    stats.requested = work.len();
    stage.total(work.len() as u64);

    let images = &catalog.images;
    let decoded = decode_requests_in_parallel(images, index.as_ref(), &work, stage);

    for outcome in decoded {
        match outcome {
            ImageOutcome::Decoded { image_index, image } => {
                catalog.images[image_index].decoded = Some(image);
                stats.decoded += 1;
            }
            ImageOutcome::Missing { gap } => {
                stats.missing += 1;
                stats.first_gap.get_or_insert(gap);
            }
            ImageOutcome::Unsupported { gap } => {
                stats.unsupported += 1;
                stats.first_unsupported.get_or_insert(gap);
            }
        }
    }
    Ok(stats)
}

fn requested_color_map_slots(catalog: &MaterialCatalog) -> Vec<ImageRequest> {
    let mut requested = vec![None; catalog.images.len()];
    for material in &catalog.materials {
        let alpha_test = catalog
            .agreed_alpha_test_cutoff(material)
            .is_some_and(|cutoff| cutoff.is_some());
        let force_linear = color_map_force_linear(
            catalog.is_unlit(material),
            catalog.color_map_transform(material),
            catalog.takes_model_lighting(material),
        );
        for texture in &material.textures {
            let is_normal = match texture_semantic_decodes_as_normal(texture.semantic) {
                Some(is_normal) => is_normal,
                None => continue,
            };
            let Some(image) = texture.image else {
                continue;
            };
            if catalog
                .images
                .get(image)
                .is_some_and(|image| image.decoded.is_some() || image.pending_decode.is_some())
            {
                continue;
            }
            let Some(slot) = requested.get_mut(image) else {
                continue;
            };
            let sharp = alpha_test && !is_normal;
            let linear = force_linear && !is_normal;
            match slot {
                None => *slot = Some((texture.sampler_state, is_normal, sharp, linear)),
                Some((_, already_normal, already_sharp, already_linear)) => {
                    *already_normal |= is_normal;
                    *already_sharp |= sharp;
                    *already_linear |= linear;
                }
            }
        }
    }
    requested
        .into_iter()
        .enumerate()
        .filter_map(|(image_index, request)| request.map(|request| (image_index, request)))
        .collect()
}

pub fn decode_color_or_2d_for_names(
    zone_ff: &Path,
    catalog: &mut MaterialCatalog,
    names: impl IntoIterator<Item = impl AsRef<str>>,
    stage: &LoadStage,
) -> Result<usize, String> {
    let work = requested_named_2d_slots(catalog, names);
    if work.is_empty() {
        return Ok(0);
    }
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    stage.total(work.len() as u64);
    let images = &catalog.images;
    let decoded = decode_requests_in_parallel(images, index.as_ref(), &work, stage);
    let mut n = 0usize;
    for outcome in decoded {
        if let ImageOutcome::Decoded { image_index, image } = outcome {
            catalog.images[image_index].decoded = Some(image);
            n += 1;
        }
    }
    Ok(n)
}

fn requested_named_2d_slots(
    catalog: &MaterialCatalog,
    names: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<ImageRequest> {
    let wanted: std::collections::HashSet<String> = names
        .into_iter()
        .map(|n| crate::AssetRef::bare_name(n.as_ref()).to_owned())
        .filter(|n| !n.is_empty())
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut work: Vec<ImageRequest> = Vec::new();
    for material in &catalog.materials {
        if !material.name.is_real() {
            continue;
        }
        let bind = material.name.as_str();
        if bind.is_empty() || !wanted.contains(bind) {
            continue;
        }
        let tex = material
            .textures
            .iter()
            .find(|t| t.semantic == TS_COLOR_MAP && t.image.is_some())
            .or_else(|| {
                material
                    .textures
                    .iter()
                    .find(|t| t.semantic == TS_2D && t.image.is_some())
            });
        let Some(texture) = tex else {
            continue;
        };
        let Some(image) = texture.image else {
            continue;
        };
        if catalog
            .images
            .get(image)
            .is_some_and(|img| img.decoded.is_some() || img.pending_decode.is_some())
        {
            continue;
        }
        if work.iter().any(|(already, _)| *already == image) {
            continue;
        }
        work.push((image, (texture.sampler_state, false, false, false)));
    }
    work
}

pub fn decode_catalog_images_from_iwd(
    zone_ff: &Path,
    catalog: &mut MaterialCatalog,
    images: impl IntoIterator<Item = (usize, u8)>,
    stage: &LoadStage,
) -> Result<usize, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut work: Vec<ImageRequest> = Vec::new();
    for (image, sampler_state) in images {
        if !seen.insert(image) {
            continue;
        }
        if catalog
            .images
            .get(image)
            .is_some_and(|img| img.decoded.is_some())
        {
            continue;
        }
        work.push((image, (sampler_state, false, false, true)));
    }
    if work.is_empty() {
        return Ok(0);
    }
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    stage.total(work.len() as u64);
    let catalog_images = &catalog.images;
    let decoded = decode_requests_in_parallel(catalog_images, index.as_ref(), &work, stage);
    let mut n = 0usize;
    for outcome in decoded {
        if let ImageOutcome::Decoded { image_index, image } = outcome {
            catalog.images[image_index].decoded = Some(image);
            n += 1;
        }
    }
    Ok(n)
}

pub fn decode_in_zone_builtin_images(catalog: &mut MaterialCatalog) -> usize {
    let mut decoded = 0usize;
    for image in &mut catalog.images {
        if image.decoded.is_some() || image.payload.is_empty() || image.map_type != 3 {
            continue;
        }
        if !image.name.is_real() {
            continue;
        }
        let name = image.name.as_str();
        if !name.starts_with('$') {
            continue;
        }
        let Ok(mips) = decode_gfx_image(
            &image.payload,
            u32::from(image.width),
            u32::from(image.height),
            image.format,
        ) else {
            continue;
        };
        let is_normal = name.contains("normal");
        let format = DecodedMips::texture_format(mips.storage, is_normal || !image.use_srgb_reads);
        let levels = mips.level_count();
        let mut gpu = Image::new_uninit(
            Extent3d {
                width: mips.width,
                height: mips.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        gpu.texture_descriptor.mip_level_count = levels;
        gpu.data = Some(mips.packed());
        gpu.sampler = ImageSampler::Descriptor(sampler_from_iw4(0, levels, false));
        image.decoded = Some(gpu);
        decoded += 1;
    }
    decoded
}

enum ImageOutcome {
    Decoded { image_index: usize, image: Image },
    Missing { gap: String },
    Unsupported { gap: String },
}

type ImageRequest = (usize, (u8, bool, bool, bool));

fn decode_requests_in_parallel(
    images: &[AuthoredImage],
    index: &IwdIndex,
    work: &[ImageRequest],
    stage: &LoadStage,
) -> Vec<ImageOutcome> {
    if work.is_empty() {
        return Vec::new();
    }
    let pool = ComputeTaskPool::get_or_init(TaskPool::default);

    let chunk_size = work.len().div_ceil(pool.thread_num().max(1)).clamp(1, 64);

    pool.scope(|scope| {
        for chunk in work.chunks(chunk_size) {
            scope.spawn(async move {
                let mut out = Vec::with_capacity(chunk.len());
                for &(image_index, request) in chunk {
                    if stage.is_canceled() {
                        out.push(ImageOutcome::Missing {
                            gap: "image decode canceled: load retargeted".to_owned(),
                        });
                        continue;
                    }
                    out.push(decode_one_request(images, index, image_index, request));
                    stage.advance(1);
                }
                out
            });
        }
    })
    .into_iter()
    .flatten()
    .collect()
}

fn decode_one_request(
    images: &[AuthoredImage],
    index: &IwdIndex,
    image_index: usize,
    (sampler_state, is_normal, alpha_test_color, force_linear): (u8, bool, bool, bool),
) -> ImageOutcome {
    let Some(source) = images.get(image_index) else {
        return ImageOutcome::Missing {
            gap: format!("catalog image index {image_index} is out of bounds"),
        };
    };
    let decoded = if source.payload.is_empty() {
        let name = crate::AssetRef::bare_name(source.name.as_str());

        if name.starts_with('$') {
            return ImageOutcome::Missing {
                gap: format!(
                    "{} is a builtin alias with empty payload; body ships in code_post_gfx_mp",
                    source.name
                ),
            };
        }

        if let Some(cubemap) = index.decode_cubemap_if_skybox(name, source.map_type) {
            return match cubemap {
                Ok((size, faces)) => ImageOutcome::Decoded {
                    image_index,
                    image: pack_material_cubemap(
                        size,
                        &faces,
                        source.use_srgb_reads && !force_linear,
                    ),
                },
                Err(error) => ImageOutcome::Unsupported {
                    gap: format!("{}: {error}", source.name),
                },
            };
        }
        index.decode(name)
    } else {
        Some(decode_gfx_image(
            &source.payload,
            u32::from(source.width),
            u32::from(source.height),
            source.format,
        ))
    };
    let Some(decoded) = decoded else {
        return ImageOutcome::Missing {
            gap: format!("{} is absent from IWD", source.name),
        };
    };
    let mips = match decoded {
        Ok(decoded) => decoded,
        Err(error) => {
            return ImageOutcome::Unsupported {
                gap: format!("{}: {error}", source.name),
            };
        }
    };
    let format = DecodedMips::texture_format(
        mips.storage,
        is_normal || !source.use_srgb_reads || force_linear,
    );
    let levels = mips.level_count();

    let mut image = Image::new_uninit(
        Extent3d {
            width: mips.width,
            height: mips.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.data = Some(mips.packed());
    image.sampler =
        ImageSampler::Descriptor(sampler_from_iw4(sampler_state, levels, alpha_test_color));
    ImageOutcome::Decoded { image_index, image }
}

pub fn decode_reflection_probe_cubemap(source: &AuthoredImage) -> Result<Image, String> {
    if source.map_type != 5 || source.width == 0 || source.width != source.height {
        return Err(format!("{} is not a square cubemap", source.name));
    }
    let pixel_format = match source.format {
        21 => PixelFormat::Bgra8,
        value if value == u32::from_le_bytes(*b"DXT1") => PixelFormat::Bc1,
        _ => {
            return Err(format!(
                "{} has unsupported probe format {}",
                source.name, source.format
            ));
        }
    };
    let size = u32::from(source.width);
    let levels = size.ilog2() + 1;
    let source_level_bytes = (0..levels)
        .map(|level| {
            compressed_mip_bytes((size >> level).max(1), (size >> level).max(1), pixel_format)
        })
        .collect::<Vec<_>>();
    let face_bytes = source_level_bytes.iter().sum::<usize>();
    let needed = face_bytes * 6;
    if source.payload.len() != needed {
        return Err(format!(
            "{} probe payload: need {needed} bytes for six {size}px mip chains, have {}",
            source.name,
            source.payload.len()
        ));
    }
    let rgba_len = (0..levels)
        .map(|level| {
            let side = (size >> level).max(1);
            (side * side * 4 * 6) as usize
        })
        .sum();
    let mut data = Vec::with_capacity(rgba_len);
    let mut level_offset = 0;
    for level in 0..levels {
        let side = (size >> level).max(1);
        let level_bytes = source_level_bytes[level as usize];
        for face in 0..6 {
            let start = face * face_bytes + level_offset;
            let encoded = &source.payload[start..start + level_bytes];
            match pixel_format {
                PixelFormat::Bgra8 => {
                    for pixel in encoded.chunks_exact(4) {
                        data.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
                PixelFormat::Bc1 => {
                    data.extend_from_slice(&decode_blocks(encoded, side, side, pixel_format)?);
                }
                _ => unreachable!("probe decoder admits only BGRA8 and BC1"),
            }
        }
        level_offset += level_bytes;
    }
    let mut image = Image::new_uninit(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = levels;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        address_mode_w: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
    image.data = Some(data);
    Ok(image)
}

fn sampler_from_iw4(
    sampler_state: u8,
    levels: u32,
    alpha_test_color: bool,
) -> ImageSamplerDescriptor {
    let clamp_u = (sampler_state >> 5) & 1 != 0;
    let clamp_v = (sampler_state >> 6) & 1 != 0;
    let filter = sampler_state & 0b111;
    let mip_map = (sampler_state >> 3) & 0b11;
    let anisotropy = match filter {
        _ if levels <= 1 || alpha_test_color => 1,
        3 => 2,
        4 => 4,
        _ => 1,
    };
    let point = if alpha_test_color {
        ImageFilterMode::Nearest
    } else {
        ImageFilterMode::Linear
    };
    ImageSamplerDescriptor {
        address_mode_u: if clamp_u {
            ImageAddressMode::ClampToEdge
        } else {
            ImageAddressMode::Repeat
        },
        address_mode_v: if clamp_v {
            ImageAddressMode::ClampToEdge
        } else {
            ImageAddressMode::Repeat
        },
        address_mode_w: ImageAddressMode::Repeat,
        mag_filter: point,
        min_filter: point,
        mipmap_filter: if alpha_test_color {
            ImageFilterMode::Nearest
        } else if mip_map == 2 || anisotropy > 1 {
            ImageFilterMode::Linear
        } else {
            ImageFilterMode::Nearest
        },
        anisotropy_clamp: anisotropy,
        ..Default::default()
    }
}

fn expand_top_level(decoded: Result<DecodedMips, String>) -> Result<(u32, u32, Vec<u8>), String> {
    decoded.and_then(DecodedMips::into_top_level_rgba8)
}

pub fn decode_menu_background(games_root: &Path) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    for main in ui_decode_mains(games_root) {
        let index = IwdIndex::open(&main)?;
        match index.decode("menu_mp_image").map(expand_top_level) {
            Some(Ok(image)) => return Ok(Some(image)),
            Some(Err(error)) => {
                diag::info!(
                    Zone,
                    "menu: decode menu_mp_image from {}: {error}",
                    main.display()
                );
            }
            None => {}
        }
    }
    Ok(None)
}

pub fn decode_zone_image_rgba(
    width: u32,
    height: u32,
    format: u32,
    payload: &[u8],
) -> Result<(u32, u32, Vec<u8>), String> {
    if payload.is_empty() {
        return Err("empty in-zone payload".into());
    }
    decode_gfx_image(payload, width, height, format)?.into_top_level_rgba8()
}

pub fn decode_ui_image(
    games_root: &Path,
    image_name: &str,
) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let name = crate::AssetRef::bare_name(image_name);
    for main in ui_decode_mains(games_root) {
        match decode_ui_image_from_main(&main, name)? {
            Some(image) => return Ok(Some(image)),
            None => {}
        }
    }
    Ok(None)
}

pub fn decode_ui_image_from_main(
    main: &Path,
    image_name: &str,
) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let name = crate::AssetRef::bare_name(image_name);
    let index = IwdIndex::open(main)?;
    match index.decode(name).map(expand_top_level) {
        Some(Ok(image)) => Ok(Some(image)),
        Some(Err(error)) => {
            diag::warn!(
                Zone,
                "ui-image: decode {name} from {}: {error}",
                main.display()
            );
            Ok(None)
        }
        None => Ok(None),
    }
}

fn ui_decode_mains(games_root: &Path) -> Vec<PathBuf> {
    let mut mains = game_mains_under(games_root);
    mains.sort_by(
        |a, b| match (IwdIndex::is_cached(a), IwdIndex::is_cached(b)) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        },
    );
    mains
}

fn game_mains_under(games_root: &Path) -> Vec<PathBuf> {
    asset_transport::game_mains_under(games_root)
}

pub fn decode_map_preview(
    zone_ff: &Path,
    map_name: &str,
) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
    let main = game_main_for_zone(zone_ff)?;
    let index = IwdIndex::open(&main)?;
    for stem in [
        format!("loadscreen_{map_name}"),
        format!("preview_{map_name}"),
    ] {
        match index.decode(&stem).map(expand_top_level) {
            Some(Ok(image)) => return Ok(Some(image)),
            Some(Err(error)) => {
                diag::warn!(Zone, "loading: decode {stem}: {error}");
            }
            None => {}
        }
    }
    Ok(None)
}

pub fn game_main_for_zone(zone_ff: &Path) -> Result<PathBuf, String> {
    asset_transport::game_main_for_zone(zone_ff)
}

const MIP_CACHE_FORMAT: u32 = 2;
const MIP_MAGIC: &[u8; 8] = b"IWL1MIPS";
static MIP_HIT: AtomicU64 = AtomicU64::new(0);
static MIP_MISS: AtomicU64 = AtomicU64::new(0);
static MIP_IO_NS: AtomicU64 = AtomicU64::new(0);

pub fn mip_cache_cost() -> (u64, u64, f64) {
    (
        MIP_HIT.load(Ordering::Relaxed),
        MIP_MISS.load(Ordering::Relaxed),
        MIP_IO_NS.load(Ordering::Relaxed) as f64 / 1.0e6,
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ImageWorkingSet {
    pub decoded_n: u64,
    pub decoded_bytes: u64,
    pub world_n: u64,
    pub world_bytes: u64,
    pub smodel_n: u64,
    pub smodel_bytes: u64,
    pub fpv_n: u64,
    pub fpv_bytes: u64,
    pub probe_n: u64,
    pub probe_bytes: u64,
    pub lightmap_n: u64,
    pub lightmap_bytes: u64,
    pub fx_n: u64,
    pub fx_bytes: u64,
}

static LAST_WORKING_SET: RwLock<ImageWorkingSet> = RwLock::new(ImageWorkingSet {
    decoded_n: 0,
    decoded_bytes: 0,
    world_n: 0,
    world_bytes: 0,
    smodel_n: 0,
    smodel_bytes: 0,
    fpv_n: 0,
    fpv_bytes: 0,
    probe_n: 0,
    probe_bytes: 0,
    lightmap_n: 0,
    lightmap_bytes: 0,
    fx_n: 0,
    fx_bytes: 0,
});

pub fn last_image_working_set() -> ImageWorkingSet {
    *LAST_WORKING_SET
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn store_image_working_set(set: ImageWorkingSet) {
    *LAST_WORKING_SET
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = set;
}

pub fn cpu_image_census<'a>(images: impl IntoIterator<Item = &'a Image>) -> (u64, u64) {
    let mut n = 0u64;
    let mut bytes = 0u64;
    for image in images {
        n = n.saturating_add(1);
        bytes = bytes.saturating_add(
            image
                .data
                .as_ref()
                .map(|data| data.len() as u64)
                .unwrap_or(0),
        );
    }
    (n, bytes)
}

pub fn census_image_working_set(
    catalog: &MaterialCatalog,
    world_materials: impl IntoIterator<Item = usize>,
    smodel_materials: impl IntoIterator<Item = usize>,
    fpv_materials: impl IntoIterator<Item = usize>,
    fx_materials: impl IntoIterator<Item = usize>,
) -> ImageWorkingSet {
    fn bound_images(
        catalog: &MaterialCatalog,
        mats: impl IntoIterator<Item = usize>,
    ) -> (u64, u64) {
        let mut seen = HashSet::new();
        for mi in mats {
            let Some(material) = catalog.materials.get(mi) else {
                continue;
            };
            for texture in &material.textures {
                if let Some(image) = texture.image {
                    seen.insert(image);
                }
            }
        }
        let mut bytes = 0u64;
        for image in &seen {
            if let Some(authored) = catalog.images.get(*image)
                && let Some(decoded) = &authored.decoded
            {
                bytes += decoded
                    .data
                    .as_ref()
                    .map(|data| data.len() as u64)
                    .unwrap_or(0);
            }
        }
        (seen.len() as u64, bytes)
    }
    let mem = catalog.image_memory();
    let (world_n, world_bytes) = bound_images(catalog, world_materials);
    let (smodel_n, smodel_bytes) = bound_images(catalog, smodel_materials);
    let (fpv_n, fpv_bytes) = bound_images(catalog, fpv_materials);
    let (fx_n, fx_bytes) = bound_images(catalog, fx_materials);
    ImageWorkingSet {
        decoded_n: mem.decoded_images as u64,
        decoded_bytes: mem.decoded_bytes as u64,
        world_n,
        world_bytes,
        smodel_n,
        smodel_bytes,
        fpv_n,
        fpv_bytes,
        probe_n: 0,
        probe_bytes: 0,
        lightmap_n: 0,
        lightmap_bytes: 0,
        fx_n,
        fx_bytes,
    }
}

fn read_cached_mips(key: &str, io_at: std::time::Instant) -> Option<DecodedMips> {
    let hit = crate::cache_get("mips", key)?;
    MIP_IO_NS.fetch_add(io_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    let mips = DecodedMips::cache_decode(&hit)?;
    MIP_HIT.fetch_add(1, Ordering::Relaxed);
    Some(mips)
}

fn mip_cache_key(crc: u32, size: u64, entry: &str) -> String {
    format!(
        "{MIP_CACHE_FORMAT:08x}-{crc:08x}-{size:x}-{:016x}",
        crate::fnv1a64(entry.as_bytes())
    )
}

fn load_or_decode_mips(candidate: &asset_transport::IwdFile) -> Result<DecodedMips, String> {
    let key = mip_cache_key(candidate.crc32(), candidate.size(), candidate.entry());
    let io_at = std::time::Instant::now();
    if let Some(mips) = read_cached_mips(&key, io_at) {
        return Ok(mips);
    }

    let _flight = crate::cache_flight("mips", &key);
    if let Some(mips) = read_cached_mips(&key, std::time::Instant::now()) {
        return Ok(mips);
    }
    let bytes = candidate.read()?;
    let mips = decode_iwi_mips(&bytes)?;
    let blob = mips.cache_encode();
    let store_at = std::time::Instant::now();
    if let Err(error) = crate::cache_put("mips", &key, &blob) {
        diag::warn!(Zone, "mip cache store {key}: {error}");
    }
    MIP_IO_NS.fetch_add(store_at.elapsed().as_nanos() as u64, Ordering::Relaxed);
    MIP_MISS.fetch_add(1, Ordering::Relaxed);
    Ok(mips)
}

pub fn iwd_read_cost() -> (f64, u64, f64) {
    asset_transport::iwd_read_cost()
}

fn decode_iwi_mips(bytes: &[u8]) -> Result<DecodedMips, String> {
    if bytes.len() >= IWI_V8_HEADER_LEN
        && bytes[..3] == *b"IWi"
        && bytes[3] == 8
        && let Ok(header) = IwiHeader::parse(bytes)
        && img_format_info(header.format).is_some_and(|info| info.kind == ImgFormatKind::Wavelet)
    {
        return decode_wavelet_iwi(bytes, header);
    }
    let header = parse_iwi_header(bytes)?;
    let end = header.mip0_end.min(bytes.len());
    let start = if (header.header_len..end).contains(&header.mip0_start) {
        header.mip0_start
    } else {
        header.header_len
    };
    let payload = bytes
        .get(start..end)
        .ok_or_else(|| "truncated IWI top mip".to_owned())?;
    let storage = mip_storage(header.format);

    let single = || -> Result<DecodedMips, String> {
        let (w, h, pixels) = load_mip_level(payload, header.width, header.height, header.format)?;
        Ok(match storage {
            MipStorage::Rgba8 => DecodedMips::single(w, h, pixels),
            other => DecodedMips::single_compressed(w, h, other, pixels),
        })
    };
    let count = mip_level_count(header.width, header.height);
    if count <= 1 {
        return single();
    }

    let mut cursor = if header.mip0_end <= bytes.len() {
        header.mip0_end
    } else {
        bytes.len()
    };
    let mut ranges = Vec::with_capacity(count as usize);
    for level in 0..count {
        let (w, h) = (
            (header.width >> level).max(1),
            (header.height >> level).max(1),
        );
        let size = compressed_mip_bytes(w, h, header.format);
        let Some(level_start) = cursor.checked_sub(size) else {
            return single();
        };
        ranges.push((level_start, cursor, w, h));
        cursor = level_start;
    }
    if cursor != header.header_len {
        return single();
    }

    let mut levels = Vec::with_capacity(ranges.len());
    for (level_start, level_end, w, h) in ranges {
        match load_mip_level(&bytes[level_start..level_end], w, h, header.format) {
            Ok((_, _, pixels)) => levels.push(pixels),
            Err(_) => return single(),
        }
    }
    Ok(DecodedMips {
        width: header.width,
        height: header.height,
        levels,
        storage,
    })
}

fn decode_wavelet_iwi(bytes: &[u8], header: IwiHeader) -> Result<DecodedMips, String> {
    let format = wavelet_check_header(&header).map_err(wavelet_error)?;
    let info = img_format_info(format).ok_or_else(|| format!("unsupported IWI format {format}"))?;
    let channels = info.channels;
    let stride = wavelet_pixel_stride(channels);
    let pixel_format = wavelet_d3d_pixel_format(format)?;
    let payload = bytes
        .get(IWI_V8_HEADER_LEN..)
        .ok_or_else(|| "truncated wavelet IWI payload".to_owned())?;
    let mut bits = WaveletBits::new(payload);
    let mut parent = Vec::new();
    let mut d3d_levels = Vec::new();
    let width = u32::from(header.width);
    let height = u32::from(header.height);
    for level in (0..=wavelet_top_level(&header)).rev() {
        let w = wavelet_level_size(width, level);
        let h = wavelet_level_size(height, level);
        let mut plane = vec![0u8; w as usize * h as usize * stride];
        wavelet_decompress_level(&mut bits, &mut parent, &mut plane, w, h, channels, stride)
            .map_err(wavelet_error)?;
        d3d_levels.push(plane.clone());
        parent = plane;
    }
    bits.check_landing().map_err(wavelet_error)?;
    d3d_levels.reverse();
    let mut levels = Vec::with_capacity(d3d_levels.len());
    for (i, d3d) in d3d_levels.iter().enumerate() {
        let w = (width >> i).max(1);
        let h = (height >> i).max(1);
        let (_, _, rgba) = load_mip_level(d3d, w, h, pixel_format)?;
        levels.push(rgba);
    }
    Ok(DecodedMips {
        width,
        height,
        levels,
        storage: MipStorage::Rgba8,
    })
}

fn wavelet_d3d_pixel_format(format: u8) -> Result<PixelFormat, String> {
    match format {
        6 => Ok(PixelFormat::Bgra8),
        7 => Ok(PixelFormat::Bgrx8),
        8 => Ok(PixelFormat::La8),
        9 => Ok(PixelFormat::L8),
        10 => Ok(PixelFormat::A8),
        other => Err(format!("unsupported IWI format {other}")),
    }
}

fn wavelet_error(error: WaveletError) -> String {
    match error {
        WaveletError::Truncated { needed, have } => {
            format!("wavelet truncated: needed {needed} have {have}")
        }
        WaveletError::Dimensions => "wavelet level is not even 2D".into(),
        WaveletError::UnsupportedFormat(format) => format!("unsupported IWI format {format}"),
        WaveletError::CubemapOrVolume => "wavelet cubemap/volume is unlocated".into(),
        WaveletError::BadLanding { pos, end } => {
            format!("wavelet cursor landed at {pos}, payload ends at {end}")
        }
    }
}

fn mip_storage(format: PixelFormat) -> MipStorage {
    match format {
        PixelFormat::Bc1 => MipStorage::Bc1,
        PixelFormat::Bc2 => MipStorage::Bc2,
        PixelFormat::Bc3 => MipStorage::Bc3,
        _ => MipStorage::Rgba8,
    }
}

fn load_mip_level(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<(u32, u32, Vec<u8>), String> {
    match format {
        PixelFormat::Bc1 | PixelFormat::Bc2 | PixelFormat::Bc3 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC image".to_owned())?;
            Ok((width, height, source.to_vec()))
        }
        _ => decode_pixels(data, width, height, format),
    }
}

struct IwiHeaderInfo {
    width: u32,
    height: u32,
    format: PixelFormat,
    header_len: usize,

    mip0_end: usize,

    mip0_start: usize,
}

fn parse_iwi_header(bytes: &[u8]) -> Result<IwiHeaderInfo, String> {
    if bytes.len() < 4 || &bytes[..3] != b"IWi" {
        return Err("unsupported IWI header".into());
    }
    match bytes[3] {
        8 => {
            if bytes.len() < 32 {
                return Err("truncated IWI v8 header".into());
            }
            let width = u32::from(u16::from_le_bytes([bytes[10], bytes[11]]));
            let height = u32::from(u16::from_le_bytes([bytes[12], bytes[13]]));
            let mip0_end = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
            let mip0_start = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
            Ok(IwiHeaderInfo {
                width,
                height,
                format: iwi_pixel_format(bytes[8])?,
                header_len: 32,
                mip0_end,
                mip0_start,
            })
        }

        13 => {
            if bytes.len() < 48 {
                return Err("truncated IWI v13 header".into());
            }
            let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
            let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
            let mip0_end = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;

            let mip0_start = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
            Ok(IwiHeaderInfo {
                width,
                height,
                format: iwi_pixel_format(bytes[4])?,
                header_len: 48,
                mip0_end,
                mip0_start,
            })
        }
        version => Err(format!("unsupported IWI version {version}")),
    }
}

fn iwi_pixel_format(format: u8) -> Result<PixelFormat, String> {
    match format {
        1 => Ok(PixelFormat::Bgra8),
        2 => Ok(PixelFormat::Rgb8),
        3 => Ok(PixelFormat::La8),
        4 => Ok(PixelFormat::L8),
        5 => Ok(PixelFormat::A8),
        11 => Ok(PixelFormat::Bc1),
        12 => Ok(PixelFormat::Bc2),
        13 => Ok(PixelFormat::Bc3),
        format => Err(format!("unsupported IWI format {format}")),
    }
}

fn mip_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}

fn compressed_mip_bytes(width: u32, height: u32, format: PixelFormat) -> usize {
    match format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 | PixelFormat::Bgrx8 => {
            width as usize * height as usize * 4
        }
        PixelFormat::Rgb8 => width as usize * height as usize * 3,
        PixelFormat::La8 => width as usize * height as usize * 2,
        PixelFormat::L8 | PixelFormat::A8 => width as usize * height as usize,
        PixelFormat::Bc1 => width.div_ceil(4) as usize * height.div_ceil(4) as usize * 8,
        PixelFormat::Bc2 | PixelFormat::Bc3 => {
            width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16
        }
    }
}

fn decode_gfx_image(
    bytes: &[u8],
    width: u32,
    height: u32,
    format: u32,
) -> Result<DecodedMips, String> {
    let format = match format {
        value if value == u32::from_le_bytes(*b"DXT1") => PixelFormat::Bc1,
        value if value == u32::from_le_bytes(*b"DXT3") => PixelFormat::Bc2,
        value if value == u32::from_le_bytes(*b"DXT5") => PixelFormat::Bc3,
        21 => PixelFormat::Bgra8,
        22 => PixelFormat::Bgrx8,
        28 => PixelFormat::A8,
        50 => PixelFormat::L8,
        51 => PixelFormat::La8,
        format => return Err(format!("unsupported D3D format {format}")),
    };
    let (w, h, pixels) = load_mip_level(bytes, width, height, format)?;
    Ok(match mip_storage(format) {
        MipStorage::Rgba8 => DecodedMips::single(w, h, pixels),
        storage => DecodedMips::single_compressed(w, h, storage, pixels),
    })
}

#[derive(Clone, Copy)]
enum PixelFormat {
    #[allow(dead_code)]
    Rgba8,
    Rgb8,
    Bgra8,
    Bgrx8,
    L8,
    La8,
    A8,
    Bc1,
    Bc2,
    Bc3,
}

fn decode_pixels(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<(u32, u32, Vec<u8>), String> {
    if width == 0 || height == 0 {
        return Err("zero-sized image".into());
    }
    let pixels = match format {
        PixelFormat::Rgba8 => take_rgba(data, width, height, false, false)?,
        PixelFormat::Bgra8 => take_rgba(data, width, height, true, false)?,
        PixelFormat::Bgrx8 => take_rgba(data, width, height, true, true)?,
        PixelFormat::Rgb8 => {
            let needed = width as usize * height as usize * 3;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated RGB8".to_owned())?;
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in source.chunks_exact(3) {
                out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
            out
        }
        PixelFormat::L8 | PixelFormat::A8 => {
            let needed = width as usize * height as usize;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated L8/A8".to_owned())?;
            let mut out = Vec::with_capacity(needed * 4);
            for &value in source {
                if matches!(format, PixelFormat::A8) {
                    out.extend_from_slice(&[255, 255, 255, value]);
                } else {
                    out.extend_from_slice(&[value, value, value, 255]);
                }
            }
            out
        }
        PixelFormat::La8 => {
            let needed = width as usize * height as usize * 2;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated LA8".to_owned())?;
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in source.chunks_exact(2) {
                out.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            out
        }
        PixelFormat::Bc1 | PixelFormat::Bc2 | PixelFormat::Bc3 => {
            decode_blocks(data, width, height, format)?
        }
    };
    Ok((width, height, pixels))
}

fn take_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    bgra: bool,
    opaque: bool,
) -> Result<Vec<u8>, String> {
    let needed = width as usize * height as usize * 4;
    let mut out = data
        .get(..needed)
        .ok_or_else(|| "truncated RGBA8".to_owned())?
        .to_vec();
    for pixel in out.chunks_exact_mut(4) {
        if bgra {
            pixel.swap(0, 2);
        }
        if opaque {
            pixel[3] = 255;
        }
    }
    Ok(out)
}

pub fn decoded_image_top_level_rgba8(image: &Image) -> Result<(u32, u32, Vec<u8>), String> {
    if image.texture_descriptor.dimension != TextureDimension::D2
        || image.texture_descriptor.size.depth_or_array_layers != 1
    {
        return Err("decoded image is not a single 2D layer".to_owned());
    }
    let width = image.texture_descriptor.size.width;
    let height = image.texture_descriptor.size.height;
    let data = image
        .data
        .as_deref()
        .ok_or_else(|| "decoded image has no CPU data".to_owned())?;
    let pixels = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => {
            take_rgba(data, width, height, false, false)?
        }
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
            take_rgba(data, width, height, true, false)?
        }
        TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb => {
            decode_blocks(data, width, height, PixelFormat::Bc1)?
        }
        TextureFormat::Bc2RgbaUnorm | TextureFormat::Bc2RgbaUnormSrgb => {
            decode_blocks(data, width, height, PixelFormat::Bc2)?
        }
        TextureFormat::Bc3RgbaUnorm | TextureFormat::Bc3RgbaUnormSrgb => {
            decode_blocks(data, width, height, PixelFormat::Bc3)?
        }
        format => return Err(format!("unsupported decoded image format {format:?}")),
    };
    Ok((width, height, pixels))
}

fn decode_blocks(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    let block_size = if matches!(format, PixelFormat::Bc1) {
        8
    } else {
        16
    };
    let blocks_wide = width.div_ceil(4) as usize;
    let blocks_high = height.div_ceil(4) as usize;
    let needed = blocks_wide * blocks_high * block_size;
    if data.len() < needed {
        return Err("truncated BC image".into());
    }
    let mut out = vec![0; width as usize * height as usize * 4];
    let mut tile = [0u8; 64];
    for block_y in 0..blocks_high {
        for block_x in 0..blocks_wide {
            let offset = (block_y * blocks_wide + block_x) * block_size;
            match format {
                PixelFormat::Bc1 => bcdec_rs::bc1(&data[offset..offset + 8], &mut tile, 16),
                PixelFormat::Bc2 => bcdec_rs::bc2(&data[offset..offset + 16], &mut tile, 16),
                PixelFormat::Bc3 => bcdec_rs::bc3(&data[offset..offset + 16], &mut tile, 16),
                _ => unreachable!(),
            }
            for y in 0..4 {
                for x in 0..4 {
                    let destination_x = block_x * 4 + x;
                    let destination_y = block_y * 4 + y;
                    if destination_x < width as usize && destination_y < height as usize {
                        let destination = (destination_y * width as usize + destination_x) * 4;
                        let source = (y * 4 + x) * 4;
                        out[destination..destination + 4]
                            .copy_from_slice(&tile[source..source + 4]);
                    }
                }
            }
        }
    }
    Ok(out)
}

pub fn retail_lit_color(albedo_rgb: [f32; 3], lighting: [f32; 3]) -> [f32; 3] {
    let albedo = lighting_iw4::lit_albedo(albedo_rgb, [1.0, 1.0, 1.0]);
    lighting_iw4::lit_fragment_color(albedo, lighting, [0.0, 0.0, 0.0])
}

fn decode_iwi_cubemap(bytes: &[u8]) -> Result<(u32, CubemapFaces), String> {
    let header = parse_iwi_header(bytes)?;
    if header.width == 0 || header.width != header.height {
        return Err(format!(
            "non-square cubemap {}x{}",
            header.width, header.height
        ));
    }
    let face_bytes = match header.format {
        PixelFormat::Rgba8 | PixelFormat::Bgra8 | PixelFormat::Bgrx8 => {
            (header.width * header.height * 4) as usize
        }
        PixelFormat::Rgb8 => (header.width * header.height * 3) as usize,
        PixelFormat::Bc1 => (header.width.div_ceil(4) * header.height.div_ceil(4) * 8) as usize,
        PixelFormat::Bc2 | PixelFormat::Bc3 => {
            (header.width.div_ceil(4) * header.height.div_ceil(4) * 16) as usize
        }
        _ => return Err("unsupported cubemap pixel layout".into()),
    };

    let start = header
        .mip0_end
        .checked_sub(face_bytes * 6)
        .filter(|&start| start >= header.header_len && header.mip0_end <= bytes.len())
        .ok_or_else(|| "truncated cubemap top mip".to_owned())?;
    let mut faces = std::array::from_fn(|_| Vec::new());
    for (i, face) in faces.iter_mut().enumerate() {
        let start = start + i * face_bytes;
        let (_, _, rgba) = decode_pixels(
            &bytes[start..start + face_bytes],
            header.width,
            header.height,
            header.format,
        )?;
        *face = rgba;
    }
    Ok((header.width, faces))
}

fn pack_material_cubemap(size: u32, faces: &CubemapFaces, srgb: bool) -> Image {
    let mut pixels = Vec::with_capacity(6 * size as usize * size as usize * 4);
    for face in faces {
        pixels.extend_from_slice(face);
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size * 6,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image
        .reinterpret_stacked_2d_as_array(6)
        .expect("six equal cubemap faces");
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut sampler = ImageSamplerDescriptor::linear();
    sampler.address_mode_u = ImageAddressMode::ClampToEdge;
    sampler.address_mode_v = ImageAddressMode::ClampToEdge;
    sampler.address_mode_w = ImageAddressMode::ClampToEdge;
    image.sampler = ImageSampler::Descriptor(sampler);
    image
}

pub const NORMAL_DECODE_SCALE: [f32; 2] = [4.08, 4.06452];
pub const NORMAL_DECODE_BIAS: [f32; 2] = [-2.08, -2.06452];

pub fn decode_dxt5nm_xy(rgba: [f32; 4]) -> [f32; 2] {
    [
        rgba[3] * NORMAL_DECODE_SCALE[0] + NORMAL_DECODE_BIAS[0],
        rgba[1] * NORMAL_DECODE_SCALE[1] + NORMAL_DECODE_BIAS[1],
    ]
}

pub fn retail_lightmap_bake(
    page0_rgb: [f32; 3],
    page1_rgb: [f32; 3],
    lm_dir: [f32; 2],
    n_ts: [f32; 2],
) -> [f32; 3] {
    let inv_n = (n_ts[0] * n_ts[0] + n_ts[1] * n_ts[1] + 1.0).sqrt().recip();
    let inv_l = (lm_dir[0] * lm_dir[0] + lm_dir[1] * lm_dir[1] + 1.0)
        .sqrt()
        .recip();
    let bake_ndotl =
        ((lm_dir[0] * n_ts[0] + lm_dir[1] * n_ts[1] + 1.0) * inv_l * inv_n).clamp(0.0, 1.0);
    [
        {
            let v = page0_rgb[0] * inv_n + page1_rgb[0] * bake_ndotl;
            v * v
        },
        {
            let v = page0_rgb[1] * inv_n + page1_rgb[1] * bake_ndotl;
            v * v
        },
        {
            let v = page0_rgb[2] * inv_n + page1_rgb[2] * bake_ndotl;
            v * v
        },
    ]
}

pub struct ImageDemandPlan {
    id: u64,
    zone_ff: PathBuf,

    demands: Vec<AuthoredImage>,
    requests: Vec<(u8, bool, bool, bool)>,
}

#[derive(Default)]
pub struct DecodedImageBatch {
    plan: Option<u64>,
    decoded: Vec<(String, Image)>,
    pub stats: MaterialImageStats,
}

impl ImageDemandPlan {
    fn new(zone_ff: &Path) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            zone_ff: zone_ff.to_owned(),
            demands: Vec::new(),
            requests: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.demands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.demands.is_empty()
    }

    fn push(&mut self, image: AuthoredImage, request: (u8, bool, bool, bool)) {
        if let Some(at) = self
            .demands
            .iter()
            .position(|demand| demand.name.as_str() == image.name.as_str())
        {
            let (_, already_normal, already_sharp, already_linear) = &mut self.requests[at];
            *already_normal |= request.1;
            *already_sharp |= request.2;
            *already_linear |= request.3;
            return;
        }
        self.demands.push(image);
        self.requests.push(request);
    }

    pub fn run(self, stage: &LoadStage) -> DecodedImageBatch {
        if self.demands.is_empty() {
            return DecodedImageBatch::default();
        }
        let index = match game_main_for_zone(&self.zone_ff).and_then(|main| IwdIndex::open(&main)) {
            Ok(index) => index,
            Err(error) => {
                return DecodedImageBatch {
                    plan: Some(self.id),
                    decoded: Vec::new(),
                    stats: MaterialImageStats {
                        requested: self.demands.len(),
                        missing: self.demands.len(),
                        first_gap: Some(error),
                        ..Default::default()
                    },
                };
            }
        };
        let work = self
            .requests
            .iter()
            .copied()
            .enumerate()
            .collect::<Vec<ImageRequest>>();
        let mut stats = MaterialImageStats {
            archives: index.0.archive_count(),
            requested: work.len(),
            ..Default::default()
        };
        stage.total(work.len() as u64);
        let outcomes = decode_requests_in_parallel(&self.demands, index.as_ref(), &work, stage);
        let mut decoded = Vec::with_capacity(outcomes.len());
        for outcome in outcomes {
            match outcome {
                ImageOutcome::Decoded { image_index, image } => {
                    let Some(source) = self.demands.get(image_index) else {
                        continue;
                    };
                    decoded.push((source.name.as_str().to_owned(), image));
                    stats.decoded += 1;
                }
                ImageOutcome::Missing { gap } => {
                    stats.missing += 1;
                    stats.first_gap.get_or_insert(gap);
                }
                ImageOutcome::Unsupported { gap } => {
                    stats.unsupported += 1;
                    stats.first_unsupported.get_or_insert(gap);
                }
            }
        }
        DecodedImageBatch {
            plan: Some(self.id),
            decoded,
            stats,
        }
    }
}

impl DecodedImageBatch {
    pub fn apply(self, catalog: &mut MaterialCatalog) -> (usize, usize, usize) {
        let Some(id) = self.plan else {
            return (0, 0, 0);
        };
        let mut decoded: HashMap<String, Image> = self.decoded.into_iter().collect();

        let mut rows: HashMap<String, Vec<usize>> = HashMap::new();
        let mut already = 0usize;
        for (index, image) in catalog.images.iter_mut().enumerate() {
            if image.pending_decode != Some(id) {
                continue;
            }
            image.pending_decode = None;
            if image.decoded.is_some() {
                already += 1;
                continue;
            }
            rows.entry(image.name.as_str().to_owned())
                .or_default()
                .push(index);
        }
        let mut filled = 0usize;
        for (name, indices) in rows {
            let Some(image) = decoded.remove(&name) else {
                continue;
            };
            let mut indices = indices.into_iter();
            let Some(first) = indices.next() else {
                continue;
            };
            for index in indices {
                if let Some(slot) = catalog.images.get_mut(index) {
                    slot.decoded = Some(image.clone());
                    filled += 1;
                }
            }
            if let Some(slot) = catalog.images.get_mut(first) {
                slot.decoded = Some(image);
                filled += 1;
            }
        }

        let lost = decoded.len();
        (filled, already, lost)
    }
}

pub fn plan_material_color_maps(
    zone_ff: &Path,
    catalog: &mut MaterialCatalog,
    stage: &LoadStage,
) -> (MaterialImageStats, ImageDemandPlan) {
    let mut plan = ImageDemandPlan::new(zone_ff);
    let requested = requested_color_map_slots(catalog);
    let inline = claim(catalog, &mut plan, requested);
    let stats = decode_inline(catalog, &inline, stage);
    (stats, plan)
}

fn claim(
    catalog: &mut MaterialCatalog,
    plan: &mut ImageDemandPlan,
    requested: Vec<ImageRequest>,
) -> Vec<ImageRequest> {
    let id = plan.id;
    let mut inline = Vec::new();
    for (image_index, request) in requested {
        let Some(image) = catalog.images.get_mut(image_index) else {
            continue;
        };
        if image.payload.is_empty() {
            image.pending_decode = Some(id);
            let mut owned = image.clone();
            owned.decoded = None;
            plan.push(owned, request);
        } else {
            inline.push((image_index, request));
        }
    }
    inline
}

pub fn plan_color_or_2d_for_names(
    plan: &mut ImageDemandPlan,
    catalog: &mut MaterialCatalog,
    names: impl IntoIterator<Item = impl AsRef<str>>,
    stage: &LoadStage,
) -> usize {
    let requested = requested_named_2d_slots(catalog, names);
    let inline = claim(catalog, plan, requested);
    decode_inline(catalog, &inline, stage).decoded
}

fn decode_inline(
    catalog: &mut MaterialCatalog,
    work: &[ImageRequest],
    stage: &LoadStage,
) -> MaterialImageStats {
    let mut stats = MaterialImageStats {
        requested: work.len(),
        ..Default::default()
    };
    if work.is_empty() {
        return stats;
    }

    let index = IwdIndex(Arc::new(asset_transport::IwdIndex::default()));
    let images = &catalog.images;
    for outcome in decode_requests_in_parallel(images, &index, work, stage) {
        match outcome {
            ImageOutcome::Decoded { image_index, image } => {
                catalog.images[image_index].decoded = Some(image);
                stats.decoded += 1;
            }
            ImageOutcome::Missing { gap } => {
                stats.missing += 1;
                stats.first_gap.get_or_insert(gap);
            }
            ImageOutcome::Unsupported { gap } => {
                stats.unsupported += 1;
                stats.first_unsupported.get_or_insert(gap);
            }
        }
    }
    stats
}
