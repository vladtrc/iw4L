#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdoptGap {
    pub field: &'static str,

    pub reason: &'static str,
}

pub const ADOPT_GAPS: &[AdoptGap] = &[
    AdoptGap {
        field: "spawn_rng / combat_rng / bot_rng",
        reason: "authority-only draws. A snapshot carries draw counters (RngDebugMeta) but not \
                 generator state, so a client cannot resume the sequence. It does not need to: \
                 every draw belongs to a decision authority already made and replicated \
                 (spawn placement, spread). A client that re-runs step draws from its own \
                 stream, gets a different answer, and has that answer overwritten on the next \
                 adopt — which is why spread-bearing shot results are authority-validated \
                 rather than predicted.",
    },
    AdoptGap {
        field: "next_shot / next_projectile",
        reason: "id allocators. A predicted shot gets a client-local id that authority never \
                 issued; the authoritative ids arrive with the snapshot that adopts over them. \
                 Predicted ids therefore must not escape into presentation keyed by id — \
                 events are read from the adopted snapshot, not from predicted output. \
                 Projectile flight itself is predicted (deterministic ballistics); only the id \
                 is withheld until adopt. Presentation merges by (owner, weapon) via \
                 net::merge_presented_projectiles.",
    },
    AdoptGap {
        field: "collision_history",
        reason: "lag-compensation rewind buffer. Only authority runs lag comp, because only \
                 authority resolves damage. A client's history is rebuilt from its own \
                 post-movement poses each predicted tick and is never read by anything the \
                 client presents.",
    },
    AdoptGap {
        field: "entity_collision_history",
        reason: "script-model materialized collision ring for hitscan rewind. Authority-only, \
                 same argument as collision_history: a predicting client does not resolve \
                 damage and must not restore live OBB from a snapshot.",
    },
    AdoptGap {
        field: "lagcomp_sample",
        reason: "per-attacker presentation-sample provenance for hitscan rewind — what the \
                 shooter's command was aimed at. Authority-only: the predicting client does \
                 not resolve damage, so it never reads this map. Host and remote shooters go \
                 through the same resolver; there is no zero-rewind seat.",
    },
    AdoptGap {
        field: "journal",
        reason: "cleared at the top of every step and republished from the adopted meta. \
                 Adopting it would make the client present one-shot events twice: once from \
                 the snapshot and once from its own re-run.",
    },
    AdoptGap {
        field: "next_event",
        reason: "sequence allocator for the journal above. Same argument.",
    },
    AdoptGap {
        field: "old_buttons / old_cmd_angles",
        reason: "pmove edge state (retail `pmove_t.oldcmd`). Not in the snapshot, but not lost \
                 either: buttons and angles are on the previous usercmd. A matched ack is that \
                 command; a retired or missing ack still has the last predicted command \
                 (`CG_PredictPlayerState` / `CL_GetUserCmd(cmdNum-1)`). \
                 `SimWorld::set_old_cmd` is how the client puts them back, and \
                 `net::ClientPrediction` calls it on every adopt — including Retired. \
                 Skipping Retired was the predicted-sprint toggle: held `BUTTON_SPRINT` on a \
                 snapshot that already has `PMF_SPRINTING` looks like a fresh press.",
    },
    AdoptGap {
        field: "clip_brushes / weapon_def_scales / weapon_combat / equipment_runtime / bootstrap",
        reason: "content, not state. Identical on both sides by construction and checked by \
                 `content_digest`; adopting it from a snapshot would mean shipping the map and \
                 the weapon tables every tick.",
    },
    AdoptGap {
        field: "root_seed / running",
        reason: "match identity, fixed at bootstrap. `running` is latched by the first step on \
                 either side.",
    },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdoptReport {
    pub players: usize,

    pub clients: usize,

    pub projectiles: usize,

    pub dropped: usize,

    pub content_mismatch: bool,
}

pub const ADOPT_GAP_COUNT: usize = ADOPT_GAPS.len();
