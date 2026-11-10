mod bots;
mod run;
mod runtime;

pub use assets::{SceneEndpoint, ScenePlayerclipOpening, ScenePortal, SceneRecipe, SceneSpace};
pub use run::{
    HeadlessSession, InputContext, InputFrame, InputSource, RunError, RunLimits, RunRecord,
    RunRequest, RunSummary, run,
};
pub use runtime::{Runtime, RuntimeConfig};
pub use scenario::{read, write};
pub use session::{ClassRow, SimulationOptions};
pub use sim::{ClientAction, ClientId, PlayerCommand, Tick, TickInput};

pub use bots::BotInputs;
pub use scenario::author::{Actor, Bindings, Program, Scene, Tape};

pub fn read_recipe(path: &std::path::Path) -> Result<SceneRecipe, String> {
    let value = scenario::read(path).map_err(|error| error.to_string())?;
    SceneRecipe::parse(&value)
}

pub use session::loadout::resolve_personal_class;
pub use sim::{AccountId, ClassId, PersonalClass};
