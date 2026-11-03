use asset_iw4::{
    IWI_V8_HEADER_LEN, ImgFormatKind, IwiHeader, WaveletBits, WaveletError, img_format_info,
    wavelet_check_header, wavelet_decompress_level, wavelet_level_size, wavelet_pixel_stride,
    wavelet_top_level,
};

pub(super) struct DecodedMips {
    width: u32,
    height: u32,

    packed: Vec<u8>,
    level_sizes: Vec<u32>,
    storage: MipStorage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MipStorage {
    Rgba8,
    R32Float,
    Bc1,
    Bc2,
    Bc3,
    Bc5,
}

impl DecodedMips {
    pub(super) fn single(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self::single_compressed(width, height, MipStorage::Rgba8, pixels)
    }

    fn single_compressed(width: u32, height: u32, storage: MipStorage, bytes: Vec<u8>) -> Self {
        Self {
            width,
            height,
            level_sizes: vec![bytes.len() as u32],
            packed: bytes,
            storage,
        }
    }

    fn from_levels(width: u32, height: u32, storage: MipStorage, levels: Vec<Vec<u8>>) -> Self {
        let mut packed = Vec::with_capacity(levels.iter().map(Vec::len).sum());
        let mut level_sizes = Vec::with_capacity(levels.len());
        for level in levels {
            level_sizes.push(level.len() as u32);
            packed.extend_from_slice(&level);
        }
        Self {
            width,
            height,
            packed,
            level_sizes,
            storage,
        }
    }

    pub(super) fn layout(&self) -> MipLayout {
        MipLayout {
            width: self.width,
            height: self.height,
            storage: self.storage,
            levels: self.level_count(),
        }
    }
    pub(super) fn payload(&self) -> &[u8] {
        &self.packed
    }
    pub(super) fn level_sizes(&self) -> &[u32] {
        &self.level_sizes
    }
    pub(super) fn take_payload(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.packed)
    }
    pub(super) fn from_packed(
        width: u32,
        height: u32,
        storage: MipStorage,
        packed: Vec<u8>,
        level_sizes: Vec<u32>,
    ) -> Option<Self> {
        if width == 0
            || height == 0
            || level_sizes.is_empty()
            || level_sizes.len() > mip_level_count(width, height) as usize
        {
            return None;
        }
        let mut total = 0usize;
        for (level, &size) in level_sizes.iter().enumerate() {
            let w = (width >> level).max(1) as usize;
            let h = (height >> level).max(1) as usize;
            let expected = match storage {
                MipStorage::Rgba8 | MipStorage::R32Float => w.checked_mul(h)?.checked_mul(4)?,
                MipStorage::Bc1 => w.div_ceil(4).checked_mul(h.div_ceil(4))?.checked_mul(8)?,
                MipStorage::Bc2 | MipStorage::Bc3 | MipStorage::Bc5 => {
                    w.div_ceil(4).checked_mul(h.div_ceil(4))?.checked_mul(16)?
                }
            };
            if size as usize != expected {
                return None;
            }
            total = total.checked_add(expected)?;
        }
        if total != packed.len() {
            return None;
        }
        Some(Self {
            width,
            height,
            storage,
            packed,
            level_sizes,
        })
    }
    fn level_count(&self) -> u32 {
        self.level_sizes.len() as u32
    }

    pub(super) fn into_payload(self) -> Vec<u8> {
        self.packed
    }

    pub(super) fn into_top_level_rgba8(mut self) -> Result<(u32, u32, Vec<u8>), String> {
        let top = *self
            .level_sizes
            .first()
            .ok_or_else(|| "decoded image carries no mip level".to_owned())?
            as usize;
        self.packed.truncate(top);
        let level0 = self.packed;
        let pixels = match self.storage {
            MipStorage::Rgba8 => level0,
            MipStorage::R32Float => {
                return Err("float image cannot be represented as RGBA8 without conversion".into());
            }
            MipStorage::Bc1 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc1)?,
            MipStorage::Bc2 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc2)?,
            MipStorage::Bc3 => decode_blocks(&level0, self.width, self.height, PixelFormat::Bc3)?,
            MipStorage::Bc5 => decode_blocks(
                &bc5_to_dxt5nm(&level0),
                self.width,
                self.height,
                PixelFormat::Bc3,
            )?,
        };
        Ok((self.width, self.height, pixels))
    }
}

#[derive(Clone, Copy)]
pub(super) struct MipLayout {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) storage: MipStorage,
    pub(super) levels: u32,
}

pub(super) type CubemapFaces = [Vec<u8>; 6];

