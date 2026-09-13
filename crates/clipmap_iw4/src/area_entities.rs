use alloc::{vec, vec::Vec};

const AREA_NODE_MIN_EXTENT: f32 = 512.0;
pub const AREA_SECTOR_COUNT: usize = 1024;
const AREA_ROOT_SECTOR: u16 = 1;
const AREA_FIRST_FREE_SECTOR: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaEntityWorldError {
    InvalidWorldBounds,
    InvalidQueryBounds,
    InvalidLinkBounds,
    EntityIndexOutOfRange,
    InvalidState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaBounds {
    mid: [f32; 3],
    half: [f32; 3],
}

impl AreaBounds {
    pub fn from_mins_maxs(mins: [f32; 3], maxs: [f32; 3]) -> Result<Self, AreaEntityWorldError> {
        if !(0..3).all(|axis| {
            mins[axis].is_finite() && maxs[axis].is_finite() && mins[axis] <= maxs[axis]
        }) {
            return Err(AreaEntityWorldError::InvalidWorldBounds);
        }
        Ok(Self {
            mid: [
                (mins[0] + maxs[0]) * 0.5,
                (mins[1] + maxs[1]) * 0.5,
                (mins[2] + maxs[2]) * 0.5,
            ],
            half: [
                (maxs[0] - mins[0]) * 0.5,
                (maxs[1] - mins[1]) * 0.5,
                (maxs[2] - mins[2]) * 0.5,
            ],
        })
    }

    pub fn from_mid_half(mid: [f32; 3], half: [f32; 3]) -> Result<Self, AreaEntityWorldError> {
        if !(0..3).all(|axis| mid[axis].is_finite() && half[axis].is_finite() && half[axis] >= 0.0)
        {
            return Err(AreaEntityWorldError::InvalidQueryBounds);
        }
        Ok(Self { mid, half })
    }

    pub fn mins(self) -> [f32; 3] {
        [
            self.mid[0] - self.half[0],
            self.mid[1] - self.half[1],
            self.mid[2] - self.half[2],
        ]
    }

    pub fn mid(self) -> [f32; 3] {
        self.mid
    }

    pub fn half(self) -> [f32; 3] {
        self.half
    }

    pub fn maxs(self) -> [f32; 3] {
        [
            self.mid[0] + self.half[0],
            self.mid[1] + self.half[1],
            self.mid[2] + self.half[2],
        ]
    }

    fn overlaps(self, other: Self) -> bool {
        (0..3).all(|axis| {
            (self.mid[axis] - other.mid[axis]).abs() <= self.half[axis] + other.half[axis]
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AreaSector {
    pub contents_entities: u32,
    pub linkcontents_entities: u32,

    pub entities: u16,

    pub dist: f32,
    pub axis: u16,

    pub parent_or_next_free: u16,

    pub child: [u16; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AreaEntityLink {
    pub world_sector: u16,
    pub next_entity_in_world_sector: u16,

    pub linkcontents: u32,

    pub linkmin: [f32; 2],
    pub linkmax: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct AreaEntityWorld {
    world: AreaBounds,
    free_head: u16,
    sectors: Vec<AreaSector>,
    links: Vec<AreaEntityLink>,
    entity_contents: Vec<u32>,
    entity_bounds: Vec<Option<AreaBounds>>,
}

impl AreaEntityWorld {
    pub fn new(world: AreaBounds) -> Self {
        let mut sectors = vec![AreaSector::default(); AREA_SECTOR_COUNT];
        for index in AREA_FIRST_FREE_SECTOR..(AREA_SECTOR_COUNT as u16 - 1) {
            sectors[usize::from(index)].parent_or_next_free = index + 1;
        }

        let mins = world.mins();
        let maxs = world.maxs();
        let extents = [maxs[0] - mins[0], maxs[1] - mins[1]];
        let axis = u16::from(extents[0] <= extents[1]);
        sectors[usize::from(AREA_ROOT_SECTOR)].axis = axis;
        sectors[usize::from(AREA_ROOT_SECTOR)].dist =
            (mins[usize::from(axis)] + maxs[usize::from(axis)]) * 0.5;

        Self {
            world,
            free_head: AREA_FIRST_FREE_SECTOR,
            sectors,
            links: vec![AreaEntityLink::default(); AREA_SECTOR_COUNT],
            entity_contents: vec![0; AREA_SECTOR_COUNT],
            entity_bounds: vec![None; AREA_SECTOR_COUNT],
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new(self.world);
    }

    pub fn world_bounds(&self) -> AreaBounds {
        self.world
    }

    pub fn free_head(&self) -> u16 {
        self.free_head
    }

    pub fn sectors(&self) -> &[AreaSector] {
        &self.sectors
    }

    pub fn links(&self) -> &[AreaEntityLink] {
        &self.links
    }

    pub fn entity_contents_rows(&self) -> &[u32] {
        &self.entity_contents
    }

    pub fn entity_bounds_rows(&self) -> &[Option<AreaBounds>] {
        &self.entity_bounds
    }

    pub fn from_retail_rows(
        world: AreaBounds,
        free_head: u16,
        sectors: Vec<AreaSector>,
        links: Vec<AreaEntityLink>,
        entity_contents: Vec<u32>,
        entity_bounds: Vec<Option<AreaBounds>>,
    ) -> Result<Self, AreaEntityWorldError> {
        let state = Self {
            world,
            free_head,
            sectors,
            links,
            entity_contents,
            entity_bounds,
        };
        state.validate_retail_rows()?;
        Ok(state)
    }

    fn validate_retail_rows(&self) -> Result<(), AreaEntityWorldError> {
        if self.sectors.len() != AREA_SECTOR_COUNT
            || self.links.len() != AREA_SECTOR_COUNT
            || self.entity_contents.len() != AREA_SECTOR_COUNT
            || self.entity_bounds.len() != AREA_SECTOR_COUNT
            || self.sectors[0] != AreaSector::default()
        {
            return Err(AreaEntityWorldError::InvalidState);
        }

        let mut free = vec![false; AREA_SECTOR_COUNT];
        let mut cursor = self.free_head;
        while cursor != 0 {
            let index = usize::from(cursor);
            if !(usize::from(AREA_FIRST_FREE_SECTOR)..AREA_SECTOR_COUNT).contains(&index)
                || free[index]
            {
                return Err(AreaEntityWorldError::InvalidState);
            }
            free[index] = true;
            let row = self.sectors[index];
            if row.contents_entities != 0
                || row.linkcontents_entities != 0
                || row.entities != 0
                || row.dist != 0.0
                || row.axis != 0
                || row.child != [0, 0]
            {
                return Err(AreaEntityWorldError::InvalidState);
            }
            cursor = row.parent_or_next_free;
        }

        let mut live = vec![false; AREA_SECTOR_COUNT];
        let mut stack = vec![AREA_ROOT_SECTOR];
        while let Some(node_index) = stack.pop() {
            let index = usize::from(node_index);
            if index == 0 || index >= AREA_SECTOR_COUNT || free[index] || live[index] {
                return Err(AreaEntityWorldError::InvalidState);
            }
            live[index] = true;
            let node = self.sectors[index];
            if node.axis > 1 || !node.dist.is_finite() {
                return Err(AreaEntityWorldError::InvalidState);
            }
            if node_index == AREA_ROOT_SECTOR {
                if node.parent_or_next_free != 0 {
                    return Err(AreaEntityWorldError::InvalidState);
                }
            } else if node.parent_or_next_free == 0 {
                return Err(AreaEntityWorldError::InvalidState);
            }
            for child in node.child {
                if child == 0 {
                    continue;
                }
                let child_index = usize::from(child);
                if child_index >= AREA_SECTOR_COUNT
                    || self.sectors[child_index].parent_or_next_free != node_index
                {
                    return Err(AreaEntityWorldError::InvalidState);
                }
                stack.push(child);
            }
        }
        if (1..AREA_SECTOR_COUNT).any(|index| free[index] == live[index]) {
            return Err(AreaEntityWorldError::InvalidState);
        }

        let mut linked = vec![false; AREA_SECTOR_COUNT];
        for (node_index, is_live) in live.iter().copied().enumerate().skip(1) {
            if !is_live {
                continue;
            }
            let mut encoded = self.sectors[node_index].entities;
            while encoded != 0 {
                let entity_index = usize::from(encoded - 1);
                if entity_index >= AREA_SECTOR_COUNT
                    || linked[entity_index]
                    || usize::from(self.links[entity_index].world_sector) != node_index
                {
                    return Err(AreaEntityWorldError::InvalidState);
                }
                linked[entity_index] = true;
                encoded = self.links[entity_index].next_entity_in_world_sector;
            }
        }
        for (index, is_linked) in linked.into_iter().enumerate() {
            let link = self.links[index];
            if is_linked {
                if link.linkcontents == 0
                    || self.entity_bounds[index].is_none()
                    || !(0..2).all(|axis| {
                        link.linkmin[axis].is_finite()
                            && link.linkmax[axis].is_finite()
                            && link.linkmin[axis] <= link.linkmax[axis]
                    })
                {
                    return Err(AreaEntityWorldError::InvalidState);
                }
            } else if link != AreaEntityLink::default()
                || self.entity_contents[index] != 0
                || self.entity_bounds[index].is_some()
            {
                return Err(AreaEntityWorldError::InvalidState);
            }
        }

        let (contents, linkcontents) = self.aggregate_masks(AREA_ROOT_SECTOR)?;
        let root = self.sectors[usize::from(AREA_ROOT_SECTOR)];
        if root.contents_entities != contents || root.linkcontents_entities != linkcontents {
            return Err(AreaEntityWorldError::InvalidState);
        }
        Ok(())
    }

    fn aggregate_masks(&self, node_index: u16) -> Result<(u32, u32), AreaEntityWorldError> {
        let node = self.sectors[usize::from(node_index)];
        let mut contents = 0;
        let mut linkcontents = 0;
        for child in node.child {
            if child != 0 {
                let child_masks = self.aggregate_masks(child)?;
                contents |= child_masks.0;
                linkcontents |= child_masks.1;
            }
        }
        let mut entity = node.entities;
        while entity != 0 {
            let entity_index = usize::from(entity - 1);
            contents |= self.entity_contents[entity_index];
            linkcontents |= self.links[entity_index].linkcontents;
            entity = self.links[entity_index].next_entity_in_world_sector;
        }
        if node.contents_entities != contents || node.linkcontents_entities != linkcontents {
            return Err(AreaEntityWorldError::InvalidState);
        }
        Ok((contents, linkcontents))
    }

    pub fn link(
        &mut self,
        entity_num: u16,
        contents: u32,
        linkcontents: u32,
        bounds: AreaBounds,
        linkmin: [f32; 2],
        linkmax: [f32; 2],
    ) -> Result<(), AreaEntityWorldError> {
        let entity_index = usize::from(entity_num);
        if entity_index >= AREA_SECTOR_COUNT {
            return Err(AreaEntityWorldError::EntityIndexOutOfRange);
        }
        if !(0..2).all(|axis| {
            linkmin[axis].is_finite() && linkmax[axis].is_finite() && linkmin[axis] <= linkmax[axis]
        }) {
            return Err(AreaEntityWorldError::InvalidLinkBounds);
        }
        if linkcontents == 0 {
            self.unlink(entity_num)?;
            return Ok(());
        }

        self.entity_contents[entity_index] = contents;
        self.entity_bounds[entity_index] = Some(bounds);

        loop {
            let (node_index, mins, maxs, previous_contents, previous_linkcontents) =
                self.descend_and_accumulate(contents, linkcontents, linkmin, linkmax);
            let old_node = self.links[entity_index].world_sector;
            let can_update_in_place = old_node == node_index
                && previous_contents & !contents == 0
                && previous_linkcontents & !linkcontents == 0;

            if old_node == 0 {
                self.write_link(entity_num, linkcontents, linkmin, linkmax);
                self.add_entity_to_node(entity_num, node_index);
                self.sort_node(node_index, mins, maxs);
                return Ok(());
            }
            if can_update_in_place {
                self.write_link(entity_num, linkcontents, linkmin, linkmax);
                self.sort_node(node_index, mins, maxs);
                return Ok(());
            }

            self.remove_entity_link(entity_num);
        }
    }

    pub fn unlink(&mut self, entity_num: u16) -> Result<bool, AreaEntityWorldError> {
        let entity_index = usize::from(entity_num);
        if entity_index >= AREA_SECTOR_COUNT {
            return Err(AreaEntityWorldError::EntityIndexOutOfRange);
        }
        if self.links[entity_index].world_sector == 0 {
            self.entity_contents[entity_index] = 0;
            self.entity_bounds[entity_index] = None;
            return Ok(false);
        }
        self.remove_entity_link(entity_num);
        self.entity_contents[entity_index] = 0;
        self.entity_bounds[entity_index] = None;
        Ok(true)
    }

    pub fn translate(
        &mut self,
        entity_num: u16,
        delta: [f32; 3],
    ) -> Result<bool, AreaEntityWorldError> {
        let entity_index = usize::from(entity_num);
        if entity_index >= AREA_SECTOR_COUNT {
            return Err(AreaEntityWorldError::EntityIndexOutOfRange);
        }
        if !delta.iter().all(|value| value.is_finite()) {
            return Err(AreaEntityWorldError::InvalidLinkBounds);
        }
        let Some(bounds) = self.entity_bounds[entity_index] else {
            return Ok(false);
        };
        let link = self.links[entity_index];
        let mid = bounds.mid();
        let moved = AreaBounds {
            mid: [mid[0] + delta[0], mid[1] + delta[1], mid[2] + delta[2]],
            half: bounds.half(),
        };
        self.link(
            entity_num,
            self.entity_contents[entity_index],
            link.linkcontents,
            moved,
            [link.linkmin[0] + delta[0], link.linkmin[1] + delta[1]],
            [link.linkmax[0] + delta[0], link.linkmax[1] + delta[1]],
        )?;
        Ok(true)
    }

    pub fn entity_bounds(&self, entity_num: u16) -> Option<AreaBounds> {
        self.entity_bounds
            .get(usize::from(entity_num))
            .copied()
            .flatten()
    }

    pub fn query(&self, bounds: AreaBounds, mask: u32, capacity: usize) -> Vec<u16> {
        let mut out = Vec::new();
        self.query_node(AREA_ROOT_SECTOR, bounds, mask, capacity, &mut out);
        out
    }

    fn descend_and_accumulate(
        &mut self,
        contents: u32,
        linkcontents: u32,
        linkmin: [f32; 2],
        linkmax: [f32; 2],
    ) -> (u16, [f32; 2], [f32; 2], u32, u32) {
        let world_mins = self.world.mins();
        let world_maxs = self.world.maxs();
        let mut mins = [world_mins[0], world_mins[1]];
        let mut maxs = [world_maxs[0], world_maxs[1]];
        let mut node_index = AREA_ROOT_SECTOR;

        loop {
            let node = &mut self.sectors[usize::from(node_index)];
            let previous_contents = node.contents_entities;
            let previous_linkcontents = node.linkcontents_entities;
            node.contents_entities |= contents;
            node.linkcontents_entities |= linkcontents;
            let axis = usize::from(node.axis);
            let dist = node.dist;

            if linkmin[axis] > dist {
                mins[axis] = dist;
                if node.child[0] != 0 {
                    node_index = node.child[0];
                    continue;
                }
            } else if linkmax[axis] < dist {
                maxs[axis] = dist;
                if node.child[1] != 0 {
                    node_index = node.child[1];
                    continue;
                }
            }
            return (
                node_index,
                mins,
                maxs,
                previous_contents,
                previous_linkcontents,
            );
        }
    }

    fn write_link(
        &mut self,
        entity_num: u16,
        linkcontents: u32,
        linkmin: [f32; 2],
        linkmax: [f32; 2],
    ) {
        let link = &mut self.links[usize::from(entity_num)];
        link.linkcontents = linkcontents;
        link.linkmin = linkmin;
        link.linkmax = linkmax;
    }

    fn add_entity_to_node(&mut self, entity_num: u16, node_index: u16) {
        let encoded = entity_num + 1;
        let mut previous = None;
        let mut current = self.sectors[usize::from(node_index)].entities;
        while current != 0 && current - 1 <= entity_num {
            previous = Some(current - 1);
            current = self.links[usize::from(current - 1)].next_entity_in_world_sector;
        }

        let link = &mut self.links[usize::from(entity_num)];
        link.world_sector = node_index;
        link.next_entity_in_world_sector = current;
        if let Some(previous) = previous {
            self.links[usize::from(previous)].next_entity_in_world_sector = encoded;
        } else {
            self.sectors[usize::from(node_index)].entities = encoded;
        }
    }

    fn remove_entity_link(&mut self, entity_num: u16) {
        let entity_index = usize::from(entity_num);
        let node_index = self.links[entity_index].world_sector;
        debug_assert_ne!(node_index, 0);
        let encoded = entity_num + 1;
        let next = self.links[entity_index].next_entity_in_world_sector;

        if self.sectors[usize::from(node_index)].entities == encoded {
            self.sectors[usize::from(node_index)].entities = next;
        } else {
            let mut scan = self.sectors[usize::from(node_index)].entities;
            while self.links[usize::from(scan - 1)].next_entity_in_world_sector != encoded {
                scan = self.links[usize::from(scan - 1)].next_entity_in_world_sector;
                assert_ne!(scan, 0, "linked entity must exist in its world sector");
            }
            self.links[usize::from(scan - 1)].next_entity_in_world_sector = next;
        }
        self.links[entity_index] = AreaEntityLink::default();

        let surviving_node = self.prune_empty_nodes(node_index);
        self.recompute_aggregate_masks(surviving_node);
    }

    fn prune_empty_nodes(&mut self, mut node_index: u16) -> u16 {
        loop {
            let node = self.sectors[usize::from(node_index)];
            if node.entities != 0 || node.child != [0, 0] {
                return node_index;
            }
            self.sectors[usize::from(node_index)].contents_entities = 0;
            self.sectors[usize::from(node_index)].linkcontents_entities = 0;
            let parent = node.parent_or_next_free;
            if parent == 0 {
                return node_index;
            }

            self.sectors[usize::from(node_index)] = AreaSector {
                parent_or_next_free: self.free_head,
                ..AreaSector::default()
            };
            self.free_head = node_index;
            let parent_node = &mut self.sectors[usize::from(parent)];
            if parent_node.child[0] == node_index {
                parent_node.child[0] = 0;
            } else {
                debug_assert_eq!(parent_node.child[1], node_index);
                parent_node.child[1] = 0;
            }
            node_index = parent;
        }
    }

    fn recompute_aggregate_masks(&mut self, mut node_index: u16) {
        loop {
            let node = self.sectors[usize::from(node_index)];
            let mut contents = self.sectors[usize::from(node.child[0])].contents_entities
                | self.sectors[usize::from(node.child[1])].contents_entities;
            let mut linkcontents = self.sectors[usize::from(node.child[0])].linkcontents_entities
                | self.sectors[usize::from(node.child[1])].linkcontents_entities;
            let mut entity = node.entities;
            while entity != 0 {
                let entity_index = usize::from(entity - 1);
                contents |= self.entity_contents[entity_index];
                linkcontents |= self.links[entity_index].linkcontents;
                entity = self.links[entity_index].next_entity_in_world_sector;
            }
            if node.contents_entities == contents && node.linkcontents_entities == linkcontents {
                return;
            }
            self.sectors[usize::from(node_index)].contents_entities = contents;
            self.sectors[usize::from(node_index)].linkcontents_entities = linkcontents;
            if node.parent_or_next_free == 0 {
                return;
            }
            node_index = node.parent_or_next_free;
        }
    }

    fn sort_node(&mut self, node_index: u16, mins: [f32; 2], maxs: [f32; 2]) {
        let axis = usize::from(self.sectors[usize::from(node_index)].axis);
        let dist = self.sectors[usize::from(node_index)].dist;
        let mut previous = None;
        let mut entity = self.sectors[usize::from(node_index)].entities;

        while entity != 0 {
            let entity_num = entity - 1;
            let entity_index = usize::from(entity_num);
            let next = self.links[entity_index].next_entity_in_world_sector;
            let child_slot = if self.links[entity_index].linkmin[axis] > dist {
                Some(0)
            } else if self.links[entity_index].linkmax[axis] < dist {
                Some(1)
            } else {
                None
            };

            let Some(child_slot) = child_slot else {
                previous = Some(entity_num);
                entity = next;
                continue;
            };
            let mut child = self.sectors[usize::from(node_index)].child[child_slot];
            if child == 0 {
                let mut child_mins = mins;
                let mut child_maxs = maxs;
                if child_slot == 0 {
                    child_mins[axis] = dist;
                } else {
                    child_maxs[axis] = dist;
                }
                child = self.alloc_sector(child_mins, child_maxs);
                if child != 0 {
                    self.sectors[usize::from(child)].parent_or_next_free = node_index;
                    self.sectors[usize::from(node_index)].child[child_slot] = child;
                }
            }
            if child == 0 {
                previous = Some(entity_num);
                entity = next;
                continue;
            }

            if let Some(previous) = previous {
                self.links[usize::from(previous)].next_entity_in_world_sector = next;
            } else {
                self.sectors[usize::from(node_index)].entities = next;
            }
            self.links[entity_index].next_entity_in_world_sector = 0;
            self.add_entity_to_node(entity_num, child);
            self.sectors[usize::from(child)].contents_entities |=
                self.entity_contents[entity_index];
            self.sectors[usize::from(child)].linkcontents_entities |=
                self.links[entity_index].linkcontents;
            entity = next;
        }
    }

    fn alloc_sector(&mut self, mins: [f32; 2], maxs: [f32; 2]) -> u16 {
        if self.free_head == 0 {
            return 0;
        }
        let extents = [maxs[0] - mins[0], maxs[1] - mins[1]];
        let axis = usize::from(extents[0] <= extents[1]);
        if extents[axis] <= AREA_NODE_MIN_EXTENT {
            return 0;
        }

        let allocated = self.free_head;
        self.free_head = self.sectors[usize::from(allocated)].parent_or_next_free;
        self.sectors[usize::from(allocated)] = AreaSector {
            dist: (mins[axis] + maxs[axis]) * 0.5,
            axis: axis as u16,
            ..AreaSector::default()
        };
        allocated
    }

    fn query_node(
        &self,
        node_index: u16,
        bounds: AreaBounds,
        mask: u32,
        capacity: usize,
        out: &mut Vec<u16>,
    ) {
        if node_index == 0
            || out.len() >= capacity
            || self.sectors[usize::from(node_index)].contents_entities & mask == 0
        {
            return;
        }
        let node = self.sectors[usize::from(node_index)];
        let mut entity = node.entities;
        while entity != 0 {
            let entity_num = entity - 1;
            let entity_index = usize::from(entity_num);
            if self.entity_contents[entity_index] & mask != 0
                && self.entity_bounds[entity_index].is_some_and(|row| row.overlaps(bounds))
            {
                out.push(entity_num);
                if out.len() == capacity {
                    return;
                }
            }
            entity = self.links[entity_index].next_entity_in_world_sector;
        }

        let axis = usize::from(node.axis);
        let query_mins = bounds.mins();
        let query_maxs = bounds.maxs();
        if query_maxs[axis] <= node.dist {
            self.query_node(node.child[1], bounds, mask, capacity, out);
        } else if query_mins[axis] >= node.dist {
            self.query_node(node.child[0], bounds, mask, capacity, out);
        } else {
            self.query_node(node.child[0], bounds, mask, capacity, out);
            self.query_node(node.child[1], bounds, mask, capacity, out);
        }
    }
}
