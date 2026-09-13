use crate::kind::GameModeKind;

pub const AIRDROP_PALLET: &str = "airdrop_pallet";

pub const fn allowed_after_main(kind: GameModeKind) -> &'static [&'static str] {
    match kind {
        GameModeKind::FreeForAll => &["dm", AIRDROP_PALLET],
        GameModeKind::Demolition => &["dd", "bombzone", "blocker", AIRDROP_PALLET],
        GameModeKind::Domination => &["dom", AIRDROP_PALLET],
    }
}

pub fn gameobject_survives(script_gameobjectname: &str, kind: GameModeKind) -> bool {
    gameobject_survives_in(script_gameobjectname, allowed_after_main(kind))
}

pub fn gameobject_survives_in(script_gameobjectname: &str, allowed: &[&str]) -> bool {
    if script_gameobjectname.is_empty() {
        return true;
    }
    script_gameobjectname
        .split_ascii_whitespace()
        .any(|token| allowed.iter().copied().any(|want| want == token))
}