pub(super) fn decode_iwi_mips(bytes: &[u8]) -> Result<DecodedMips, String> {
    decode_iwi_mips_with(bytes, false)
}

pub(super) fn decode_iwi_mips_with(bytes: &[u8], keep_bc5: bool) -> Result<DecodedMips, String> {
    if bytes.len() >= IWI_V8_HEADER_LEN
        && bytes[..3] == *b"IWi"
        && bytes[3] == 8
        && let Ok(header) = IwiHeader::parse(bytes)
        && img_format_info(header.format).is_some_and(|info| info.kind == ImgFormatKind::Wavelet)
    {
        return decode_wavelet_iwi(bytes, header, IWI_V8_HEADER_LEN);
    }
    if bytes.len() >= 48 && bytes[..4] == *b"IWi\x0d" && (6..=10).contains(&bytes[4]) {
        let half = |at| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let word = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let header = IwiHeader {
            flags: bytes[5],
            usage: if bytes[5] & 0x0c != 0 { 1 } else { 0 },
            format: bytes[4],
            width: half(6),
            height: half(8),
            depth: half(10),
            file_size_for_picmip: [word(16), word(20), word(24), word(28)],
        };
        return decode_wavelet_iwi(bytes, header, 48);
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
    let storage = match header.format {
        PixelFormat::Bc5 if keep_bc5 => MipStorage::Bc5,
        format => mip_storage(format),
    };

    let single = || -> Result<DecodedMips, String> {
        let (w, h, pixels) = load_mip_level_with(
            payload,
            header.width,
            header.height,
            header.format,
            keep_bc5,
        )?;
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
        match load_mip_level_with(
            &bytes[level_start..level_end],
            w,
            h,
            header.format,
            keep_bc5,
        ) {
            Ok((_, _, pixels)) => levels.push(pixels),
            Err(_) => return single(),
        }
    }
    Ok(DecodedMips::from_levels(
        header.width,
        header.height,
        storage,
        levels,
    ))
}

fn decode_wavelet_iwi(
    bytes: &[u8],
    header: IwiHeader,
    header_len: usize,
) -> Result<DecodedMips, String> {
    let format = wavelet_check_header(&header).map_err(wavelet_error)?;
    let info = img_format_info(format).ok_or_else(|| format!("unsupported IWI format {format}"))?;
    let channels = info.channels;
    let stride = wavelet_pixel_stride(channels);
    let pixel_format = wavelet_d3d_pixel_format(format)?;
    let payload = bytes
        .get(header_len..)
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
    Ok(DecodedMips::from_levels(
        width,
        height,
        MipStorage::Rgba8,
        levels,
    ))
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
        PixelFormat::Bc3 | PixelFormat::Bc5 => MipStorage::Bc3,
        _ => MipStorage::Rgba8,
    }
}

