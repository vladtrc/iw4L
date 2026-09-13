use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use asset_iw4::size as sz;
use fastfile_iw4::{
    AssetLinkSink, AssetType, Ptr, Result, ScriptStrings, XAnimPartsGeometry, ZoneStream,
};

use crate::asset_graph::ZoneOwner;
use crate::asset_key::AssetNamespace;
use crate::xanim_clip::{AnimClip, ClipNotify, RawDeltaTrans, RawXAnimParts};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct XAnimKey {
    pub namespace: AssetNamespace,
    pub name: String,
}

impl XAnimKey {
    pub fn new(namespace: AssetNamespace, name: &str) -> Self {
        Self {
            namespace,
            name: ascii_lower(name),
        }
    }

    pub fn display(&self) -> String {
        format!("{}:anim/{}", self.namespace.as_str(), self.name)
    }
}

#[derive(Clone, Debug)]
pub struct CapturedXAnim {
    pub namespace: AssetNamespace,
    pub parts: RawXAnimParts,
}

impl CapturedXAnim {
    pub fn key(&self) -> XAnimKey {
        XAnimKey::new(self.namespace, &self.parts.name)
    }
}

#[derive(Debug)]
pub struct XAnimCatalog {
    by_ns: HashMap<AssetNamespace, HashMap<String, CapturedXAnim>>,

    order: Vec<XAnimKey>,

    zones: Vec<ZoneOwner>,
    capture_zone: ZoneOwner,
    capture_ns: AssetNamespace,

    decoded: Mutex<HashMap<AssetNamespace, HashMap<String, Arc<AnimClip>>>>,
    pub capture_gaps: usize,
    strings: ScriptStrings,
}

impl Default for XAnimCatalog {
    fn default() -> Self {
        Self {
            by_ns: HashMap::new(),
            order: Vec::new(),
            zones: Vec::new(),
            capture_zone: ZoneOwner::default(),
            capture_ns: AssetNamespace::Iw4,
            decoded: Mutex::new(HashMap::new()),
            capture_gaps: 0,
            strings: ScriptStrings::default(),
        }
    }
}

impl Clone for XAnimCatalog {
    fn clone(&self) -> Self {
        let decoded = self
            .decoded
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        Self {
            by_ns: self.by_ns.clone(),
            order: self.order.clone(),
            zones: self.zones.clone(),
            capture_zone: self.capture_zone,
            capture_ns: self.capture_ns,
            decoded: Mutex::new(decoded),
            capture_gaps: self.capture_gaps,
            strings: self.strings.clone(),
        }
    }
}

