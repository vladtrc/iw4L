//! MW3 map scripts ship as bytecode this VM does not run. A stand-in source
//! is written from the zone's string literals and entity string.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

#[derive(Clone, Debug, Default)]
pub struct Iw5MapDeclarations {
    compass: Option<String>,
    teams: Option<(String, String)>,
    preanims: BTreeSet<String>,
    effects: BTreeMap<String, String>,
    props: Vec<(String, String)>,
}

impl Iw5MapDeclarations {
    pub fn capture(&mut self, name: &str, compressed_stack: &[u8]) {
        let Ok(stack) = asset_transport::inflate_zlib(compressed_stack) else {
            return;
        };
        let literals = stack_literals(&stack);
        if literals.iter().any(|l| l.starts_with("compass_map_")) {
            self.capture_map_main(&literals);
        } else if let Some(prop) = prop_script(&literals) {
            self.props.push(prop);
        } else if !name.contains('/') {
            self.capture_destructible_anims(&literals);
        }
    }

    fn capture_map_main(&mut self, literals: &[String]) {
        if self.compass.is_none() {
            self.compass = literals
                .iter()
                .find(|l| l.starts_with("compass_map_"))
                .cloned();
        }
        let team_before = |key: &str| {
            let at = literals.iter().position(|l| l == key)?;
            let team = literals.get(at.checked_sub(1)?)?;
            matches!(team.as_str(), "allies" | "axis").then(|| team.clone())
        };
        let other = |team: &str| if team == "axis" { "allies" } else { "axis" }.to_owned();
        let teams = match (team_before("attackers"), team_before("defenders")) {
            (Some(a), Some(d)) => (a, d),
            (Some(a), None) => {
                let d = other(&a);
                (a, d)
            }
            (None, Some(d)) => (other(&d), d),
            (None, None) => ("allies".to_owned(), "axis".to_owned()),
        };
        self.teams.get_or_insert(teams);
    }

