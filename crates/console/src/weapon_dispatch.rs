use std::sync::{Arc, RwLock};

use asset_game::{LoadoutRules, WeaponSelection};
use assets::PreparedWeapons;
use bevy::prelude::*;
use net::{ClientActionInbox, LocalPresentClient, PresentedSnapshot};
use sim::{ClientAction, ClientLifecycle};

use crate::{ArgCompleter, ConsoleCommand, ConsoleRegistry, StaticCompleter};

#[derive(Clone, Default)]
pub struct WeaponArgCompletions {
    pub give: Vec<String>,
    pub attach: Vec<String>,
    pub camos: Vec<String>,
    pub killstreaks: Vec<String>,
    pub bots: Vec<String>,
}

#[derive(Resource, Clone, Default)]
pub(crate) struct CompletionSlot(Arc<RwLock<Option<WeaponArgCompletions>>>);

impl CompletionSlot {
    fn read<R>(&self, read: impl FnOnce(Option<&WeaponArgCompletions>) -> R) -> R {
        read(self.0.read().expect("completion slot poisoned").as_ref())
    }

    fn publish(&self, value: Option<WeaponArgCompletions>) {
        *self.0.write().expect("completion slot poisoned") = value;
    }
}

pub(crate) fn clear_weapon_arg_completions(completions: Res<CompletionSlot>) {
    completions.publish(None);
}

struct LiveListCompleter(CompletionSlot, fn(&WeaponArgCompletions) -> &[String]);

impl ArgCompleter for LiveListCompleter {
    fn complete(&self, prefix: &str) -> Vec<String> {
        self.0.read(|value| {
            value.map_or_else(Vec::new, |value| {
                StaticCompleter::new((self.1)(value).iter().cloned()).complete(prefix)
            })
        })
    }
}

pub(crate) fn register_weapon_commands(
    registry: &mut ConsoleRegistry,
    completions: &CompletionSlot,
) {
    if registry.resolve("give").is_none() {
        registry.register(
            crate::CommandSpec::new("give")
                .usage(GIVE_USAGE)
                .arg(GiveCompleter(completions.clone()))
                .arg(GiveCompleter(completions.clone())),
        );
    }
    if registry.resolve("bot").is_none() {
        let mut spec = crate::CommandSpec::new("bot").usage(super::feature_dispatch::BOT_USAGE);
        for _ in 0..4 {
            spec = spec.arg(BotCompleter(completions.clone()));
        }
        registry.register(spec);
    }
    if registry.resolve("attach").is_none() {
        registry.register(
            crate::CommandSpec::new("attach")
                .usage("attach [name] — show the current set and choices, or toggle a named attachment")
                .arg(LiveListCompleter(completions.clone(), |value| &value.attach)),
        );
    }
}

#[derive(Resource, Default)]
pub(crate) struct WeaponCompletionState(Option<(u32, Vec<String>)>);

pub(crate) fn refresh_weapon_arg_completions(
    weapons: Res<PreparedWeapons>,
    killstreaks: Option<Res<assets::prepared::PreparedKillstreaks>>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    completions: Res<CompletionSlot>,
    mut previous: ResMut<WeaponCompletionState>,
) {
    let held = presented.held_weapon_id(local.0).unwrap_or(0);
    let bots = presented.snapshot().map_or_else(Vec::new, |snapshot| {
        snapshot
            .meta
            .clients
            .iter()
            .filter(|(_, meta)| {
                ["bot", "dummy"]
                    .iter()
                    .any(|name| meta.name == entity_iw4::pack_client_state_name(name))
            })
            .map(|(id, _)| id.0.to_string())
            .collect()
    });
    if previous
        .0
        .as_ref()
        .is_some_and(|(old_held, old_bots)| *old_held == held && *old_bots == bots)
        && !weapons.is_changed()
        && killstreaks.as_ref().is_none_or(|value| !value.is_changed())
    {
        return;
    }
    previous.0 = Some((held, bots.clone()));
    let camos = if held == 0 {
        Vec::new()
    } else {
        std::iter::once("none".to_owned())
            .chain(
                weapons
                    .registry()
                    .camouflage_choices(held)
                    .into_iter()
                    .map(|(_, name)| name.to_owned()),
            )
            .collect()
    };
    completions.publish(Some(WeaponArgCompletions {
        give: weapon_completions(&weapons),
        attach: if held == 0 {
            Vec::new()
        } else {
            attach_completions(&weapons, held)
        },
        camos,
        killstreaks: killstreaks.map_or_else(Vec::new, |value| value.0.clone()),
        bots,
    }));
}

