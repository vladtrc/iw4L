use asset_anim::{CapturedXAnim, ClipNotify, RawXAnimParts, XAnimBuild};
use asset_core::FamilyId;
use std::sync::Arc;
pub fn capture(
    catalog: &mut XAnimBuild,
    ns: FamilyId,
    prefix: &str,
    load: &ext_t6_fastfile::ZoneLoad,
    asset: &ext_t6_fastfile::LoadedAsset,
) -> bool {
    const SIZE: usize = 104;
    let h = &asset.header;
    if asset.ty != ext_t6_fastfile::AssetType::XAnimParts || h.len() < SIZE {
        return false;
    }
    let u16_at = |o: usize| u16::from_le_bytes([h[o], h[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap());
    let ptr = |o: usize| {
        let raw = u32_at(o);
        (raw != 0 && raw < 0xFFFF_FFFE).then(|| ext_t6_fastfile::Ptr {
            block: ((raw - 1) >> 29) as u8,
            offset: (raw - 1) & 0x1FFF_FFFF,
        })
    };
    let bytes = |o: usize, len: usize| -> Vec<u8> {
        ptr(o)
            .and_then(|p| load.blocks.bytes(p, len).ok())
            .map_or_else(Vec::new, <[u8]>::to_vec)
    };
    let words = |o: usize, count: usize| -> Vec<u16> {
        bytes(o, 2 * count)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect()
    };
    let dwords = |o: usize, count: usize| -> Vec<u32> {
        bytes(o, 4 * count)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    };
    let Some(name) = ptr(0)
        .and_then(|p| load.blocks.cstr(p).ok())
        .and_then(|b| core::str::from_utf8(b).ok())
        .filter(|name| !name.is_empty())
    else {
        catalog.capture_gaps += 1;
        return false;
    };
    let numframes = u16_at(14);
    let mut bone_count = [0u8; 10];
    bone_count.copy_from_slice(&h[24..34]);
    let names = words(64, usize::from(bone_count[9]))
        .into_iter()
        .map(|id| load.script_string(id).unwrap_or("").to_owned())
        .collect();
    let notifies = bytes(96, 8 * usize::from(h[34]))
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| ClipNotify {
            name: load
                .script_string(u16::from_le_bytes([c[0], c[1]]))
                .unwrap_or("")
                .to_owned(),
            time: f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
        })
        .collect();
    let index_count = u32_at(44) as usize;
    let indices = if numframes < 256 {
        bytes(92, index_count).into_iter().map(u16::from).collect()
    } else {
        words(92, index_count)
    };
    let parts = RawXAnimParts {
        name: format!("{prefix}{}", name.to_ascii_lowercase()),
        data_byte: bytes(68, usize::from(u16_at(4))),
        data_short: words(72, usize::from(u16_at(6))),
        data_int: dwords(76, usize::from(u16_at(8))),
        random_data_short: words(80, u32_at(40) as usize),
        random_data_byte: bytes(84, usize::from(u16_at(10))),
        random_data_int: dwords(88, usize::from(u16_at(12))),
        numframes,
        flags: u8::from(h[16] != 0) | (u8::from(h[17] != 0) << 1),
        bone_count,
        framerate: f32::from_le_bytes(h[48..52].try_into().unwrap()),
        names,
        notifies,
        indices,
        delta_trans: None,
        delta_quat2: None,
        delta_quat: None,
    };
    catalog.insert_in(
        ns,
        CapturedXAnim {
            namespace: ns,
            parts: Arc::new(parts),
        },
    );
    true
}