    fn capture_destructible_anims(&mut self, literals: &[String]) {
        if literals.len() < 2 || literals.len() % 2 != 0 {
            return;
        }
        let pairs: Vec<(&String, &String)> = literals
            .chunks(2)
            .map(|pair| (&pair[0], &pair[1]))
            .collect();
        let anim = |(a, b): &(&String, &String)| a == b && !a.contains('/');
        let effect = |(a, b): &(&String, &String)| a.contains('/') && !b.contains('/');
        if !pairs.iter().any(anim) || !pairs.iter().all(|p| anim(p) || effect(p)) {
            return;
        }
        for (a, b) in pairs {
            if a == b {
                self.preanims.insert(a.clone());
            } else {
                self.effects.insert(b.clone(), a.clone());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.compass.is_none() && self.preanims.is_empty() && self.props.is_empty()
    }

    pub fn map_script(&self, entities: &str) -> String {
        let mut out = String::from("#using_animtree( \"destructibles\" );\n\nmain()\n{\n");
        if !self.preanims.is_empty() {
            out.push_str("\tif ( !isdefined( level._destructible_preanims ) )\n");
            out.push_str("\t\tlevel._destructible_preanims = [];\n");
        }
        for name in &self.preanims {
            let _ = writeln!(
                out,
                "\tlevel._destructible_preanims[ \"{name}\" ] = %{name};"
            );
        }
        if !self.effects.is_empty() {
            out.push_str("\tif ( !isdefined( level._effect ) )\n\t\tlevel._effect = [];\n");
        }
        for (key, path) in &self.effects {
            let _ = writeln!(out, "\tlevel._effect[ \"{key}\" ] = loadfx( \"{path}\" );");
        }
        let models = self.prop_models(entities);
        if !models.is_empty() {
            out.push_str("\tif ( !isdefined( level.anim_prop_models ) )\n");
            out.push_str("\t\tlevel.anim_prop_models = [];\n");
        }
        for (model, (anim, key)) in &models {
            let _ = writeln!(
                out,
                "\tlevel.anim_prop_models[ \"{model}\" ][ \"{key}\" ] = \"{anim}\";"
            );
        }
        out.push_str("\n\tmaps\\mp\\_load::main();\n");
        if let Some(compass) = &self.compass {
            let _ = writeln!(out, "\tmaps\\mp\\_compass::setupMiniMap( \"{compass}\" );");
        }
        let (attackers, defenders) = self
            .teams
            .clone()
            .unwrap_or_else(|| ("allies".to_owned(), "axis".to_owned()));
        let _ = writeln!(out, "\n\tgame[ \"attackers\" ] = \"{attackers}\";");
        let _ = writeln!(out, "\tgame[ \"defenders\" ] = \"{defenders}\";");
        out.push_str("}\n");
        out
    }

    // A prop names its animation, not its module. Match by that name first; leftover
    // modules pair in zone order, then fall back to the animation they are named after.
    fn prop_models(&self, entities: &str) -> BTreeMap<String, (String, String)> {
        let mut modules: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for block in entity_blocks(entities) {
            if block.get("targetname").map(String::as_str) != Some("animated_model")
                || block.contains_key("animation")
            {
                continue;
            }
            let (Some(model), Some(module)) = (block.get("model"), block.get(PRECACHE_KEY)) else {
                continue;
            };
            let Some(module) = module.strip_prefix("maps animated_models ") else {
                continue;
            };
            modules
                .entry(module.to_owned())
                .or_default()
                .insert(model.clone());
        }
        let mut used = vec![false; self.props.len()];
        let mut chosen: BTreeMap<&str, (String, String)> = BTreeMap::new();
        for module in modules.keys() {
            if let Some(at) = self.props.iter().position(|(anim, _)| anim == module) {
                used[at] = true;
                chosen.insert(module, self.props[at].clone());
            }
        }
        let rest_modules: Vec<&String> = modules
            .keys()
            .filter(|m| !chosen.contains_key(m.as_str()))
            .collect();
        let rest_props: Vec<&(String, String)> = self
            .props
            .iter()
            .zip(&used)
            .filter(|(_, used)| !**used)
            .map(|(prop, _)| prop)
            .collect();
        for (at, module) in rest_modules.iter().enumerate() {
            let prop = if rest_modules.len() == rest_props.len() {
                rest_props[at].clone()
            } else {
                ((*module).clone(), "default".to_owned())
            };
            chosen.insert(module, prop);
        }
        let mut models = BTreeMap::new();
        for (module, names) in &modules {
            for model in names {
                models.insert(model.clone(), chosen[module.as_str()].clone());
            }
        }
        models
    }
}

/// This precache key's script string has no name in the zone.
const PRECACHE_KEY: &str = "2818";

/// `[mp_, animated_props,] animation, key, animation, key`: the two assignments of an
/// animated prop script, one for each of SP and MP.
fn prop_script(literals: &[String]) -> Option<(String, String)> {
    let body = match literals {
        [mp, tree, body @ ..] if mp == "mp_" && tree == "animated_props" => body,
        body => body,
    };
    match body {
        [a, k, a2, k2] if a == a2 && k == k2 && a != k && !a.contains('/') => {
            Some((a.clone(), k.clone()))
        }
        _ => None,
    }
}

/// The NUL-terminated printable runs of an inflated script stack. Single bytes are
/// dropped: sizes in the stack can read as one printable byte.
pub fn stack_literals(stack: &[u8]) -> Vec<String> {
    stack
        .split(|&b| b == 0)
        .filter(|run| run.len() > 1 && run.iter().all(|&b| (0x20..0x7f).contains(&b)))
        .map(|run| String::from_utf8_lossy(run).into_owned())
        .collect()
}

fn entity_blocks(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut blocks = Vec::new();
    let mut current: Option<BTreeMap<String, String>> = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "{" {
            current = Some(BTreeMap::new());
        } else if line == "}" {
            blocks.extend(current.take());
        } else if let Some(block) = current.as_mut() {
            let mut quoted = line.split('"').skip(1).step_by(2);
            if let (Some(key), Some(value)) = (quoted.next(), quoted.next()) {
                block.insert(key.to_owned(), value.to_owned());
            }
        }
    }
    blocks
}
