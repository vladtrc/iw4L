//! Named map × mode × traversal coverage. A bake or node count is not a support claim.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Traversal {
    Walk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapModeTell {
    pub map: &'static str,
    pub mode: &'static str,
    pub traversal: Traversal,
    pub combat: bool,
    pub objective: bool,
    pub diagnostic: bool,
}

/// First-version tells. `mp_terminal` is diagnostic only. Drop is a walk-off;
/// Mantle/Ladder stay `Unsupported` and are not in this table.
pub const V1_TELLS: &[MapModeTell] = &[
    MapModeTell {
        map: "mp_boneyard",
        mode: "ffa",
        traversal: Traversal::Walk,
        combat: true,
        objective: false,
        diagnostic: false,
    },
    MapModeTell {
        map: "mp_boneyard",
        mode: "domination",
        traversal: Traversal::Walk,
        combat: true,
        objective: true,
        diagnostic: false,
    },
    MapModeTell {
        map: "mp_terminal",
        mode: "ffa",
        traversal: Traversal::Walk,
        combat: false,
        objective: false,
        diagnostic: true,
    },
];
