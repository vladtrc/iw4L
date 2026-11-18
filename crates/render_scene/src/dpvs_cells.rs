use bevy::prelude::*;
use dpvs_iw4::CPlane;

#[derive(Resource, Clone, Debug, Default)]
pub struct WorldDpvsCells {
    pub planes: Vec<CPlane>,
    pub nodes: Vec<u16>,
    pub cell_count: usize,
    pub atlas: Vec<(usize, WorldDpvsCells)>,
}

impl WorldDpvsCells {
    pub fn for_world(&self, world: usize) -> Option<&Self> {
        if world == 0 {
            Some(self)
        } else {
            self.atlas.iter().find(|(w, _)| *w == world).map(|(_, c)| c)
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        !self.nodes.is_empty() && self.cell_count > 0
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct PublishedCellVis {
    /// Physical domain of the root cell IDs.
    pub world: usize,
    pub views: Vec<PublishedCellView>,
    pub words: Vec<u32>,
    pub cell_count: usize,
    pub vis_all: bool,
}

#[derive(Component)]
#[require(bevy::prelude::DespawnOnExit<frame::MatchScope> = bevy::prelude::DespawnOnExit(frame::MatchScope::Live))]
pub struct DynEntModelEntity;

#[derive(Clone, Debug, Default)]
pub struct PublishedCellView {
    pub view: usize,
    pub world: usize,
    pub shared: bool,
    pub eye: [f32; 3],
    pub words: Vec<u32>,
    pub cell_count: usize,
    pub planes: Vec<[f32; 4]>,
    pub vis_all: bool,
}

impl PublishedCellView {
    pub fn admits_bounds(&self, cells: &WorldDpvsCells, bounds: dpvs_iw4::Bounds) -> bool {
        if dpvs_iw4::bounds_culled(bounds, &self.planes) {
            return false;
        }
        if self.vis_all || !cells.is_ready() {
            return true;
        }
        let dpvs = dpvs_iw4::DpvsPlanes {
            planes: &cells.planes,
            nodes: &cells.nodes,
            cell_count: cells.cell_count as u32,
        };
        let mut seen = false;
        dpvs_iw4::scene_ent_cells(&dpvs, bounds, &mut |cell| {
            let cell = cell as usize;
            seen |= self
                .words
                .get(cell / 32)
                .is_some_and(|w| w & (1u32 << (cell % 32)) != 0);
        });
        seen
    }
}
