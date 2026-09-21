use core::fmt;

/// Named holes in the script layer. Variants are the GSC symbol they stand in
/// for, so a live gap is a count of a specific function we reached and could
/// not finish.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptGap {
    DestructablesBlockArea,
    DestructablesPlayFx,
    FlammableCrateFx,
    FlammableCratePhysics,
    ExplodableBarrelPhysics,
    DestructiblePartLaunch,
    RadiationDoorKillEdge,
    RadiationSwitchExploder,
    RadiationDiggerFx,
    RadiationTunnelLightFx,
    RadiationDoorDropToGround,
    DomOnUse,
    DemOnUseObject,
}

impl ScriptGap {
    pub const ALL: &'static [Self] = &[
        Self::DestructablesBlockArea,
        Self::DestructablesPlayFx,
        Self::FlammableCrateFx,
        Self::FlammableCratePhysics,
        Self::ExplodableBarrelPhysics,
        Self::DestructiblePartLaunch,
        Self::RadiationDoorKillEdge,
        Self::RadiationSwitchExploder,
        Self::RadiationDiggerFx,
        Self::RadiationTunnelLightFx,
        Self::RadiationDoorDropToGround,
        Self::DomOnUse,
        Self::DemOnUseObject,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::DestructablesBlockArea => "gsc.mp._destructables.blockArea",
            Self::DestructablesPlayFx => "gsc.mp._destructables.destructable_destruct",
            Self::FlammableCrateFx => "gsc.mp._interactive_objects.flammable_crate_explode",
            Self::FlammableCratePhysics => "gsc.mp._interactive_objects.flammable_crate_explode",
            Self::ExplodableBarrelPhysics => "gsc.mp._explosive_barrels.explodable_barrel_explode",
            Self::DestructiblePartLaunch => "gsc.common_scripts._destructible.physics_launch",
            Self::RadiationDoorKillEdge => "gsc.mp.mp_radiation.kill_edge_players_func",
            Self::RadiationSwitchExploder => "gsc.mp.mp_radiation.turnSwitchPanelGreen",
            Self::RadiationDiggerFx => "gsc.mp.mp_radiation.digger_dig_think",
            Self::RadiationTunnelLightFx => "gsc.mp.mp_radiation.tunnel_lights",
            Self::RadiationDoorDropToGround => "gsc.mp.mp_radiation.dropEverythingOnDoorsToGround",
            Self::DomOnUse => "gsc.dom.onUse",
            Self::DemOnUseObject => "gsc.dem.onUseObject",
        }
    }

    pub const fn is_standing(self) -> bool {
        false
    }

    pub fn from_use_gap_id(id: &'static str) -> Option<Self> {
        match id {
            "gsc.dom.onUse" => Some(Self::DomOnUse),
            "gsc.dem.onUseObject" => Some(Self::DemOnUseObject),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptGapCause {
    BlockAreaMissingTdmSpawns,
    DestructablePlayFx { source_ordinal: u32 },
    FlammableCrateFx { source_ordinal: u32 },
    FlammableCratePhysics { source_ordinal: u32 },
    ExplodableBarrelPhysics { source_ordinal: u32 },
    DestructiblePartLaunch { source_ordinal: u32 },
    RadiationDoorKillEdge,
    RadiationSwitchExploder,
    RadiationDiggerFx,
    RadiationTunnelLightFx,
    RadiationDoorDropToGround,
    DomOnUse { object: u32 },
    DemOnUseObject { object: u32 },
}

impl ScriptGapCause {
    pub const fn gap(self) -> ScriptGap {
        match self {
            Self::BlockAreaMissingTdmSpawns => ScriptGap::DestructablesBlockArea,
            Self::DestructablePlayFx { .. } => ScriptGap::DestructablesPlayFx,
            Self::FlammableCrateFx { .. } => ScriptGap::FlammableCrateFx,
            Self::FlammableCratePhysics { .. } => ScriptGap::FlammableCratePhysics,
            Self::ExplodableBarrelPhysics { .. } => ScriptGap::ExplodableBarrelPhysics,
            Self::DestructiblePartLaunch { .. } => ScriptGap::DestructiblePartLaunch,
            Self::RadiationDoorKillEdge => ScriptGap::RadiationDoorKillEdge,
            Self::RadiationSwitchExploder => ScriptGap::RadiationSwitchExploder,
            Self::RadiationDiggerFx => ScriptGap::RadiationDiggerFx,
            Self::RadiationTunnelLightFx => ScriptGap::RadiationTunnelLightFx,
            Self::RadiationDoorDropToGround => ScriptGap::RadiationDoorDropToGround,
            Self::DomOnUse { .. } => ScriptGap::DomOnUse,
            Self::DemOnUseObject { .. } => ScriptGap::DemOnUseObject,
        }
    }
}

impl fmt::Display for ScriptGapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockAreaMissingTdmSpawns => f.write_str(
                "blockArea also walks mp_tdm_spawn; this runtime does not load that classname",
            ),
            Self::DestructablePlayFx { source_ordinal } => write!(
                f,
                "destructable {source_ordinal} authored script_fxid; loadfx/playfx is not wired"
            ),
            Self::FlammableCrateFx { source_ordinal } => write!(
                f,
                "flammable crate {source_ordinal} loadfx/playfx and ignite/explode sounds are not wired"
            ),
            Self::FlammableCratePhysics { source_ordinal } => write!(
                f,
                "flammable crate {source_ordinal} physicsexplosionsphere and tipped pose are not wired"
            ),
            Self::ExplodableBarrelPhysics { source_ordinal } => write!(
                f,
                "explodable barrel {source_ordinal} physicsexplosionsphere, earthquake, piece2 husk, and tipped pose are not wired"
            ),
            Self::DestructiblePartLaunch { source_ordinal } => write!(
                f,
                "destructible {source_ordinal} launched a part; the part hides but no physics model is spawned"
            ),
            Self::RadiationDoorKillEdge => {
                f.write_str("radiation doors closing: kill_edge_players_func DoDamage is not wired")
            }
            Self::RadiationSwitchExploder => {
                f.write_str("radiation switch panel exploders 2001/2002 are not wired")
            }
            Self::RadiationDiggerFx => {
                f.write_str("radiation digger PlayFX sand and excavator loop sounds are not wired")
            }
            Self::RadiationTunnelLightFx => {
                f.write_str("radiation tunnel PlayFXOnTag green_light / blink_light is not wired")
            }
            Self::RadiationDoorDropToGround => f.write_str(
                "radiation doors moving: dropAllToGround PhysicsExplosionSphere / drop weapons and crates is not wired",
            ),
            Self::DomOnUse { object } => write!(f, "dom flag {object} onUse is unbound"),
            Self::DemOnUseObject { object } => {
                write!(f, "dem bombzone {object} onUseObject is unbound")
            }
        }
    }
}
