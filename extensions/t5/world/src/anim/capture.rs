use asset_anim::{CapturedXAnim, ClipNotify, RawXAnimParts, XAnimBuild};
use asset_core::FamilyId;
use std::sync::Arc;
pub fn capture(
    catalog: &mut XAnimBuild,
    s: &ext_t5_fastfile::ZoneStream<'_>,
    strings: &ext_t5_fastfile::ScriptStrings,
    geometry: ext_t5_fastfile::XAnimPartsGeometry,
) {
    let Some(name_ptr) = geometry.name else {
        catalog.capture_gaps += 1;
        return;
    };
    let Ok(name) = s.cstr(name_ptr) else {
        catalog.capture_gaps += 1;
        return;
    };
    if name.is_empty() {
        catalog.capture_gaps += 1;
        return;
    }

    let track_count = geometry.bone_count[9] as usize;
    let mut names = Vec::with_capacity(track_count);
    if let Some(arr) = geometry.names {
        for i in 0..track_count {
            let sid = s.u16_at(arr, i * 2).unwrap_or(0);
            names.push(strings.get(s, sid).unwrap_or("").to_owned());
        }
    } else if track_count > 0 {
        catalog.capture_gaps += 1;
        return;
    }

    let mut notifies = Vec::with_capacity(geometry.notify_count);
    if let Some(arr) = geometry.notify {
        for i in 0..geometry.notify_count {
            let b = arr.at(i * ext_t5_fastfile::size::XANIM_NOTIFY_INFO);
            let sid = s.u16_at(b, 0).unwrap_or(0);
            let time = s.f32_at(b, 4).unwrap_or(0.0);
            notifies.push(ClipNotify {
                name: strings.get(s, sid).unwrap_or("").to_owned(),
                time,
            });
        }
    }

    let data_byte = copy_u8_t5(s, geometry.data_byte, geometry.data_byte_count);
    let data_short = copy_u16_t5(s, geometry.data_short, geometry.data_short_count);
    let data_int = copy_u32_t5(s, geometry.data_int, geometry.data_int_count);
    let random_data_short = copy_u16_t5(
        s,
        geometry.random_data_short,
        geometry.random_data_short_count,
    );
    let random_data_byte = copy_u8_t5(
        s,
        geometry.random_data_byte,
        geometry.random_data_byte_count,
    );
    let random_data_int = copy_u32_t5(s, geometry.random_data_int, geometry.random_data_int_count);
    let indices = if geometry.indices_are_bytes {
        copy_u8_t5(s, geometry.indices, geometry.index_count)
            .into_iter()
            .map(u16::from)
            .collect()
    } else {
        copy_u16_t5(s, geometry.indices, geometry.index_count)
    };

    catalog.insert_in(
        FamilyId::new("t5"),
        CapturedXAnim {
            namespace: FamilyId::new("t5"),
            parts: Arc::new(RawXAnimParts {
                name: name.to_owned(),
                data_byte,
                data_short,
                data_int,
                random_data_byte,
                random_data_short,
                random_data_int,
                numframes: geometry.numframes,
                flags: geometry.flags,
                bone_count: geometry.bone_count,
                framerate: geometry.framerate,
                names,
                notifies,
                indices,
                delta_trans: None,
                delta_quat2: None,
                delta_quat: None,
            }),
        },
    );
}
fn copy_u8_t5(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    ptr: Option<ext_t5_fastfile::Ptr>,
    count: usize,
) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or(Vec::new())
}

fn copy_u16_t5(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    ptr: Option<ext_t5_fastfile::Ptr>,
    count: usize,
) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32_t5(
    s: &ext_t5_fastfile::ZoneStream<'_>,
    ptr: Option<ext_t5_fastfile::Ptr>,
    count: usize,
) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
