#[derive(bevy::prelude::Message, Clone, Debug, PartialEq, Eq)]
pub struct SvcObjective(pub sim::ObjectiveUpdate);
