use std::sync::{Arc, RwLock};

use assets::{FamilySlot, LoadoutRules, PreparedWeapons, WeaponSelection};
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
                .usage("give <game:weapon> [attachment...] — equip a weapon on the active slot (e.g. give iw5:acr acog)")
                .arg(LiveListCompleter(Arc::clone(&completions.give))),
        );
    }
    if registry.resolve("attach").is_none() {
        registry.register(
            crate::CommandSpec::new("attach")
                .usage("attach [name] — show the current set and choices, or toggle a named attachment")
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
                        "usage: give <weapon> [attachment...] — equip a catalog weapon on the \
                         active slot"
                            .into(),
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
                match resolve_give_id(&weapons.0, arg, &cmd.args[1..]) {
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
                                weapons.0.configuration_label(weapon)
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
                let Some(name) = cmd.args.first() else {
                    match attachment_hints(&weapons.0, current) {
                        Ok(hints) => echo(format!("attach: {hints}"), &mut console, &mut line),
                        Err(reason) => echo(format!("attach: {reason}"), &mut console, &mut line),
                    }
                    continue;
                };
                let next = toggle_named_attachment(&weapons.0, current, name);
                let next = match next {
                    Ok(id) => id,
                    Err(msg) => {
                        echo(format!("attach: {msg}"), &mut console, &mut line);
                        continue;
                    }
                };
                if next == current {
                    echo(
                        format!(
                            "attach: `{}` is already the selected configuration — refused \
                             (an unchanged id is not a successful attach)",
                            weapons.0.configuration_label(current)
                        ),
                        &mut console,
                        &mut line,
                    );
                    continue;
                }
                let request_id = seq.allocate();
                if let Err(error) = inbox.push(
                    local.0,
                    ClientAction::ChangeWeaponConfiguration {
                        request_id,
                        from: current,
                        to: next,
                    },
                ) {
                    echo(format!("attach: {error}"), &mut console, &mut line);
                    continue;
                }
                echo(
                    format!(
                        "attach: queued {} id={next} request_id={request_id}",
                        weapons.0.configuration_label(next)
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
        .map(|w| w.0.configuration_label(weapon))
        .filter(|label| !label.is_empty())
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

pub(crate) fn echo_configuration_change_results(
    changes: Option<Res<net::DumpConfigurationChangeLog>>,
    weapons: Option<Res<PreparedWeapons>>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut echoed: Local<Option<u32>>,
) {
    let Some(changes) = changes.as_ref() else {
        return;
    };
    let (Some(request_id), Some(to), Some(accepted)) =
        (changes.request_id, changes.to, changes.accepted)
    else {
        return;
    };
    if *echoed == Some(request_id) {
        return;
    }
    *echoed = Some(request_id);
    let key = weapons
        .as_ref()
        .map(|w| w.0.configuration_label(to))
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| format!("#{to}"));
    let msg = if accepted {
        format!("attach: ok {key} id={to} request_id={request_id}")
    } else {
        format!(
            "attach: rejected {key} id={to} request_id={request_id} ({})",
            changes.reject_reason.unwrap_or("unknown")
        )
    };
    diag::info!(Console, "{msg}");
    line.0 = msg.clone();
    console.echo(msg, settings.log_capacity);
}

fn alive(presented: &PresentedSnapshot, id: sim::ClientId) -> bool {
    presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
}

pub(crate) fn resolve_give_id(
    registry: &assets::WeaponRegistry,
    raw: &str,
    attachments: &[String],
) -> Result<u32, String> {
    let resolve = |selection: WeaponSelection| {
        registry
            .resolve_configuration(&selection, LoadoutRules::default())
            .map(|resolved| resolved.id)
            .map_err(|refusal| format!("`{raw}`: {refusal} ({})", refusal.code()))
    };
    if let Some(key) = assets::FamilyKey::parse(raw)
        && let Some(family) = registry.weapon_families().find(&key)
    {
        return resolve(WeaponSelection::with(family.key.clone(), attachments));
    }
    let id = match registry.resolve_index(raw) {
        Ok(Some(id)) => id,
        Ok(None) => return Err("empty weapon name".into()),
        Err(_) => {
            let with_mp = if raw.ends_with("_mp") {
                raw.to_owned()
            } else {
                format!("{raw}_mp")
            };
            match registry.resolve_index(&with_mp) {
                Ok(Some(id)) => id,
                Ok(None) | Err(_) => return Err(format!("unknown weapon `{raw}`")),
            }
        }
    };
    if attachments.is_empty() {
        return registry
            .configuration_admission(id)
            .map(|()| id)
            .map_err(|refusal| format!("`{raw}`: {refusal} ({})", refusal.code()));
    }
    let Some(described) = registry.describe_configuration(id) else {
        return Err(format!("`{raw}` belongs to no weapon family"));
    };
    let mut selection = described.clone();
    selection.attachments.extend(attachments.iter().cloned());
    resolve(selection)
}

fn family_of(registry: &assets::WeaponRegistry, weapon: u32) -> Result<WeaponSelection, String> {
    registry
        .describe_configuration(weapon)
        .filter(|selection| selection.family.is_some())
        .cloned()
        .ok_or_else(|| format!("`{}` belongs to no weapon family", registry.name_of(weapon)))
}

fn attachment_hints(registry: &assets::WeaponRegistry, current: u32) -> Result<String, String> {
    let selection = family_of(registry, current)?;
    let options = registry
        .list_attachment_choices(&selection, LoadoutRules::default())
        .map_err(|refusal| format!("{refusal} ({})", refusal.code()))?;
    let selected = if selection.attachments.is_empty() {
        "none".to_owned()
    } else {
        selection.attachments.join(", ")
    };
    let actions = if options.is_empty() {
        "no authored attachment choices".to_owned()
    } else {
        options
            .iter()
            .map(|option| {
                let action = if option.selected { "remove" } else { "add" };
                match &option.toggle {
                    Ok(_) => format!("{} ({action})", option.choice.name),
                    Err(reason) => format!(
                        "{} ({action} unavailable: {reason}; {})",
                        option.choice.name,
                        reason.code()
                    ),
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok(format!(
        "{}; selected: {selected}; choices: {actions}",
        registry.configuration_label(current)
    ))
}

fn toggle_named_attachment(
    registry: &assets::WeaponRegistry,
    current: u32,
    name: &str,
) -> Result<u32, String> {
    let selection = family_of(registry, current)?;
    let needle = name.to_ascii_lowercase();
    let options = registry
        .list_attachment_choices(&selection, LoadoutRules::default())
        .map_err(|refusal| format!("{refusal} ({})", refusal.code()))?;
    let Some(option) = options.iter().find(|option| option.choice.name == needle) else {
        return Err(format!("attachment `{name}` is not offered on this weapon"));
    };
    option
        .toggle
        .clone()
        .map_err(|refusal| format!("{refusal} ({})", refusal.code()))
}

pub fn weapon_completions(weapons: &PreparedWeapons) -> Vec<String> {
    let mut names: Vec<String> = weapons
        .0
        .weapon_families()
        .offered()
        .filter(|family| matches!(family.slot, FamilySlot::Primary | FamilySlot::Secondary))
        .filter(|family| {
            family
                .base
                .is_some_and(|id| weapons.0.gun_xmodel_of(id).is_some())
        })
        .map(|family| family.key.short())
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn attach_completions(weapons: &PreparedWeapons, current_weapon: u32) -> Vec<String> {
    let Ok(selection) = family_of(&weapons.0, current_weapon) else {
        return Vec::new();
    };
    weapons
        .0
        .list_attachment_choices(&selection, LoadoutRules::default())
        .map(|options| {
            options
                .into_iter()
                .map(|option| option.choice.name)
                .collect()
        })
        .unwrap_or_default()
}
