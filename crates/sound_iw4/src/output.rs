#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClientMask(pub u64);

impl ClientMask {
    pub const EVERYONE: ClientMask = ClientMask(u64::MAX);

    #[must_use]
    pub const fn with(self, client: ClientId) -> Self {
        ClientMask(self.0 | (1u64 << client.0))
    }

    pub const fn contains(self, client: ClientId) -> bool {
        self.0 & (1u64 << client.0) != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Origin(pub [f32; 3]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Team {
    Allies,
    Axis,
}

impl Team {
    pub const fn as_str(self) -> &'static str {
        match self {
            Team::Allies => "allies",
            Team::Axis => "axis",
        }
    }

    pub const fn other(self) -> Team {
        match self {
            Team::Allies => Team::Axis,
            Team::Axis => Team::Allies,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersTeam {
    Allies,
    Axis,
    Spectator,
}

impl PersTeam {
    pub const fn playing(self) -> Option<Team> {
        match self {
            PersTeam::Allies => Some(Team::Allies),
            PersTeam::Axis => Some(Team::Axis),
            PersTeam::Spectator => None,
        }
    }
}

pub const VOICE_INFIX: &str = "1mc_";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Alias {
    Literal(&'static str),

    TeamMusic { team: Team, suffix: &'static str },

    Voice { team: Team, line: &'static str },
}

impl Alias {
    pub const fn stem(self) -> &'static str {
        match self {
            Alias::Literal(s)
            | Alias::TeamMusic { suffix: s, .. }
            | Alias::Voice { line: s, .. } => s,
        }
    }

    pub const fn prefix_team(self) -> Option<Team> {
        match self {
            Alias::Literal(_) => None,
            Alias::TeamMusic { team, .. } | Alias::Voice { team, .. } => Some(team),
        }
    }
}

pub fn team_voice_prefix(_team: Team) -> &'static str {
    panic!(
        "getTeamVoicePrefix needs the mp/factionTable.csv StringTable (gap G1, S5); the prefix is not inferred from an emblem icon stem"
    )
}

pub fn music_bus_interaction(_playing: Alias, _incoming: Alias) -> ! {
    panic!(
        "gap G2: ScrCmd_PlayLocalSound has no stop-previous semantics; what a second music alias does over a playing one (ducking, snd_alias_list_t voice limit) has not been read"
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Output {
    PlayLocalSound {
        client: ClientId,
        alias: Alias,
    },

    StopLocalSound {
        client: ClientId,
        alias: Alias,
    },

    EntitySound {
        origin: Origin,
        alias: Alias,
        client_mask: ClientMask,

        ignore: Option<ClientId>,
    },
}
