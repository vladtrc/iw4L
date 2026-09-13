use playerstate_iw4::PlayerState;

use crate::ProjectileState;
use crate::match_state::SnapshotMeta;
use crate::world::{ClientId, Tick};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaSectorSnapshot {
    pub index: u16,
    pub contents_entities: u32,
    pub linkcontents_entities: u32,
    pub entities: u16,
    pub dist: f32,
    pub axis: u16,
    pub parent: u16,
    pub child: [u16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaEntityLinkSnapshot {
    pub entity_num: u16,
    pub world_sector: u16,
    pub next_entity: u16,
    pub linkcontents: u32,
    pub linkmin: [f32; 2],
    pub linkmax: [f32; 2],
    pub contents: u32,
    pub bounds_mid: [f32; 3],
    pub bounds_half: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct AreaEntityWorldSnapshot {
    pub world_mid: [f32; 3],
    pub world_half: [f32; 3],

    pub free_prefix: Vec<u16>,

    pub contiguous_free_head: u16,
    pub sectors: Vec<AreaSectorSnapshot>,
    pub entities: Vec<AreaEntityLinkSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaEntityWorldSnapshotError {
    InvalidWorldBounds,
    InvalidFreeChain,
    InvalidSectorRows,
    InvalidEntityRows,
    InvalidRetailRows,
}

impl AreaEntityWorldSnapshot {
    pub(crate) fn capture(world: &clipmap_iw4::AreaEntityWorld) -> Self {
        let mut free_order = Vec::new();
        let mut free = vec![false; clipmap_iw4::AREA_SECTOR_COUNT];
        let mut cursor = world.free_head();
        while cursor != 0 {
            let index = usize::from(cursor);
            assert!(index < free.len() && !free[index], "retail area free chain");
            free[index] = true;
            free_order.push(cursor);
            cursor = world.sectors()[index].parent_or_next_free;
        }

        let tail_at = free_order.iter().enumerate().find_map(|(offset, &first)| {
            let first = usize::from(first);
            let tail = &free_order[offset..];
            (tail.len() == clipmap_iw4::AREA_SECTOR_COUNT.saturating_sub(first)
                && tail
                    .iter()
                    .enumerate()
                    .all(|(delta, &value)| usize::from(value) == first + delta))
            .then_some(offset)
        });
        let (free_prefix, contiguous_free_head) = tail_at.map_or_else(
            || (free_order.clone(), 0),
            |offset| (free_order[..offset].to_vec(), free_order[offset]),
        );

        let sectors = world
            .sectors()
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(index, _)| !free[*index])
            .map(|(index, row)| AreaSectorSnapshot {
                index: index as u16,
                contents_entities: row.contents_entities,
                linkcontents_entities: row.linkcontents_entities,
                entities: row.entities,
                dist: row.dist,
                axis: row.axis,
                parent: row.parent_or_next_free,
                child: row.child,
            })
            .collect();
        let entities = world
            .links()
            .iter()
            .enumerate()
            .filter(|(_, link)| link.world_sector != 0)
            .map(|(entity_num, link)| {
                let bounds = world.entity_bounds_rows()[entity_num]
                    .expect("linked retail area entity has Bounds");
                AreaEntityLinkSnapshot {
                    entity_num: entity_num as u16,
                    world_sector: link.world_sector,
                    next_entity: link.next_entity_in_world_sector,
                    linkcontents: link.linkcontents,
                    linkmin: link.linkmin,
                    linkmax: link.linkmax,
                    contents: world.entity_contents_rows()[entity_num],
                    bounds_mid: bounds.mid(),
                    bounds_half: bounds.half(),
                }
            })
            .collect();
        let world_bounds = world.world_bounds();
        Self {
            world_mid: world_bounds.mid(),
            world_half: world_bounds.half(),
            free_prefix,
            contiguous_free_head,
            sectors,
            entities,
        }
    }

    pub fn validate(&self) -> Result<(), AreaEntityWorldSnapshotError> {
        self.restore().map(|_| ())
    }

    pub(crate) fn restore(
        &self,
    ) -> Result<clipmap_iw4::AreaEntityWorld, AreaEntityWorldSnapshotError> {
        let world = clipmap_iw4::AreaBounds::from_mid_half(self.world_mid, self.world_half)
            .map_err(|_| AreaEntityWorldSnapshotError::InvalidWorldBounds)?;
        let count = clipmap_iw4::AREA_SECTOR_COUNT;
        let mut free_order = self.free_prefix.clone();
        if self.contiguous_free_head != 0 {
            let first = usize::from(self.contiguous_free_head);
            if !(2..count).contains(&first) {
                return Err(AreaEntityWorldSnapshotError::InvalidFreeChain);
            }
            free_order.extend((first..count).map(|index| index as u16));
        }
        let mut free = vec![false; count];
        for &index in &free_order {
            let index = usize::from(index);
            if !(2..count).contains(&index) || free[index] {
                return Err(AreaEntityWorldSnapshotError::InvalidFreeChain);
            }
            free[index] = true;
        }

        let mut sectors = vec![clipmap_iw4::AreaSector::default(); count];
        for (offset, &index) in free_order.iter().enumerate() {
            sectors[usize::from(index)].parent_or_next_free =
                free_order.get(offset + 1).copied().unwrap_or(0);
        }
        let mut live = vec![false; count];
        for row in &self.sectors {
            let index = usize::from(row.index);
            if index == 0 || index >= count || free[index] || live[index] {
                return Err(AreaEntityWorldSnapshotError::InvalidSectorRows);
            }
            live[index] = true;
            sectors[index] = clipmap_iw4::AreaSector {
                contents_entities: row.contents_entities,
                linkcontents_entities: row.linkcontents_entities,
                entities: row.entities,
                dist: row.dist,
                axis: row.axis,
                parent_or_next_free: row.parent,
                child: row.child,
            };
        }
        if (1..count).any(|index| free[index] == live[index]) {
            return Err(AreaEntityWorldSnapshotError::InvalidSectorRows);
        }

        let mut links = vec![clipmap_iw4::AreaEntityLink::default(); count];
        let mut contents = vec![0; count];
        let mut bounds = vec![None; count];
        let mut occupied = vec![false; count];
        for row in &self.entities {
            let index = usize::from(row.entity_num);
            if index >= count || occupied[index] {
                return Err(AreaEntityWorldSnapshotError::InvalidEntityRows);
            }
            occupied[index] = true;
            links[index] = clipmap_iw4::AreaEntityLink {
                world_sector: row.world_sector,
                next_entity_in_world_sector: row.next_entity,
                linkcontents: row.linkcontents,
                linkmin: row.linkmin,
                linkmax: row.linkmax,
            };
            contents[index] = row.contents;
            bounds[index] = Some(
                clipmap_iw4::AreaBounds::from_mid_half(row.bounds_mid, row.bounds_half)
                    .map_err(|_| AreaEntityWorldSnapshotError::InvalidEntityRows)?,
            );
        }

        clipmap_iw4::AreaEntityWorld::from_retail_rows(
            world,
            free_order.first().copied().unwrap_or(0),
            sectors,
            links,
            contents,
            bounds,
        )
        .map_err(|_| AreaEntityWorldSnapshotError::InvalidRetailRows)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub tick: Tick,
    pub players: Vec<(ClientId, PlayerState)>,
    pub projectiles: Vec<ProjectileState>,

    pub meta: SnapshotMeta,
}

impl Snapshot {
    pub fn unpublished(tick: Tick) -> Self {
        Self {
            tick,
            players: Vec::new(),
            projectiles: Vec::new(),
            meta: SnapshotMeta::default(),
        }
    }
}
