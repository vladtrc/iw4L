use std::collections::VecDeque;
use std::fmt;

use bevy::prelude::*;

use assets::AssetNamespace;

const START_DECISION_CAP: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartOutcome {
    Submitted,
    Pending,
    Suppressed(SuppressReason),
    Failed(StartFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuppressReason {
    Inaudible,
    VoiceLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundClass {
    Weapon,
    World,
    Ui,
}

impl SoundClass {
    pub fn oneshot_wait(self) -> std::time::Duration {
        match self {
            Self::Weapon => std::time::Duration::from_millis(200),
            Self::World => std::time::Duration::from_millis(350),
            Self::Ui => std::time::Duration::from_millis(500),
        }
    }

    pub fn scope(self) -> crate::backend::AudioScope {
        match self {
            Self::Ui => crate::backend::AudioScope::Menu,
            Self::Weapon | Self::World => crate::backend::AudioScope::Match,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartFailure {
    BankMissing,
    MissingAlias,
    NoPcm,
    NoListener,
    NoFalloffCurve,
    FalloffEval,
    DecodeFailed,
    Expired,
}

impl StartOutcome {
    pub fn is_terminal_success(&self) -> bool {
        matches!(self, Self::Submitted)
    }

    pub fn allows_binding_fallback(&self) -> bool {
        matches!(
            self,
            Self::Failed(StartFailure::MissingAlias | StartFailure::NoPcm)
        )
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Self::Submitted | Self::Pending | Self::Suppressed(_))
    }
}

impl fmt::Display for StartOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Submitted => f.write_str("Submitted"),
            Self::Pending => f.write_str("Pending"),
            Self::Suppressed(SuppressReason::Inaudible) => f.write_str("SuppressedInaudible"),
            Self::Suppressed(SuppressReason::VoiceLimit) => f.write_str("SuppressedVoiceLimit"),
            Self::Failed(StartFailure::BankMissing) => f.write_str("FailedBankMissing"),
            Self::Failed(StartFailure::MissingAlias) => f.write_str("FailedMissingAlias"),
            Self::Failed(StartFailure::NoPcm) => f.write_str("FailedNoPcm"),
            Self::Failed(StartFailure::NoListener) => f.write_str("FailedNoListener"),
            Self::Failed(StartFailure::NoFalloffCurve) => f.write_str("FailedNoFalloffCurve"),
            Self::Failed(StartFailure::FalloffEval) => f.write_str("FailedFalloffEval"),
            Self::Failed(StartFailure::DecodeFailed) => f.write_str("FailedDecode"),
            Self::Failed(StartFailure::Expired) => f.write_str("ExpiredAwaitingDecode"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StartDecision {
    pub namespace: AssetNamespace,
    pub alias: String,
    pub variant: Option<usize>,
    pub outcome: StartOutcome,
    pub secondary: Option<(String, StartOutcome)>,

    pub detail: Option<String>,
}

impl StartDecision {
    pub fn line(&self) -> String {
        let variant = self
            .variant
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into());
        let mut line = format!(
            "audio: start alias=`{}:{}` variant={variant} result={}",
            self.namespace.as_str(),
            self.alias,
            self.outcome
        );
        if let Some((sec, outcome)) = &self.secondary {
            line.push_str(&format!(" secondary=`{sec}` result={outcome}"));
        }
        if let Some(detail) = &self.detail {
            line.push(' ');
            line.push_str(detail);
        }
        line
    }
}

#[derive(Resource, Default, Debug)]
pub struct StartDecisions {
    entries: VecDeque<StartDecision>,
}

impl StartDecisions {
    pub fn record(&mut self, decision: StartDecision) {
        if self.entries.len() == START_DECISION_CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(decision);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn lines(&self) -> impl Iterator<Item = String> + '_ {
        self.entries.iter().map(StartDecision::line)
    }
}
