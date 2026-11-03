pub mod destructible;
mod script;

pub fn script_registration() -> sim::script::families::ScriptFamily {
    script::registration()
}
