use crate::ClientId;
use gamemode_iw4::{Team, UseHoldLoopState, dd};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectiveFlash {
    pub teams: u8,

    pub start_ms: u32,

    pub stop_ms: Option<u32>,
}

impl ObjectiveFlash {
    pub const AXIS: u8 = 1;

    pub const ALLIES: u8 = 2;

    #[must_use]
    pub fn team_bit(team: Team) -> u8 {
        match team {
            Team::Axis => Self::AXIS,
            Team::Allies => Self::ALLIES,
            _ => 0,
        }
    }

    #[must_use]
    pub fn shows_to(&self, team: Team) -> bool {
        self.teams & Self::team_bit(team) != 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectiveView {
    pub id: u32,
    pub model_source: u32,
    pub label: String,
    pub origin: [f32; 3],
    pub owner: Team,
    pub progress: f32,
    pub capturing: Team,
    pub contested: bool,
    pub users: Vec<ClientId>,

    pub flash: Option<ObjectiveFlash>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveHull {
    pub mid: [f32; 3],
    pub half: [f32; 3],
    pub slabs: Vec<([f32; 3], f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BombSite {
    pub view: ObjectiveView,
    pub intact_sources: Vec<u32>,
    pub destroyed_sources: Vec<u32>,
    pub hulls: Vec<ObjectiveHull>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub planted_at_ms: Option<u32>,
    pub planter: Option<ClientId>,
    pub bomb_origin: [f32; 3],
    pub bomb_angles: [f32; 3],
    pub destroyed: bool,
    pub user: Option<ClientId>,
    pub return_weapon: Option<u32>,
    pub hold: UseHoldLoopState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveMatch {
    pub flags: Vec<ObjectiveView>,
    pub flag_models: [String; 3],
    pub use_weapons: [u32; 2],
    pub restoring: Vec<(ClientId, u32, u32)>,
    pub bombs: Vec<BombSite>,
    pub last_plant: Option<(ClientId, u32)>,
    pub scores: [i32; 3],
    pub attackers: Team,
    pub round: u32,
    pub round_remaining_ms: u32,
    pub round_end_at_ms: Option<u32>,
    pub winner: Option<Team>,
    pub match_over: bool,
}
impl Default for ObjectiveMatch {
    fn default() -> Self {
        Self {
            flags: Vec::new(),
            flag_models: Default::default(),
            use_weapons: [0; 2],
            restoring: Vec::new(),
            bombs: Vec::new(),
            last_plant: None,
            scores: [0; 3],
            attackers: Team::Allies,
            round: 1,
            round_remaining_ms: dd::TIME_LIMIT_MS,
            round_end_at_ms: None,
            winner: None,
            match_over: false,
        }
    }
}
impl ObjectiveMatch {
    pub(crate) fn constrain_cmds(&self, cmds: &mut [(ClientId, playerstate_iw4::UserCmd)]) {
        for (id, cmd) in cmds {
            // Active use (plant/defuse): the player is linked to the site, so
            // translation is dropped while viewangles, stance and the melee or
            // grenade cancels keep flowing.
            if let Some(site) = self.bombs.iter().find(|b| b.user == Some(*id)) {
                let weapon = self.use_weapons[usize::from(site.planted_at_ms.is_some())];
                cmd.weapon = weapon as u16;
                cmd.weapon_mapped = weapon as u16;
                cmd.forwardmove = 0;
                cmd.rightmove = 0;
                continue;
            }
            // The briefcase is taken back after the unlink, and the freed player
            // keeps full movement: steer the weapon switch, never the movement.
            if let Some((_, weapon, _)) = self.restoring.iter().find(|(c, _, _)| c == id) {
                cmd.weapon = *weapon as u16;
                cmd.weapon_mapped = *weapon as u16;
            }
        }
    }
    pub fn model_visible(&self, source: u32) -> Option<bool> {
        self.bombs.iter().find_map(|site| {
            if site.intact_sources.contains(&source) {
                Some(!site.destroyed)
            } else if site.destroyed_sources.contains(&source) {
                Some(site.destroyed)
            } else {
                None
            }
        })
    }
    pub fn collision_active(&self, owner: crate::AuthorityModelOwner) -> bool {
        owner
            .script_model()
            .and_then(|id| self.model_visible(id.to_wire()))
            .unwrap_or(true)
    }
    pub fn defenders(&self) -> Team {
        if self.attackers == Team::Allies {
            Team::Axis
        } else {
            Team::Allies
        }
    }
}

pub(crate) fn touching(origin: [f32; 3], site: &BombSite) -> bool {
    let mid: [f32; 3] = std::array::from_fn(|i| {
        origin[i]
            + (crate::bullet_collision::PLAYER_MAXS[i] + crate::bullet_collision::PLAYER_MINS[i])
                * 0.5
    });
    let half: [f32; 3] = std::array::from_fn(|i| {
        (crate::bullet_collision::PLAYER_MAXS[i] - crate::bullet_collision::PLAYER_MINS[i]) * 0.5
    });
    site.hulls.iter().any(|h| {
        gamemode_iw4::use_bind::capsule_trigger_hull_contact(mid, half, h.mid, h.half, &h.slabs)
    })
}