fn load_mip_level(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<(u32, u32, Vec<u8>), String> {
    load_mip_level_with(data, width, height, format, false)
}

fn load_mip_level_with(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    keep_bc5: bool,
) -> Result<(u32, u32, Vec<u8>), String> {
    match format {
        PixelFormat::Bc5 if keep_bc5 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC5 image".to_owned())?;
            Ok((width, height, source.to_vec()))
        }
        PixelFormat::Bc1 | PixelFormat::Bc2 | PixelFormat::Bc3 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC image".to_owned())?;
            Ok((width, height, source.to_vec()))
        }
        PixelFormat::Bc5 => {
            let needed = compressed_mip_bytes(width, height, format);
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated BC5 image".to_owned())?;
            Ok((width, height, bc5_to_dxt5nm(source)))
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
        27 => {
            if bytes.len() < 64 {
                return Err("truncated IWI v27 header".into());
            }
            let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
            let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
            let mip0_end = u32::from_le_bytes(bytes[32..36].try_into().unwrap()) as usize;
            let mip0_start = u32::from_le_bytes(bytes[36..40].try_into().unwrap()) as usize;
            Ok(IwiHeaderInfo {
                width,
                height,
                format: iwi_pixel_format(bytes[4])?,
                header_len: 64,
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
        14 => Ok(PixelFormat::Bc5),
        format => Err(format!("unsupported IWI format {format}")),
    }
}

fn mip_level_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}

pub(super) fn compressed_mip_bytes(width: u32, height: u32, format: PixelFormat) -> usize {
    match format {
        PixelFormat::Bgra8 | PixelFormat::Bgrx8 => width as usize * height as usize * 4,
        PixelFormat::Rgb8 => width as usize * height as usize * 3,
        PixelFormat::La8 => width as usize * height as usize * 2,
        PixelFormat::L8 | PixelFormat::A8 => width as usize * height as usize,
        PixelFormat::Bc1 => width.div_ceil(4) as usize * height.div_ceil(4) as usize * 8,
        PixelFormat::Bc2 | PixelFormat::Bc3 | PixelFormat::Bc5 => {
            width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16
        }
    }
}

pub(super) fn decode_gfx_image(
    bytes: &[u8],
    width: u32,
    height: u32,
    format: u32,
) -> Result<DecodedMips, String> {
    if format == 114 {
        if width == 0 || height == 0 {
            return Err("zero-sized image".into());
        }
        let needed = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| "R32F image size overflow".to_owned())?;
        let pixels = bytes
            .get(..needed)
            .ok_or_else(|| "truncated R32F image".to_owned())?;
        return Ok(DecodedMips::single_compressed(
            width,
            height,
            MipStorage::R32Float,
            pixels.to_vec(),
        ));
    }
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PixelFormat {
    Rgb8,
    Bgra8,
    Bgrx8,
    L8,
    La8,
    A8,
    Bc1,
    Bc2,
    Bc3,
    Bc5,
}

fn bc5_block_to_dxt5nm(src: &[u8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&src[..8]);
    let mut y = [0u8; 16];
    bcdec_rs::bc4(&src[8..16], &mut y, 4, false);
    let (lo, hi) = y
        .iter()
        .fold((u8::MAX, 0u8), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let (g0, g1) = (u16::from(hi >> 2), u16::from(lo >> 2));
    let expand = |g6: u16| f32::from(((g6 << 2) | (g6 >> 4)) as u8);
    let palette = if g0 > g1 {
        let (p0, p1) = (expand(g0), expand(g1));
        [p0, p1, (2.0 * p0 + p1) / 3.0, (p0 + 2.0 * p1) / 3.0]
    } else {
        [expand(g0); 4]
    };
    out[8..10].copy_from_slice(&(g0 << 5).to_le_bytes());
    out[10..12].copy_from_slice(&(g1 << 5).to_le_bytes());
    let mut indices = 0u32;
    for (i, &v) in y.iter().enumerate() {
        let best = (0..4)
            .min_by(|&a, &b| {
                (palette[a] - f32::from(v))
                    .abs()
                    .total_cmp(&(palette[b] - f32::from(v)).abs())
            })
            .unwrap_or(0) as u32;
        indices |= best << (2 * i);
    }
    out[12..16].copy_from_slice(&indices.to_le_bytes());
    out
}

fn bc5_to_dxt5nm(data: &[u8]) -> Vec<u8> {
    data.as_chunks::<16>()
        .0
        .iter()
        .flat_map(|block| bc5_block_to_dxt5nm(block))
        .collect()
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
        PixelFormat::Bgra8 => take_rgba(data, width, height, true, false)?,
        PixelFormat::Bgrx8 => take_rgba(data, width, height, true, true)?,
        PixelFormat::Rgb8 => {
            let needed = width as usize * height as usize * 3;
            let source = data
                .get(..needed)
                .ok_or_else(|| "truncated RGB8".to_owned())?;
            let mut out = Vec::with_capacity(width as usize * height as usize * 4);
            for pixel in source.as_chunks::<3>().0 {
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
            for pixel in source.as_chunks::<2>().0 {
                out.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            out
        }
        PixelFormat::Bc1 | PixelFormat::Bc2 | PixelFormat::Bc3 => {
            decode_blocks(data, width, height, format)?
        }
        PixelFormat::Bc5 => decode_blocks(&bc5_to_dxt5nm(data), width, height, PixelFormat::Bc3)?,
    };
    Ok((width, height, pixels))
}

pub(super) fn take_rgba(
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
    for pixel in out.as_chunks_mut::<4>().0 {
        if bgra {
            pixel.swap(0, 2);
        }
        if opaque {
            pixel[3] = 255;
        }
    }
    Ok(out)
}

pub(super) fn decode_blocks(
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

pub(super) fn decode_iwi_cubemap(bytes: &[u8]) -> Result<(u32, CubemapFaces), String> {
    let header = parse_iwi_header(bytes)?;
    if header.width == 0 || header.width != header.height {
        return Err(format!(
            "non-square cubemap {}x{}",
            header.width, header.height
        ));
    }
    let face_bytes = match header.format {
        PixelFormat::Bgra8 | PixelFormat::Bgrx8 => (header.width * header.height * 4) as usize,
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