impl XAnimCatalog {
    pub fn set_strings(&mut self, strings: ScriptStrings) {
        self.strings = strings;
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn set_capture_ns(&mut self, ns: AssetNamespace) {
        self.capture_ns = ns;
    }

    pub fn get(&self, ns: AssetNamespace, name: &str) -> Option<&CapturedXAnim> {
        let map = self.by_ns.get(&ns)?;
        if let Some(hit) = map.get(name) {
            return Some(hit);
        }
        if name.as_bytes().iter().any(|&b| b.is_ascii_uppercase()) {
            map.get(ascii_lower(name).as_str())
        } else {
            None
        }
    }

    fn take_row(&mut self, key: &XAnimKey) -> Option<CapturedXAnim> {
        self.by_ns.get_mut(&key.namespace)?.remove(&key.name)
    }

    fn has_key(&self, key: &XAnimKey) -> bool {
        self.by_ns
            .get(&key.namespace)
            .is_some_and(|map| map.contains_key(&key.name))
    }

    pub fn index_by_name(&self, ns: AssetNamespace, name: &str) -> Option<usize> {
        let lowered = ascii_lookup(name);
        self.order
            .iter()
            .position(|n| n.namespace == ns && n.name == lowered.as_ref())
    }

    pub fn zone_of(&self, index: usize) -> ZoneOwner {
        self.zones.get(index).copied().unwrap_or_default()
    }

    pub fn set_capture_zone(&mut self, zone: ZoneOwner) {
        self.capture_zone = zone;
    }

    pub fn name_at(&self, index: usize) -> Option<&str> {
        self.order.get(index).map(|k| k.name.as_str())
    }

    pub fn insert_captured(&mut self, captured: CapturedXAnim) {
        self.insert_in(self.capture_ns, captured);
    }

    pub fn insert_in(&mut self, ns: AssetNamespace, mut captured: CapturedXAnim) {
        captured.namespace = ns;
        let key = captured.key();
        self.retain(key, captured);
    }

    fn retain(&mut self, key: XAnimKey, captured: CapturedXAnim) {
        if let Some(pos) = self.order.iter().position(|n| n == &key) {
            self.zones[pos] = self.capture_zone;
        } else {
            self.order.push(key.clone());
            self.zones.push(self.capture_zone);
        }
        {
            let mut decoded = self
                .decoded
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(map) = decoded.get_mut(&key.namespace) {
                map.remove(&key.name);
            }
        }
        self.by_ns
            .entry(key.namespace)
            .or_default()
            .insert(key.name, captured);
    }

    pub fn clip_at(&self, index: usize) -> Option<Arc<AnimClip>> {
        let key = self.order.get(index)?;
        self.clip(key.namespace, &key.name)
    }

    pub fn clip(&self, ns: AssetNamespace, name: &str) -> Option<Arc<AnimClip>> {
        let lowered = ascii_lookup(name);
        let name = lowered.as_ref();
        {
            let decoded = self
                .decoded
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if let Some(clip) = decoded.get(&ns).and_then(|map| map.get(name)) {
                return Some(Arc::clone(clip));
            }
        }
        let captured = self.by_ns.get(&ns)?.get(name)?;
        let clip = AnimClip::from_parts(&captured.parts).ok()?;
        let arc = Arc::new(clip);
        let mut decoded = self
            .decoded
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        decoded
            .entry(ns)
            .or_default()
            .insert(name.to_owned(), Arc::clone(&arc));
        Some(arc)
    }

    pub fn decode(&self, ns: AssetNamespace, name: &str) -> Option<AnimClip> {
        let captured = self.get(ns, name)?;
        AnimClip::from_parts(&captured.parts).ok()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(|k| k.name.as_str())
    }

    pub fn namespace_count(&self, ns: AssetNamespace) -> usize {
        self.order.iter().filter(|k| k.namespace == ns).count()
    }

    pub fn collide_name_count(&self) -> usize {
        let mut seen: std::collections::HashMap<&str, u8> = std::collections::HashMap::new();
        for k in &self.order {
            *seen.entry(k.name.as_str()).or_insert(0) |= match k.namespace {
                AssetNamespace::Iw4 => 1,
                AssetNamespace::T5 => 2,
                AssetNamespace::Iw5 => 4,
            };
        }
        seen.values().filter(|bits| bits.count_ones() >= 2).count()
    }

    pub fn peer(&self, ns: AssetNamespace, name: &str) -> Option<&CapturedXAnim> {
        const PREFER: [AssetNamespace; 3] =
            [AssetNamespace::T5, AssetNamespace::Iw5, AssetNamespace::Iw4];
        for other in PREFER {
            if other == ns {
                continue;
            }
            if let Some(captured) = self.get(other, name) {
                return Some(captured);
            }
        }
        None
    }

    pub fn absorb(&mut self, mut local: XAnimCatalog) -> usize {
        self.capture_gaps = self.capture_gaps.saturating_add(local.capture_gaps);
        let saved_zone = self.capture_zone;
        let saved_ns = self.capture_ns;
        let mut added = 0;
        let order = std::mem::take(&mut local.order);
        for (i, key) in order.into_iter().enumerate() {
            let Some(captured) = local.take_row(&key) else {
                continue;
            };
            let vacant = !self.has_key(&key);
            self.capture_zone = local.zones.get(i).copied().unwrap_or(local.capture_zone);
            self.capture_ns = key.namespace;
            self.retain(key, captured);
            if vacant {
                added += 1;
            }
        }
        self.capture_zone = saved_zone;
        self.capture_ns = saved_ns;
        added
    }

    pub fn absorb_local(&mut self, local: XAnimCatalog) {
        let _ = self.absorb(local);
    }

    pub fn capture_xanim_iw5(
        &mut self,
        s: &fastfile_iw5::ZoneStream<'_>,
        strings: &fastfile_iw5::ScriptStrings,
        geometry: fastfile_iw5::XAnimPartsGeometry,
    ) {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return;
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return;
        };
        if name.is_empty() {
            self.capture_gaps += 1;
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
            self.capture_gaps += 1;
            return;
        }

        let mut notifies = Vec::with_capacity(geometry.notify_count);
        if let Some(arr) = geometry.notify {
            for i in 0..geometry.notify_count {
                let b = arr.at(i * fastfile_iw5::size::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                let time = s.f32_at(b, 4).unwrap_or(0.0);
                notifies.push(ClipNotify {
                    name: strings.get(s, sid).unwrap_or("").to_owned(),
                    time,
                });
            }
        }

        let data_byte = copy_u8_iw5(s, geometry.data_byte, geometry.data_byte_count);
        let data_short = copy_u16_iw5(s, geometry.data_short, geometry.data_short_count);
        let data_int = copy_u32_iw5(s, geometry.data_int, geometry.data_int_count);
        let random_data_short = copy_u16_iw5(
            s,
            geometry.random_data_short,
            geometry.random_data_short_count,
        );
        let random_data_byte = copy_u8_iw5(
            s,
            geometry.random_data_byte,
            geometry.random_data_byte_count,
        );
        let random_data_int =
            copy_u32_iw5(s, geometry.random_data_int, geometry.random_data_int_count);
        let indices = if geometry.indices_are_bytes {
            copy_u8_iw5(s, geometry.indices, geometry.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16_iw5(s, geometry.indices, geometry.index_count)
        };

        self.insert_in(
            AssetNamespace::Iw5,
            CapturedXAnim {
                namespace: AssetNamespace::Iw5,
                parts: RawXAnimParts {
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
                },
            },
        );
    }

    pub fn capture_xanim_t5(
        &mut self,
        s: &fastfile_t5::ZoneStream<'_>,
        strings: &fastfile_t5::ScriptStrings,
        geometry: fastfile_t5::XAnimPartsGeometry,
    ) {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return;
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return;
        };
        if name.is_empty() {
            self.capture_gaps += 1;
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
            self.capture_gaps += 1;
            return;
        }

        let mut notifies = Vec::with_capacity(geometry.notify_count);
        if let Some(arr) = geometry.notify {
            for i in 0..geometry.notify_count {
                let b = arr.at(i * fastfile_t5::size::XANIM_NOTIFY_INFO);
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
        let random_data_int =
            copy_u32_t5(s, geometry.random_data_int, geometry.random_data_int_count);
        let indices = if geometry.indices_are_bytes {
            copy_u8_t5(s, geometry.indices, geometry.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16_t5(s, geometry.indices, geometry.index_count)
        };

        self.insert_in(
            AssetNamespace::T5,
            CapturedXAnim {
                namespace: AssetNamespace::T5,
                parts: RawXAnimParts {
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
                },
            },
        );
    }
}

impl AssetLinkSink for XAnimCatalog {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        _ty: AssetType,
        _slot: Ptr,
        _insert_slot: Option<Ptr>,
    ) -> Result<()> {
        Ok(())
    }

    fn alias(&mut self, _ty: AssetType, _slot: Ptr, _target: Ptr) -> Result<()> {
        Ok(())
    }

    fn capture_xanim(&mut self, s: &ZoneStream<'_>, geometry: XAnimPartsGeometry) -> Result<()> {
        let Some(name_ptr) = geometry.name else {
            self.capture_gaps += 1;
            return Ok(());
        };
        let Ok(name) = s.cstr(name_ptr) else {
            self.capture_gaps += 1;
            return Ok(());
        };
        if name.is_empty() {
            self.capture_gaps += 1;
            return Ok(());
        }

        let track_count = geometry.bone_count[9] as usize;
        let mut names = Vec::with_capacity(track_count);
        if let Some(arr) = geometry.names {
            for i in 0..track_count {
                let sid = s.u16_at(arr, i * 2).unwrap_or(0);
                names.push(self.strings.get(s, sid).unwrap_or("").to_owned());
            }
        } else if track_count > 0 {
            self.capture_gaps += 1;
            return Ok(());
        }

        let mut notifies = Vec::with_capacity(geometry.notify_count);
        if let Some(arr) = geometry.notify {
            for i in 0..geometry.notify_count {
                let b = arr.at(i * sz::XANIM_NOTIFY_INFO);
                let sid = s.u16_at(b, 0).unwrap_or(0);
                let time = s.f32_at(b, 4).unwrap_or(0.0);
                notifies.push(ClipNotify {
                    name: self.strings.get(s, sid).unwrap_or("").to_owned(),
                    time,
                });
            }
        }

        let data_byte = copy_u8(s, geometry.data_byte, geometry.data_byte_count);
        let data_short = copy_u16(s, geometry.data_short, geometry.data_short_count);
        let data_int = copy_u32(s, geometry.data_int, geometry.data_int_count);
        let random_data_short = copy_u16(
            s,
            geometry.random_data_short,
            geometry.random_data_short_count,
        );
        let random_data_byte = copy_u8(
            s,
            geometry.random_data_byte,
            geometry.random_data_byte_count,
        );
        let random_data_int = copy_u32(s, geometry.random_data_int, geometry.random_data_int_count);
        let indices = if geometry.indices_are_bytes {
            copy_u8(s, geometry.indices, geometry.index_count)
                .into_iter()
                .map(u16::from)
                .collect()
        } else {
            copy_u16(s, geometry.indices, geometry.index_count)
        };
        let delta_trans = copy_delta_trans(s, geometry.delta_trans);

        self.insert_captured(CapturedXAnim {
            namespace: AssetNamespace::Iw4,
            parts: RawXAnimParts {
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
                delta_trans,
            },
        });
        let _ = geometry.frequency;
        Ok(())
    }
}

fn ascii_lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn ascii_lookup(name: &str) -> Cow<'_, str> {
    if name.as_bytes().iter().all(|&b| !b.is_ascii_uppercase()) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.to_ascii_lowercase())
    }
}

