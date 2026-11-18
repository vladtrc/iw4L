use crate::lanes::lane;
use asset_core::FamilyId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneStatus {
    SupportedPopulated,
    SupportedEmpty,
    MissingDecoder,
    MissingEvidence,
    UnsupportedByRuntimeProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedCapability {
    Envelope,
    PreparedWorld,
    CollisionSpawns,
    WeaponCatalog,

    BodySkeleton,
    PlayableFfa,
}

pub fn lane_status(game: FamilyId, capability: PreparedCapability) -> LaneStatus {
    let Some(lane) = lane(game) else {
        return LaneStatus::MissingDecoder;
    };
    for &(cap, status) in lane.capabilities() {
        if cap == capability {
            return status;
        }
    }

    LaneStatus::MissingEvidence
}
