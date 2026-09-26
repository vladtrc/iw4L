use crate::{dd, dom, ffa};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GameModeKind {
    #[default]
    FreeForAll = 0,

    Demolition = 1,

    Domination = 2,

    TeamDeathmatch = 3,

    SearchAndDestroy = 4,

    CaptureTheFlag = 5,

    Headquarters = 6,

    Sabotage = 7,
}

impl GameModeKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().as_bytes() {
            b"dm" | b"ffa" | b"freeforall" | b"free_for_all" | b"deathmatch" => {
                Some(Self::FreeForAll)
            }

            b"dd" | b"dem" | b"demolition" => Some(Self::Demolition),
            b"dom" | b"domination" => Some(Self::Domination),
            b"war" | b"tdm" | b"teamdeathmatch" | b"team_deathmatch" => Some(Self::TeamDeathmatch),
            b"sd" | b"searchanddestroy" | b"search_and_destroy" => Some(Self::SearchAndDestroy),
            b"ctf" | b"capturetheflag" | b"capture_the_flag" => Some(Self::CaptureTheFlag),
            b"koth" | b"hq" | b"headquarters" => Some(Self::Headquarters),
            b"sab" | b"sabotage" => Some(Self::Sabotage),
            _ => None,
        }
    }

    pub fn parse_ascii_ignore_case(s: &str) -> Option<Self> {
        let mut buf = [0u8; 32];
        let bytes = s.trim().as_bytes();
        if bytes.len() > buf.len() {
            return None;
        }
        for (i, &b) in bytes.iter().enumerate() {
            buf[i] = b.to_ascii_lowercase();
        }
        Self::parse(core::str::from_utf8(&buf[..bytes.len()]).ok()?)
    }

    pub fn token(self) -> &'static str {
        match self {
            Self::FreeForAll => ffa::GAMETYPE_TOKEN,
            Self::Demolition => dd::GAMETYPE_TOKEN,
            Self::Domination => dom::GAMETYPE_TOKEN,
            Self::TeamDeathmatch => "war",
            Self::SearchAndDestroy => "sd",
            Self::CaptureTheFlag => "ctf",
            Self::Headquarters => "koth",
            Self::Sabotage => "sab",
        }
    }

    pub fn script_tokens(self) -> &'static [&'static str] {
        match self {
            Self::FreeForAll => &[ffa::GAMETYPE_TOKEN],
            Self::Demolition => &[dd::GAMETYPE_TOKEN, "dem"],
            Self::Domination => &[dom::GAMETYPE_TOKEN],
            Self::TeamDeathmatch => &["war", "tdm"],
            Self::SearchAndDestroy => &["sd"],
            Self::CaptureTheFlag => &["ctf"],
            Self::Headquarters => &["koth"],
            Self::Sabotage => &["sab"],
        }
    }

    pub fn wire_tag(self) -> u8 {
        self as u8
    }

    pub fn from_wire_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::FreeForAll),
            1 => Some(Self::Demolition),
            2 => Some(Self::Domination),
            3 => Some(Self::TeamDeathmatch),
            4 => Some(Self::SearchAndDestroy),
            5 => Some(Self::CaptureTheFlag),
            6 => Some(Self::Headquarters),
            7 => Some(Self::Sabotage),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::FreeForAll => ffa::DISPLAY_NAME,
            Self::Demolition => dd::DISPLAY_NAME,
            Self::Domination => dom::DISPLAY_NAME,
            Self::TeamDeathmatch => "TEAM DEATHMATCH",
            Self::SearchAndDestroy => "SEARCH AND DESTROY",
            Self::CaptureTheFlag => "CAPTURE THE FLAG",
            Self::Headquarters => "HEADQUARTERS",
            Self::Sabotage => "SABOTAGE",
        }
    }

    pub fn is_team(self) -> bool {
        !matches!(self, Self::FreeForAll)
    }

    pub fn team_start_classname(self, axis: bool) -> Option<&'static str> {
        match self {
            Self::FreeForAll
            | Self::TeamDeathmatch
            | Self::SearchAndDestroy
            | Self::CaptureTheFlag
            | Self::Headquarters
            | Self::Sabotage => None,
            Self::Domination => Some(if axis {
                dom::START_SPAWN_AXIS
            } else {
                dom::START_SPAWN_ALLIES
            }),
            Self::Demolition => Some(if axis {
                dd::START_SPAWN_DEFENDER
            } else {
                dd::START_SPAWN_ATTACKER
            }),
        }
    }

    pub fn team_grid_classnames(self, axis: bool) -> &'static [&'static str] {
        match self {
            Self::FreeForAll
            | Self::TeamDeathmatch
            | Self::SearchAndDestroy
            | Self::CaptureTheFlag
            | Self::Headquarters
            | Self::Sabotage => &[],
            Self::Domination => &[dom::SPAWN_CLASSNAME],
            Self::Demolition => {
                if axis {
                    &[
                        dd::SPAWN_DEFENDER,
                        dd::SPAWN_DEFENDER_A,
                        dd::SPAWN_DEFENDER_B,
                    ]
                } else {
                    &[
                        dd::SPAWN_ATTACKER,
                        dd::SPAWN_ATTACKER_A,
                        dd::SPAWN_ATTACKER_B,
                    ]
                }
            }
        }
    }
}