fn copy_u8(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_default()
}

fn copy_u16(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32(s: &ZoneStream<'_>, ptr: Option<Ptr>, count: usize) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn copy_f32_3(s: &ZoneStream<'_>, ptr: Ptr, off: usize) -> Option<[f32; 3]> {
    let bytes = s.slice_at(ptr, off, 12).ok()?;
    Some([
        f32::from_le_bytes(bytes[0..4].try_into().ok()?),
        f32::from_le_bytes(bytes[4..8].try_into().ok()?),
        f32::from_le_bytes(bytes[8..12].try_into().ok()?),
    ])
}

fn copy_delta_trans(
    s: &ZoneStream<'_>,
    geo: fastfile_iw4::XAnimDeltaTransGeometry,
) -> Option<RawDeltaTrans> {
    if let Some(constant) = geo.constant {
        let mins = copy_f32_3(s, constant, 0)?;
        return Some(RawDeltaTrans {
            size: 0,
            small: geo.small != 0,
            mins,
            step: [0.0; 3],
            indices: Vec::new(),
            packed: Vec::new(),
        });
    }
    let mins_step = geo.mins_step?;
    let mins = copy_f32_3(s, mins_step, 0)?;
    let step = copy_f32_3(s, mins_step, 12)?;
    let n = geo.size as usize + 1;
    let indices = if geo.indices_are_bytes {
        copy_u8(s, geo.indices, n)
            .into_iter()
            .map(u16::from)
            .collect()
    } else {
        copy_u16(s, geo.indices, n)
    };
    let packed_n = if geo.small != 0 { 3 * n } else { 6 * n };
    let packed = copy_u8(s, geo.frames, packed_n);
    Some(RawDeltaTrans {
        size: geo.size,
        small: geo.small != 0,
        mins,
        step,
        indices,
        packed,
    })
}

fn copy_u8_iw5(
    s: &fastfile_iw5::ZoneStream<'_>,
    ptr: Option<fastfile_iw5::Ptr>,
    count: usize,
) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_default()
}

fn copy_u16_iw5(
    s: &fastfile_iw5::ZoneStream<'_>,
    ptr: Option<fastfile_iw5::Ptr>,
    count: usize,
) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32_iw5(
    s: &fastfile_iw5::ZoneStream<'_>,
    ptr: Option<fastfile_iw5::Ptr>,
    count: usize,
) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn copy_u8_t5(
    s: &fastfile_t5::ZoneStream<'_>,
    ptr: Option<fastfile_t5::Ptr>,
    count: usize,
) -> Vec<u8> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    s.slice_at(p, 0, count)
        .map(|b| b.to_vec())
        .unwrap_or_default()
}

fn copy_u16_t5(
    s: &fastfile_t5::ZoneStream<'_>,
    ptr: Option<fastfile_t5::Ptr>,
    count: usize,
) -> Vec<u16> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 2) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect()
}

fn copy_u32_t5(
    s: &fastfile_t5::ZoneStream<'_>,
    ptr: Option<fastfile_t5::Ptr>,
    count: usize,
) -> Vec<u32> {
    let Some(p) = ptr else {
        return Vec::new();
    };
    let Ok(bytes) = s.slice_at(p, 0, count * 4) else {
        return Vec::new();
    };
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}
