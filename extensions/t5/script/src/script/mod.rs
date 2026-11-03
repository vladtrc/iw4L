pub mod destructible;
mod runtime;

pub fn script_registration() -> sim::script::families::ScriptFamily {
    runtime::registration()
}