pub(crate) fn route_weapon_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: crate::feature_dispatch::ConsoleEcho,
    weapons: Option<Res<PreparedWeapons>>,
    player: net::LocalPresentation,
    mut inbox: ResMut<ClientActionInbox>,
    completions: Res<CompletionSlot>,
    authority: Option<Res<net::AuthorityWorld>>,
) {
    let net::LocalPresentation { presented, local } = player;
    let mut echo = |msg: String| output.write(msg);

    for cmd in events.read() {
        if matches!(cmd.name.as_str(), "give" | "attach")
            && authority.as_ref().is_some_and(|a| !a.0.cheats_enabled())
        {
            echo(format!("{}: cheats are off", cmd.name));
            continue;
        }
        match cmd.name.as_str() {
            "give" => {
                let target = match parse_give_target(&cmd.args) {
                    Ok(target) => target,
                    Err(reason) => {
                        echo(reason.into());
                        continue;
                    }
                };
                let arg = match target {
                    GiveTarget::Ammo => {
                        resupply_ammo(&presented, &local, &mut inbox, &mut echo);
                        continue;
                    }
                    GiveTarget::Killstreak(name) => {
                        grant_killstreak(
                            name,
                            &completions,
                            &presented,
                            &local,
                            &mut inbox,
                            &mut echo,
                        );
                        continue;
                    }
                    GiveTarget::Camo(name) => {
                        change_camo(
                            name,
                            weapons.as_deref(),
                            &presented,
                            &local,
                            &mut inbox,
                            &mut echo,
                        );
                        continue;
                    }
                    GiveTarget::Weapon(name) => name,
                };
                let Some(weapons) = weapons.as_ref() else {
                    echo("give: weapon catalog not loaded".into());
                    continue;
                };
                if !alive(&presented, local.0) {
                    echo("give: not Alive — spawn a class first".into());
                    continue;
                }
                let (camo, attachments) = split_camo(&cmd.args[1..]);
                match resolve_give_id(weapons.registry(), arg, &attachments).and_then(|weapon| {
                    let model =
                        camo.map_or(Ok(0), |camo| camo_slot(weapons.registry(), weapon, camo))?;
                    Ok((weapon, model))
                }) {
                    Ok((weapon, model)) => {
                        if let Err(error) =
                            inbox.push(local.0, ClientAction::GiveWeapon { weapon, model })
                        {
                            echo(format!("give: {error}"));
                            continue;
                        }

                        echo(format!(
                            "give: queued {} id={weapon}",
                            weapons.registry().configuration_label(weapon)
                        ));
                    }
                    Err(msg) => echo(format!("give: {msg}")),
                }
            }
            "attach" => {
                let Some(weapons) = weapons.as_ref() else {
                    echo("attach: weapon catalog not loaded".into());
                    continue;
                };
                let Some(ps) = presented.alive_player(local.0) else {
                    echo("attach: not Alive — spawn a class first".into());
                    continue;
                };
                let current = ps.weapon;
                if current == 0 {
                    echo("attach: no weapon in hands".into());
                    continue;
                }
                let Some(name) = cmd.args.first() else {
                    match attachment_hints(weapons.registry(), current) {
                        Ok(hints) => echo(format!("attach: {hints}")),
                        Err(reason) => echo(format!("attach: {reason}")),
                    }
                    continue;
                };
                let next = toggle_named_attachment(weapons.registry(), current, name);
                let next = match next {
                    Ok(id) => id,
                    Err(msg) => {
                        echo(format!("attach: {msg}"));
                        continue;
                    }
                };
                if next == current {
                    echo(format!(
                        "attach: `{}` is already the selected configuration — refused \
                             (an unchanged id is not a successful attach)",
                        weapons.registry().configuration_label(current)
                    ));
                    continue;
                }
                if let Err(error) = inbox.push(
                    local.0,
                    ClientAction::ChangeWeaponConfiguration {
                        from: current,
                        to: next,
                    },
                ) {
                    echo(format!("attach: {error}"));
                    continue;
                }
                echo(format!(
                    "attach: queued {} id={next}",
                    weapons.registry().configuration_label(next)
                ));
            }
            _ => {}
        }
    }
}

