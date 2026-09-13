use crate::phase::Team;

pub const TEAM_COLOR_DVAR_STEM: &str = "g_TeamColor";

pub const TEAM_COLOR_ALLIES_DVAR: &str = "g_TeamColor_Allies";

pub const TEAM_COLOR_AXIS_DVAR: &str = "g_TeamColor_Axis";

pub const TEAM_COLOR_FREE_DVAR: &str = "g_TeamColor_Free";

pub const EV_PLAY_FX: &str = "EV_PLAY_FX";

pub const WEAPONDEF_EXPLOSION_EFFECT_NAME: &str = "projExplosionEffect";

pub fn team_color_dvar(team: Team) -> &'static str {
    match team {
        Team::Free => TEAM_COLOR_FREE_DVAR,
        Team::Axis => TEAM_COLOR_AXIS_DVAR,
        Team::Allies => TEAM_COLOR_ALLIES_DVAR,
    }
}

pub fn team_color_suffix(team: Team) -> &'static str {
    match team {
        Team::Free => "Free",
        Team::Axis => "Axis",
        Team::Allies => "Allies",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomFlagVisualChannel {
    WorldModel,

    ScriptPlayFx,

    ObjectiveIcon,

    TeamColorDvar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BombExplodeVisualChannel {
    ScriptSpawnFx,

    WeaponDefExplosionEffect,

    ScriptPlayFx,

    EntityEventPlayFx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BombSiteDestroyChannel {
    WorldModel,

    UnlinkBrushSolid,

    ExplodeFx,

    RadiusDamage,
}

pub const SCRIPT_BRUSHMODEL_CLASSNAME: &str = "script_brushmodel";
