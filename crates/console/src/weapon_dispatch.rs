use std::sync::{Arc, RwLock};

use assets::{LoadoutCatalogKind, PreparedWeapons};
use bevy::prelude::*;
use frame::MatchTornDown;
use net::{ClientActionInbox, LocalPresentClient, PresentedSnapshot};
use sim::{ClientAction, ClientLifecycle};

use crate::{
    ArgCompleter, ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState,
    StaticCompleter,
};

#[derive(Resource, Clone)]
pub struct WeaponArgCompletions {
    pub give: Arc<RwLock<Vec<String>>>,
    pub attach: Arc<RwLock<Vec<String>>>,
}

impl Default for WeaponArgCompletions {
    fn default() -> Self {
        Self {
            give: Arc::new(RwLock::new(Vec::new())),
            attach: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

pub(crate) fn clear_weapon_args_on_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    completions: Res<WeaponArgCompletions>,
) {
    if torn.read().len() == 0 {
        return;
    }
    if let Ok(mut give) = completions.give.write() {
        give.clear();
    }
    if let Ok(mut attach) = completions.attach.write() {
        attach.clear();
    }
}

struct LiveListCompleter(Arc<RwLock<Vec<String>>>);

impl ArgCompleter for LiveListCompleter {
    fn complete(&self, prefix: &str) -> Vec<String> {
        let Ok(values) = self.0.read() else {
            return Vec::new();
        };
        StaticCompleter::new(values.iter().cloned()).complete(prefix)
    }
}

pub(crate) fn register_weapon_commands(
    registry: &mut ConsoleRegistry,
    completions: &WeaponArgCompletions,
) {
    if registry.resolve("give").is_none() {
        registry.register(
            crate::CommandSpec::new("give")
                .usage("give <weapon> — equip a catalog weapon on the active slot")
                .arg(LiveListCompleter(Arc::clone(&completions.give))),
        );
    }
    if registry.resolve("attach").is_none() {
        registry.register(
            crate::CommandSpec::new("attach")
                .usage("attach [name] — cycle attachment variants, or toggle a named one")
                .arg(LiveListCompleter(Arc::clone(&completions.attach))),
        );
    }
}

pub(crate) fn refresh_weapon_arg_completions(
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    completions: Res<WeaponArgCompletions>,
    mut last_held: Local<Option<u32>>,
) {
    let Some(weapons) = weapons.as_ref() else {
        return;
    };
    let held = presented.held_weapon_id(local.0).unwrap_or(0);
    let changed = weapons.is_changed();
    if !should_refresh_weapon_args(changed, *last_held, held) {
        return;
    }
    *last_held = Some(held);
    if let Ok(mut give) = completions.give.write() {
        *give = weapon_completions(weapons);
    }
    if let Ok(mut attach) = completions.attach.write() {
        *attach = if held == 0 {
            Vec::new()
        } else {
            attach_completions(weapons, held)
        };
    }
}

fn should_refresh_weapon_args(catalog_changed: bool, last_held: Option<u32>, held: u32) -> bool {
    catalog_changed || last_held != Some(held)
}

pub(crate) fn route_weapon_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    weapons: Option<Res<PreparedWeapons>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut inbox: ResMut<ClientActionInbox>,
    mut seq: ResMut<net::ActionRequestIds>,
) {
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "give" => {
                let Some(arg) = cmd.args.first() else {
                    echo(
                        "usage: give <weapon> — equip a catalog weapon on the active slot".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let Some(weapons) = weapons.as_ref() else {
                    echo(
                        "give: weapon catalog not loaded".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                if !alive(&presented, local.0) {
                    echo(
                        "give: not Alive — spawn a class first".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                match resolve_give_id(&weapons.0, arg) {
                    Ok(weapon) => {
                        let request_id = seq.allocate();
                        if let Err(error) =
                            inbox.push(local.0, ClientAction::GiveWeapon { request_id, weapon })
                        {
                            echo(format!("give: {error}"), &mut console, &mut line);
                            continue;
                        }

                        echo(
                            format!(
                                "give: queued {} id={weapon} request_id={request_id}",
                                weapons
                                    .0
                                    .namespaced_key_of(weapon)
                                    .unwrap_or_else(|| weapons.0.name_of(weapon).to_owned())
                            ),
                            &mut console,
                            &mut line,
                        );
                    }
                    Err(msg) => echo(format!("give: {msg}"), &mut console, &mut line),
                }
            }
            "attach" => {
                let Some(weapons) = weapons.as_ref() else {
                    echo(
                        "attach: weapon catalog not loaded".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let Some(ps) = presented.alive_player(local.0) else {
                    echo(
                        "attach: not Alive — spawn a class first".into(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                };
                let current = ps.weapon;
                if current == 0 {
                    echo("attach: no weapon in hands".into(), &mut console, &mut line);
                    continue;
                }
                let variants = attachment_variants_for(&weapons.0, current);

                if variants.len() < 2 {
                    echo(
                        format!(
                            "attach: `{}` has no pre-linked attachment variants — unsupported \
                             (IW5 WeaponAttachment assembly is not implemented)",
                            weapons.0.name_of(current)
                        ),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                let next = match cmd.args.first() {
                    None => cycle_attachment(current, &variants),
                    Some(name) => {
                        match toggle_named_attachment(&weapons.0, current, name, &variants) {
                            Ok(id) => id,
                            Err(msg) => {
                                echo(format!("attach: {msg}"), &mut console, &mut line);
                                continue;
                            }
                        }
                    }
                };
                if !weapons.0.configuration_supported(next) {
                    echo(
                        "attach: unsupported T5 dual-hand animation configuration".to_owned(),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                if next == current {
                    echo(
                        format!(
                            "attach: `{}` is already the selected configuration — refused \
                             (an unchanged id is not a successful attach)",
                            weapons.0.name_of(current)
                        ),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                let request_id = seq.allocate();
                if let Err(error) = inbox.push(
                    local.0,
                    ClientAction::GiveWeapon {
                        request_id,
                        weapon: next,
                    },
                ) {
                    echo(format!("attach: {error}"), &mut console, &mut line);
                    continue;
                }
                echo(
                    format!(
                        "attach: queued {} id={next} request_id={request_id}",
                        weapons.0.name_of(next)
                    ),
                    &mut console,
                    &mut line,
                );
            }
            _ => {}
        }
    }
}

pub(crate) fn echo_give_results(
    give: Option<Res<net::DumpGiveLog>>,
    weapons: Option<Res<PreparedWeapons>>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut echoed: Local<Option<(u32, u8)>>,
) {
    let Some(give) = give.as_ref() else {
        return;
    };
    let (Some(request_id), Some(weapon), Some(accepted)) =
        (give.request_id, give.weapon, give.accepted)
    else {
        return;
    };
    if *echoed == Some((request_id, accepted)) {
        return;
    }
    *echoed = Some((request_id, accepted));
    let key = weapons
        .as_ref()
        .and_then(|w| {
            w.0.namespaced_key_of(weapon).or_else(|| {
                let n = w.0.name_of(weapon);
                (!n.is_empty()).then(|| n.to_owned())
            })
        })
        .unwrap_or_else(|| format!("#{weapon}"));
    let capacity = settings.log_capacity;
    let msg = if accepted == 1 {
        format!("give: ok {key} id={weapon} request_id={request_id}")
    } else {
        format!(
            "give: rejected {key} id={weapon} request_id={request_id} ({})",
            give.reject_reason.unwrap_or("unknown")
        )
    };
    diag::info!(Console, "{msg}");
    line.0 = msg.clone();
    console.echo(msg, capacity);
}

fn alive(presented: &PresentedSnapshot, id: sim::ClientId) -> bool {
    presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
}

pub(crate) fn resolve_give_id(registry: &assets::WeaponRegistry, raw: &str) -> Result<u32, String> {
    match registry.resolve_index(raw) {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err("empty weapon name".into()),
        Err(_) => {
            let with_mp = if raw.ends_with("_mp") {
                raw.to_owned()
            } else {
                format!("{raw}_mp")
            };
            match registry.resolve_index(&with_mp) {
                Ok(Some(id)) => Ok(id),
                Ok(None) | Err(_) => Err(format!("unknown weapon `{raw}`")),
            }
        }
    }
}

fn attachment_variants_for(registry: &assets::WeaponRegistry, weapon: u32) -> Vec<u32> {
    let catalog = registry.loadout_catalog();
    let base = catalog
        .iter()
        .find(|row| row.id == weapon)
        .map(|row| match row.kind {
            LoadoutCatalogKind::AttachmentVariant { base_id } => base_id,
            _ => weapon,
        })
        .unwrap_or(weapon);
    let mut ids = vec![base];
    for row in &catalog {
        if let LoadoutCatalogKind::AttachmentVariant { base_id } = row.kind
            && base_id == base
            && !ids.contains(&row.id)
        {
            ids.push(row.id);
        }
    }
    ids
}

fn cycle_attachment(current: u32, variants: &[u32]) -> u32 {
    let idx = variants.iter().position(|&id| id == current).unwrap_or(0);
    variants[(idx + 1) % variants.len()]
}

fn toggle_named_attachment(
    registry: &assets::WeaponRegistry,
    current: u32,
    name: &str,
    variants: &[u32],
) -> Result<u32, String> {
    let needle = name.to_ascii_lowercase();
    let Some(&target) = variants.iter().find(|&&id| {
        let n = registry.name_of(id).to_ascii_lowercase();
        n == needle
            || n.trim_end_matches("_mp") == needle.trim_end_matches("_mp")
            || attachment_token(&n).is_some_and(|t| t == needle)
    }) else {
        return Err(format!(
            "attachment `{name}` is not available on this weapon"
        ));
    };
    let base = variants[0];
    if current == target {
        Ok(base)
    } else {
        Ok(target)
    }
}

fn attachment_token(name: &str) -> Option<&str> {
    let stem = name.trim_end_matches("_mp");
    stem.rsplit_once('_').map(|(_, token)| token)
}

pub fn weapon_completions(weapons: &PreparedWeapons) -> Vec<String> {
    let mut names: Vec<String> = weapons
        .0
        .loadout_catalog()
        .into_iter()
        .filter(|row| {
            matches!(
                row.kind,
                LoadoutCatalogKind::Primary | LoadoutCatalogKind::Secondary
            )
        })
        .filter(|row| weapons.0.gun_xmodel_of(row.id).is_some())
        .map(|row| suggest_key(&row.key))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn suggest_key(key: &assets::AssetKey) -> String {
    let name = key.logical_name();
    let stem = name.strip_suffix("_mp").unwrap_or(name);
    let prefix = format!("{}_", key.namespace.as_str());
    let stem = stem.strip_prefix(prefix.as_str()).unwrap_or(stem);
    format!("{}:weapon/{stem}", key.namespace.as_str())
}

pub fn attach_completions(weapons: &PreparedWeapons, current_weapon: u32) -> Vec<String> {
    attachment_variants_for(&weapons.0, current_weapon)
        .into_iter()
        .skip(1)
        .map(|id| {
            let name = weapons.0.name_of(id);
            attachment_token(name)
                .unwrap_or(name.trim_end_matches("_mp"))
                .to_owned()
        })
        .collect()
}
