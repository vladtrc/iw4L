#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImgFormatInfo {
    pub format: u8,
    pub channels: u8,

    pub d3d_format: u32,
    pub kind: ImgFormatKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImgFormatKind {
    PlainBitmap,

    Wavelet,

    Dxt,
}

pub fn img_format_info(format: u8) -> Option<ImgFormatInfo> {
    match format {
        1..=5 => Some(ImgFormatInfo {
            format,
            channels: match format {
                1 => 4,
                2 => 3,
                3 => 2,
                4 | 5 => 1,
                _ => unreachable!(),
            },
            d3d_format: 0,
            kind: ImgFormatKind::PlainBitmap,
        }),
        6 => Some(ImgFormatInfo {
            format,
            channels: 4,
            d3d_format: 0x15,
            kind: ImgFormatKind::Wavelet,
        }),
        7 => Some(ImgFormatInfo {
            format,
            channels: 3,
            d3d_format: 0x16,
            kind: ImgFormatKind::Wavelet,
        }),
        8 => Some(ImgFormatInfo {
            format,
            channels: 2,
            d3d_format: 0x33,
            kind: ImgFormatKind::Wavelet,
        }),
        9 => Some(ImgFormatInfo {
            format,
            channels: 1,
            d3d_format: 0x32,
            kind: ImgFormatKind::Wavelet,
        }),
        10 => Some(ImgFormatInfo {
            format,
            channels: 1,
            d3d_format: 0x1c,
            kind: ImgFormatKind::Wavelet,
        }),
        0xb..=0xd => Some(ImgFormatInfo {
            format,
            channels: 0,
            d3d_format: 0,
            kind: ImgFormatKind::Dxt,
        }),
        _ => None,
    }
}

#[inline]
pub fn wavelet_pixel_stride(channels: u8) -> usize {
    if channels == 3 { 4 } else { channels as usize }
}
