use core::fmt;

use diag::gap::{self as ledger, Gap as _, GapLedger};

const FX_GAP_COUNT: usize = <FxGap as ledger::Gap>::ALL.len();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FxGap {
    TrailCompressBasis,

    TrailIntersampleCull,

    TrailDefLookup,

    TrailCodeMesh,

    ElemCollideMotion,

    ElemMotionLookup,

    ElemImpactSpawn,

    ElemDeathSpawn,

    ElemLightingFrac,

    ElemEmitSpawn,

    ElemEmitOrientQuat,

    ElemEmitRandVariance,

    ElemSoundSpawn,

    ElemDecalSpawn,

    MarkFragments,

    ElemRunnerSpawn,
}

impl ledger::Gap for FxGap {
    const ALL: &'static [FxGap] = &[
        FxGap::TrailCompressBasis,
        FxGap::TrailIntersampleCull,
        FxGap::TrailDefLookup,
        FxGap::TrailCodeMesh,
        FxGap::ElemCollideMotion,
        FxGap::ElemMotionLookup,
        FxGap::ElemImpactSpawn,
        FxGap::ElemDeathSpawn,
        FxGap::ElemLightingFrac,
        FxGap::ElemEmitSpawn,
        FxGap::ElemEmitOrientQuat,
        FxGap::ElemEmitRandVariance,
        FxGap::ElemSoundSpawn,
        FxGap::ElemDecalSpawn,
        FxGap::MarkFragments,
        FxGap::ElemRunnerSpawn,
    ];

    fn name(self) -> &'static str {
        match self {
            FxGap::TrailCompressBasis => "trail-compress-basis",
            FxGap::TrailIntersampleCull => "trail-intersample-cull",
            FxGap::TrailDefLookup => "trail-def-lookup",
            FxGap::TrailCodeMesh => "trail-code-mesh",
            FxGap::ElemCollideMotion => "elem-collide-motion",
            FxGap::ElemMotionLookup => "elem-motion-lookup",
            FxGap::ElemImpactSpawn => "elem-impact-spawn",
            FxGap::ElemDeathSpawn => "elem-death-spawn",
            FxGap::ElemLightingFrac => "elem-lighting-frac",
            FxGap::ElemEmitSpawn => "elem-emit-spawn",
            FxGap::ElemEmitOrientQuat => "elem-emit-orient-quat",
            FxGap::ElemEmitRandVariance => "elem-emit-rand-variance",
            FxGap::ElemSoundSpawn => "elem-sound-spawn",
            FxGap::ElemDecalSpawn => "elem-decal-spawn",
            FxGap::MarkFragments => "mark-fragments",
            FxGap::ElemRunnerSpawn => "elem-runner-spawn",
        }
    }

    fn is_standing(self) -> bool {
        false
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChildSpawn {
    Impact,
    Death,
    Emitted,
}

impl fmt::Display for ChildSpawn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ChildSpawn::Impact => "effectOnImpact",
            ChildSpawn::Death => "effectOnDeath",
            ChildSpawn::Emitted => "effectEmitted",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodeMeshStep {
    Bind,

    VertReserve,

    IndexReserve,
}

impl fmt::Display for CodeMeshStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CodeMeshStep::Bind => "material bind",
            CodeMeshStep::VertReserve => "vert reserve",
            CodeMeshStep::IndexReserve => "index reserve",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FxGapCause {
    TrailSampleBasisNotCompressed { def_index: u8 },

    TrailWindowNotCulled { def_index: u8 },

    TrailDefNotFound { def_index: u8 },

    TrailCodeMeshRefused { step: CodeMeshStep },

    NoWorldClipForCollide { def_index: u8 },

    ElemDefNotFound { def_index: u8 },

    ChildSpawnRefused { child: ChildSpawn, def_index: u8 },

    NoLightGridSample { def_index: u8 },

    EmitOrientQuatNotUnpacked { def_index: u8 },

    EmitRandVarianceNotApplied { def_index: u8 },

    ElemSoundSpawnSkipped { def_index: u8 },

    ElemDecalSpawnSkipped { def_index: u8 },

    MarkFragmentsSkipped { def_index: u8 },

    ElemRunnerSpawnSkipped { def_index: u8 },
}

impl ledger::GapCause for FxGapCause {
    type Gap = FxGap;

    fn gap(&self) -> FxGap {
        match self {
            FxGapCause::TrailSampleBasisNotCompressed { .. } => FxGap::TrailCompressBasis,
            FxGapCause::TrailWindowNotCulled { .. } => FxGap::TrailIntersampleCull,
            FxGapCause::TrailDefNotFound { .. } => FxGap::TrailDefLookup,
            FxGapCause::TrailCodeMeshRefused { .. } => FxGap::TrailCodeMesh,
            FxGapCause::NoWorldClipForCollide { .. } => FxGap::ElemCollideMotion,
            FxGapCause::ElemDefNotFound { .. } => FxGap::ElemMotionLookup,
            FxGapCause::ChildSpawnRefused { child, .. } => match child {
                ChildSpawn::Impact => FxGap::ElemImpactSpawn,
                ChildSpawn::Death => FxGap::ElemDeathSpawn,
                ChildSpawn::Emitted => FxGap::ElemEmitSpawn,
            },
            FxGapCause::NoLightGridSample { .. } => FxGap::ElemLightingFrac,
            FxGapCause::EmitOrientQuatNotUnpacked { .. } => FxGap::ElemEmitOrientQuat,
            FxGapCause::EmitRandVarianceNotApplied { .. } => FxGap::ElemEmitRandVariance,
            FxGapCause::ElemSoundSpawnSkipped { .. } => FxGap::ElemSoundSpawn,
            FxGapCause::ElemDecalSpawnSkipped { .. } => FxGap::ElemDecalSpawn,
            FxGapCause::MarkFragmentsSkipped { .. } => FxGap::MarkFragments,
            FxGapCause::ElemRunnerSpawnSkipped { .. } => FxGap::ElemRunnerSpawn,
        }
    }
}

impl fmt::Display for FxGapCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FxGapCause::TrailSampleBasisNotCompressed { def_index } => {
                write!(
                    f,
                    "elem {def_index} appended a trail sample with zero basis"
                )
            }
            FxGapCause::TrailWindowNotCulled { def_index } => {
                write!(f, "elem {def_index} advanced a trail window unculled")
            }
            FxGapCause::TrailDefNotFound { def_index } => {
                write!(f, "elem {def_index} has no trail def in the catalog")
            }
            FxGapCause::TrailCodeMeshRefused { step } => {
                write!(f, "a CPU ribbon was refused at the {step}")
            }
            FxGapCause::NoWorldClipForCollide { def_index } => {
                write!(f, "elem {def_index} collides and no world clip was given")
            }
            FxGapCause::ElemDefNotFound { def_index } => {
                write!(f, "elem {def_index} has no def to evaluate motion from")
            }
            FxGapCause::ChildSpawnRefused { child, def_index } => {
                write!(f, "elem {def_index} could not play its {child} child")
            }
            FxGapCause::NoLightGridSample { def_index } => {
                write!(
                    f,
                    "elem {def_index} authored lightingFrac with no light grid"
                )
            }
            FxGapCause::EmitOrientQuatNotUnpacked { def_index } => {
                write!(f, "elem {def_index} authored an unread orient-quat axis")
            }
            FxGapCause::EmitRandVarianceNotApplied { def_index } => {
                write!(f, "elem {def_index} authored an unapplied spacing variance")
            }
            FxGapCause::ElemSoundSpawnSkipped { def_index } => {
                write!(
                    f,
                    "elem {def_index} is Sound and its Bound alias could not play"
                )
            }
            FxGapCause::ElemDecalSpawnSkipped { def_index } => {
                write!(
                    f,
                    "elem {def_index} SpawnDecal sampled; FX_ImpactMark did not enter"
                )
            }
            FxGapCause::MarkFragmentsSkipped { def_index } => {
                write!(
                    f,
                    "elem {def_index} FX_ImpactMark entered Generate; R_MarkFragments_Go / AllocMark not ported"
                )
            }
            FxGapCause::ElemRunnerSpawnSkipped { def_index } => {
                write!(
                    f,
                    "elem {def_index} is Runner and the child FxEffectDef name is absent"
                )
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct FxGaps(GapLedger<FxGapCause, FX_GAP_COUNT>);

impl FxGaps {
    pub fn raise(&mut self, cause: FxGapCause) {
        self.0.raise(cause);
    }

    pub fn hits(&self, gap: FxGap) -> u64 {
        self.0.hits(gap)
    }

    pub fn is_live(&self, gap: FxGap) -> bool {
        self.0.is_live(gap)
    }

    pub fn cause(&self, gap: FxGap) -> Option<&FxGapCause> {
        self.0.cause(gap)
    }

    pub fn live(&self) -> impl Iterator<Item = FxGap> + '_ {
        self.0.live()
    }

    pub fn count(&self) -> usize {
        self.0.count()
    }

    pub fn report_lines(&self) -> Vec<String> {
        self.0
            .live()
            .filter_map(|gap| {
                self.0
                    .cause(gap)
                    .map(|cause| format!("{}={} ({cause})", gap.name(), self.0.hits(gap)))
            })
            .collect()
    }
}
