use crate::ZoneLane;
use asset_core::FamilyId;
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(Default)]
pub struct ZoneLanes(BTreeMap<FamilyId, &'static dyn ZoneLane>);
impl ZoneLanes {
    pub fn register(
        &mut self,
        id: FamilyId,
        lane: &'static dyn ZoneLane,
    ) -> Result<(), &'static str> {
        if lane.game() != id {
            return Err("zone lane family mismatch");
        }
        if self.0.contains_key(&id) {
            return Err("duplicate zone lane");
        }
        self.0.insert(id, lane);
        Ok(())
    }
}
static LANES: OnceLock<ZoneLanes> = OnceLock::new();
pub fn install_zone_lanes(lanes: ZoneLanes) -> Result<(), &'static str> {
    LANES.set(lanes).map_err(|_| "zone lanes already installed")
}
pub fn lane(id: FamilyId) -> Option<&'static dyn ZoneLane> {
    LANES.get()?.0.get(&id).copied()
}
