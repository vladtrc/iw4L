use bevy::prelude::*;

use crate::prepare::scene::cull::DpvsFrameStats;
use crate::prepare::scene::world::{WorldCull, WorldDrawItemKind, WorldScene};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawSurfItem {
    pub key: u64,

    pub surf: u16,
    pub run: u16,
    pub kind: WorldDrawItemKind,
    pub setup_key_changed: bool,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DrawSurfList {
    pub items: Vec<DrawSurfItem>,
    pub keys: u32,
    pub rebinds: u32,

    pub draw_items_id: u64,
}

impl DrawSurfList {
    pub fn ingest_from_cull(&mut self, cull: &WorldCull) {
        self.items.clear();
        self.items.reserve(cull.draw_items.len());
        let mut prev_mat: Option<u16> = None;
        let mut rebinds = 0u32;
        for item in &cull.draw_items {
            let mat = ((item.key >> 30) & 0xfff) as u16;
            if prev_mat != Some(mat) {
                rebinds = rebinds.saturating_add(1);
                prev_mat = Some(mat);
            }
            self.items.push(DrawSurfItem {
                key: item.key,
                surf: item.surf,
                run: item.run,
                kind: item.kind,
                setup_key_changed: item.setup_key_changed,
            });
        }
        self.keys = self.items.len() as u32;
        self.rebinds = rebinds;
        self.draw_items_id = cull.draw_items_id;
    }
}

pub(crate) fn ingest_drawsurf_list(
    scene: Res<WorldScene>,
    stats: Option<Res<DpvsFrameStats>>,
    mut list: ResMut<DrawSurfList>,
) {
    let Some(cull) = scene.cull.as_ref() else {
        return;
    };
    list.ingest_from_cull(cull);
    let Some(stats) = stats else {
        return;
    };
    if list.keys != stats.keys || list.rebinds != stats.rebinds {
        diag::warn!(
            World,
            "drawsurf census mismatch: list keys={} rebinds={} vs dpvs keys={} rebinds={} \
             (one side lies; the drawsurf list and dpvs disagree)",
            list.keys,
            list.rebinds,
            stats.keys,
            stats.rebinds
        );
    }
}

pub(crate) const CONTENT_ID_SEED: u64 = 0xcbf2_9ce4_8422_2325;

pub(crate) fn mix_content_id(id: &mut u64, word: u64) {
    *id ^= word;
    *id = id.wrapping_mul(0x0000_0100_0000_01b3);
}

pub(crate) fn mix_content_bytes(id: &mut u64, bytes: &[u8]) {
    mix_content_id(id, bytes.len() as u64);
    for chunk in bytes.chunks(8) {
        let mut word = 0u64;
        for (i, b) in chunk.iter().enumerate() {
            word |= u64::from(*b) << (8 * i);
        }
        mix_content_id(id, word);
    }
}
