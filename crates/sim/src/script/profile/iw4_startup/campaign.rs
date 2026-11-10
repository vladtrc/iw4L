use super::Iw4Startup;

pub(super) const HUMAN: &[&str] = &[
    "combat",
    "cover_arrival",
    "cover_crouch",
    "cover_left",
    "cover_prone",
    "cover_right",
    "cover_stand",
    "death",
    "grenade_return_throw",
    "init",
    "pain",
    "move",
    "scripted",
    "stop",
    "grenade_cower",
    "flashed",
    "reactions",
];
const DOG: &[&str] = &[
    "dog_combat",
    "dog_death",
    "dog_init",
    "dog_pain",
    "dog_move",
    "dog_scripted",
    "dog_stop",
    "dog_flashed",
    "dog_reactions",
];
const CIVILIAN: &[&str] = &[
    "civilian_combat",
    "civilian_cover_arrival",
    "civilian_cover_crouch",
    "civilian_cover_prone",
    "civilian_cover_stand",
    "civilian_cover_left",
    "civilian_cover_right",
    "civilian_death",
    "civilian_grenade_response",
    "civilian_init",
    "civilian_pain",
    "civilian_move",
    "civilian_scripted",
    "civilian_stop",
    "civilian_flashed",
    "civilian_reactions",
];

fn require(startup: &mut Iw4Startup, module: String, labels: &[&str]) {
    if !startup.roots.contains(&module) {
        startup.roots.push(module.clone());
    }
    for label in labels {
        let entry = format!("{module}::{label}");
        if !startup.required_functions.contains(&entry) {
            startup.required_functions.push(entry);
        }
    }
}

pub(super) fn startup(
    map: &str,
    entities: &[Vec<(String, String)>],
    nodes: &[asset_core::PathNode],
) -> Iw4Startup {
    let map = format!("maps/{map}");
    let mut startup = Iw4Startup {
        roots: vec![],
        entries: vec![format!("{map}::main")],
        required_functions: vec![],
    };
    require(&mut startup, "codescripts/delete".into(), &["main"]);
    require(
        &mut startup,
        "codescripts/struct".into(),
        &["initstructs", "createstruct"],
    );
    for name in HUMAN {
        require(&mut startup, format!("animscripts/{name}"), &["main"]);
    }
    require(&mut startup, "animscripts/scripted".into(), &["init"]);
    require(&mut startup, map, &["main"]);
    for pairs in entities {
        let Some(classname) = pairs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("classname"))
            .map(|(_, v)| v)
        else {
            continue;
        };
        let Some(module) = crate::script::host::spawners::aitype_module(classname) else {
            continue;
        };
        if module.contains("dog") {
            for name in DOG {
                require(&mut startup, format!("animscripts/dog/{name}"), &["main"]);
            }
        }
        if module.contains("civilian_") {
            for name in CIVILIAN {
                require(
                    &mut startup,
                    format!("animscripts/civilian/{name}"),
                    &["main"],
                );
            }
        }
        require(&mut startup, module, &["main", "precache", "spawner"]);
    }
    for node in nodes.iter().filter(|node| node.kind == 16) {
        if let Some(name) = node.animscript.as_ref().filter(|name| !name.is_empty()) {
            let name = name.replace('\\', "/").to_ascii_lowercase();
            require(
                &mut startup,
                format!("animscripts/traverse/{name}"),
                &["main"],
            );
        }
    }
    startup
}
