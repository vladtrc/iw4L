use crate::script::source::SourceResolver;

mod campaign;

/// Install runs the struct initializer itself, before the map's structs exist.
pub struct Iw4Startup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
    pub required_functions: Vec<String>,
}
impl Iw4Startup {
    pub fn campaign(
        map: &str,
        entities: &[Vec<(String, String)>],
        nodes: &[asset_core::PathNode],
    ) -> Self {
        campaign::startup(map, entities, nodes)
    }

    pub fn validate_hooks(
        &self,
        program: &crate::script::Program,
    ) -> Result<(), crate::script::Fault> {
        for entry in &self.required_functions {
            if !program.names.contains_key(entry) {
                let (module, label) = entry.split_once("::").unwrap_or(("<startup>", entry));
                return Err(crate::script::Fault::at(
                    &crate::script::Location {
                        module: module.into(),
                        function: label.into(),
                        line: 0,
                        column: 0,
                    },
                    format!("required engine script hook {entry} is missing"),
                ));
            }
        }
        Ok(())
    }

    pub fn new(resolver: &impl SourceResolver, gametype: &str, map: &str) -> Self {
        let gametype = format!("maps/mp/gametypes/{gametype}");
        let map = format!("maps/mp/{map}");
        let callbacks = "maps/mp/gametypes/_callbacksetup";
        let mut roots = vec![
            "codescripts/delete".to_owned(),
            "codescripts/struct".to_owned(),
            callbacks.to_owned(),
            gametype.clone(),
        ];
        let mut entries = vec![format!("{gametype}::main")];
        if resolver.read_bytes(&map).is_ok() {
            entries.push(format!("{map}::main"));
            roots.push(map);
        }
        let t6_equipment = "iw4l_t6/equipment";
        if resolver.read_bytes(t6_equipment).is_ok() {
            roots.push(t6_equipment.to_owned());
            entries.push(format!("{t6_equipment}::main"));
        }
        entries.push(format!("{callbacks}::codecallback_startgametype"));
        Self {
            roots,
            entries,
            required_functions: vec![],
        }
    }
}
