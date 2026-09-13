use bevy::prelude::*;
use dpvs_iw4::CPlane;

#[derive(Resource, Clone, Debug, Default)]
pub struct WorldDpvsCells {
    pub planes: Vec<CPlane>,
    pub nodes: Vec<u16>,
    pub cell_count: usize,
}

impl WorldDpvsCells {
    #[must_use]
    pub fn is_ready(&self) -> bool {
        !self.nodes.is_empty() && self.cell_count > 0
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct PublishedCellVis {
    pub words: Vec<u32>,
    pub cell_count: usize,
    pub vis_all: bool,
}

#[derive(Component)]
pub struct DynEntModelEntity;