const GIVE_USAGE: &str = "give camo <name|slot|none> | give ammo | give killstreak/<name> | give weapon/<game:weapon> [attachment...] [camo=<name|slot>] — resupply ammo, acquire a reward, or equip a weapon";

pub(crate) fn split_camo(args: &[String]) -> (Option<&str>, Vec<String>) {
    let mut camo = None;
    let mut attachments = Vec::new();
    for arg in args {
        match arg.strip_prefix("camo=") {
            Some(value) => camo = Some(value),
            None => attachments.push(arg.clone()),
        }
    }
    (camo, attachments)
}

pub(crate) fn camo_slot(
    registry: &asset_game::WeaponRegistry,
    weapon: u32,
    camo: &str,
) -> Result<u8, String> {
    if camo.eq_ignore_ascii_case("none") {
        return Ok(0);
    }
    let slot = camo
        .parse::<u8>()
        .ok()
        .or_else(|| registry.camouflage_slot(weapon, camo));
    let choices = registry.camouflage_choices(weapon);
    slot.filter(|slot| *slot == 0 || choices.iter().any(|(own, _)| own == slot))
        .ok_or_else(|| {
            format!(
                "no camouflage `{camo}` (has {})",
                choices
                    .iter()
                    .map(|(_, name)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

#[derive(Debug, PartialEq)]
enum GiveTarget<'a> {
    Ammo,
    Camo(&'a str),
    Killstreak(&'a str),
    Weapon(&'a str),
}

fn parse_give_target(args: &[String]) -> Result<GiveTarget<'_>, &'static str> {
    let Some(item) = args.first() else {
        return Err(GIVE_USAGE);
    };
    if item == "camo" && args.len() == 2 {
        return Ok(GiveTarget::Camo(&args[1]));
    }
    if item == "ammo" && args.len() == 1 {
        return Ok(GiveTarget::Ammo);
    }
    if let Some(name) = item
        .strip_prefix("killstreak/")
        .filter(|name| !name.is_empty())
        && args.len() == 1
    {
        return Ok(GiveTarget::Killstreak(name));
    }
    if let Some(name) = item.strip_prefix("weapon/").filter(|name| !name.is_empty()) {
        return Ok(GiveTarget::Weapon(name));
    }
    Err(GIVE_USAGE)
}

fn change_camo(
    name: &str,
    weapons: Option<&PreparedWeapons>,
    presented: &PresentedSnapshot,
    local: &LocalPresentClient,
    inbox: &mut ClientActionInbox,
    mut echo: impl FnMut(String),
) {
    let Some(weapons) = weapons else {
        echo("give camo: weapon catalog not loaded".into());
        return;
    };
    let Some(ps) = presented.alive_player(local.0) else {
        echo("give camo: not Alive — spawn a class first".into());
        return;
    };
    let weapon = ps.weapon;
    if weapon == 0 {
        echo("give camo: no weapon in hands".into());
        return;
    }
    let model = match camo_slot(weapons.registry(), weapon, name) {
        Ok(model) => model,
        Err(error) => {
            echo(format!("give camo: {error}"));
            return;
        }
    };
    match inbox.push(local.0, ClientAction::ChangeWeaponCamo { weapon, model }) {
        Ok(()) => echo(format!(
            "give camo: queued {name} for {}",
            weapons.registry().configuration_label(weapon)
        )),
        Err(error) => echo(format!("give camo: {error}")),
    }
}

fn grant_killstreak(
    name: &str,
    completions: &CompletionSlot,
    presented: &PresentedSnapshot,
    local: &LocalPresentClient,
    inbox: &mut ClientActionInbox,
    mut echo: impl FnMut(String),
) {
    let names =
        completions.read(|value| value.map_or_else(Vec::new, |value| value.killstreaks.clone()));
    let requested = name.to_ascii_lowercase().replace(['-', ' '], "_");
    let requested = KILLSTREAK_ALIASES
        .iter()
        .find(|(alias, _)| *alias == requested)
        .map_or(requested.as_str(), |(_, name)| *name);
    let Some(name) = names.iter().find(|name| name.as_str() == requested) else {
        echo(format!(
            "give: unknown or unavailable reward `{requested}`; autocomplete give killstreak/ for choices"
        ));
        return;
    };
    if !alive(presented, local.0) {
        echo("give: spawn a class first".into());
        return;
    }
    let Some(field) = sim::menu_response_field(name) else {
        return;
    };
    match inbox.push(local.0, ClientAction::GiveKillstreak { name: field }) {
        Ok(()) => echo(format!(
            "give: acquisition queued for killstreak/{name}; use its action slot when ready"
        )),
        Err(error) => echo(format!("give: {error}")),
    }
}

fn resupply_ammo(
    presented: &PresentedSnapshot,
    local: &LocalPresentClient,
    inbox: &mut ClientActionInbox,
    mut echo: impl FnMut(String),
) {
    if !alive(presented, local.0) {
        echo("give: spawn a class first".into());
        return;
    }
    match inbox.push(local.0, ClientAction::ResupplyAmmo) {
        Ok(()) => echo("give: ammo resupply queued; reload magazines normally".into()),
        Err(error) => echo(format!("give: {error}")),
    }
}

const KILLSTREAK_ALIASES: &[(&str, &str)] = &[
    ("care_package", "airdrop"),
    ("predator", "predator_missile"),
    ("pave_low", "helicopter_flares"),
    ("pavelow", "helicopter_flares"),
    ("chopper_gunner", "helicopter_minigun"),
    ("sentry_gun", "airdrop_sentry_minigun"),
    ("emergency_airdrop", "airdrop_mega"),
    ("harrier", "harrier_airstrike"),
];

struct KillstreakCompleter(CompletionSlot);

impl ArgCompleter for KillstreakCompleter {
    fn complete(&self, prefix: &str) -> Vec<String> {
        let names = self
            .0
            .read(|value| value.map_or_else(Vec::new, |value| value.killstreaks.clone()));
        let aliases = KILLSTREAK_ALIASES
            .iter()
            .filter(|(_, name)| names.iter().any(|value| value == name))
            .map(|(alias, _)| alias.to_string());
        StaticCompleter::new(names.iter().cloned().chain(aliases)).complete(prefix)
    }
}

struct GiveCompleter(CompletionSlot);

impl ArgCompleter for GiveCompleter {
    fn complete(&self, prefix: &str) -> Vec<String> {
        let mut items = vec!["ammo".to_owned(), "camo".to_owned()];
        self.0.read(|value| {
            if let Some(value) = value {
                items.extend(value.give.iter().map(|name| format!("weapon/{name}")));
            }
        });
        items.extend(
            KillstreakCompleter(self.0.clone())
                .complete("")
                .into_iter()
                .map(|name| format!("killstreak/{name}")),
        );
        StaticCompleter::new(items).complete(prefix)
    }

    fn complete_with_context(&self, prefix: &str, args: &[&str]) -> Vec<String> {
        match args {
            [] => self.complete(prefix),
            ["camo"] => LiveListCompleter(self.0.clone(), |value| &value.camos).complete(prefix),
            _ => Vec::new(),
        }
    }
}

struct BotCompleter(CompletionSlot);

impl ArgCompleter for BotCompleter {
    fn complete(&self, prefix: &str) -> Vec<String> {
        StaticCompleter::new(["add", "dummy", "hold", "give", "fire", "tp"]).complete(prefix)
    }
    fn complete_with_context(&self, prefix: &str, args: &[&str]) -> Vec<String> {
        let Some(verb) = args.first() else {
            return self.complete(prefix);
        };
        match (*verb, args.len()) {
            ("add" | "dummy", 1) => {
                StaticCompleter::new((1..=16).map(|n| n.to_string())).complete(prefix)
            }
            ("hold", 1) => StaticCompleter::new(["on", "off"]).complete(prefix),
            ("give" | "fire" | "tp", 1) => {
                let mut ids = self
                    .0
                    .read(|value| value.map_or_else(Vec::new, |value| value.bots.clone()));
                if *verb != "give" {
                    ids.insert(0, "all".into());
                }
                StaticCompleter::new(ids).complete(prefix)
            }
            ("give", 2) => {
                let names = self
                    .0
                    .read(|value| value.map_or_else(Vec::new, |value| value.give.clone()));
                StaticCompleter::new(names).complete(prefix)
            }
            ("tp", 2) => StaticCompleter::new(["above"]).complete(prefix),
            _ => Vec::new(),
        }
    }
}

fn alive(presented: &PresentedSnapshot, id: sim::ClientId) -> bool {
    presented
        .snapshot()
        .and_then(|s| s.meta.for_client(id))
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
}

pub(crate) fn resolve_give_id(
    registry: &asset_game::WeaponRegistry,
    raw: &str,
    attachments: &[String],
) -> Result<u32, String> {
    let resolve = |selection: WeaponSelection| {
        registry
            .resolve_configuration(&selection, LoadoutRules::default())
            .map(|resolved| {
                registry
                    .bind(resolved.handle())
                    .expect("resolved in this registry")
                    .wire_id()
            })
            .map_err(|refusal| format!("`{raw}`: {refusal} ({})", refusal.code()))
    };
    let families = registry.weapon_families();
    let family = asset_game::FamilyKey::parse(raw)
        .and_then(|key| families.find(&key))
        .or_else(|| {
            families
                .offered()
                .find(|family| family.key.short().eq_ignore_ascii_case(raw))
        });
    if let Some(family) = family {
        return resolve(WeaponSelection::with(family.key.clone(), attachments));
    }
    let canonical = asset_game::FamilyKey::parse(raw).map(|key| key.asset_key());
    let id = match registry.resolve_index(canonical.as_deref().unwrap_or(raw)) {
        Ok(Some(id)) => id,
        Ok(None) => return Err("empty weapon name".into()),
        Err(_) => return Err(format!("unknown weapon `{raw}`")),
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

fn family_of(
    registry: &asset_game::WeaponRegistry,
    weapon: u32,
) -> Result<WeaponSelection, String> {
    registry
        .describe_configuration(weapon)
        .filter(|selection| selection.family.is_some())
        .cloned()
        .ok_or_else(|| format!("`{}` belongs to no weapon family", registry.name_of(weapon)))
}

fn attachment_hints(registry: &asset_game::WeaponRegistry, current: u32) -> Result<String, String> {
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
    registry: &asset_game::WeaponRegistry,
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
        .map(|resolved| {
            registry
                .bind(resolved.handle())
                .expect("toggle in this registry")
                .wire_id()
        })
        .map_err(|refusal| format!("{refusal} ({})", refusal.code()))
}

pub fn weapon_completions(weapons: &PreparedWeapons) -> Vec<String> {
    weapons.registry().weapon_completion_names().to_vec()
}

pub fn attach_completions(weapons: &PreparedWeapons, current_weapon: u32) -> Vec<String> {
    let Ok(selection) = family_of(weapons.registry(), current_weapon) else {
        return Vec::new();
    };
    weapons
        .registry()
        .list_attachment_choices(&selection, LoadoutRules::default())
        .map(|options| {
            options
                .into_iter()
                .map(|option| option.choice.name)
                .collect()
        })
        .unwrap_or(Vec::new())
}
