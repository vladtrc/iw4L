use core::fmt;

use diag::gap::{self as ledger, GapLedger};
use gamemode_iw4::{ScriptGap as GapId, ScriptGapCause as Cause};

const SCRIPT_GAP_COUNT: usize = GapId::ALL.len();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ScriptGapKey(GapId);

impl ledger::Gap for ScriptGapKey {
    const ALL: &'static [Self] = &[
        Self(GapId::DestructablesBlockArea),
        Self(GapId::DestructablesPlayFx),
        Self(GapId::FlammableCrateFx),
        Self(GapId::FlammableCratePhysics),
        Self(GapId::ExplodableBarrelPhysics),
        Self(GapId::RadiationDoorKillEdge),
        Self(GapId::RadiationSwitchExploder),
        Self(GapId::RadiationDiggerFx),
        Self(GapId::RadiationTunnelLightFx),
        Self(GapId::RadiationDoorDropToGround),
        Self(GapId::DomOnUse),
        Self(GapId::DemOnUseObject),
    ];

    fn name(self) -> &'static str {
        self.0.name()
    }

    fn is_standing(self) -> bool {
        self.0.is_standing()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ScriptGapCauseKey(Cause);

impl ledger::GapCause for ScriptGapCauseKey {
    type Gap = ScriptGapKey;

    fn gap(&self) -> ScriptGapKey {
        ScriptGapKey(self.0.gap())
    }
}

impl fmt::Display for ScriptGapCauseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Default)]
pub struct ScriptGaps {
    ledger: GapLedger<ScriptGapCauseKey, SCRIPT_GAP_COUNT>,
    reported: String,
}

impl Clone for ScriptGaps {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl ScriptGaps {
    pub fn raise(&mut self, cause: Cause) {
        self.ledger.raise(ScriptGapCauseKey(cause));
    }

    pub fn hits(&self, gap: GapId) -> u64 {
        self.ledger.hits(ScriptGapKey(gap))
    }

    pub fn is_live(&self, gap: GapId) -> bool {
        self.ledger.is_live(ScriptGapKey(gap))
    }

    pub fn live(&self) -> impl Iterator<Item = GapId> + '_ {
        self.ledger.live().map(|key| key.0)
    }

    pub fn count(&self) -> usize {
        self.ledger.count()
    }

    pub fn report(&mut self) {
        let mut signature = String::new();
        self.ledger
            .write_signature(&mut signature)
            .expect("writing into a String cannot fail");
        if signature == self.reported {
            return;
        }
        self.reported = signature;
        let names: Vec<&str> = self.live().map(GapId::name).collect();
        diag::info!(
            Sim,
            "script: {} gaps live: {}",
            self.count(),
            names.join(" ")
        );
        for gap in self.live() {
            if let Some(cause) = self.ledger.cause(ScriptGapKey(gap)) {
                diag::info!(
                    Sim,
                    "script gap {} (x{}): {}",
                    gap.name(),
                    self.ledger.hits(ScriptGapKey(gap)),
                    cause
                );
            }
        }
    }
}
