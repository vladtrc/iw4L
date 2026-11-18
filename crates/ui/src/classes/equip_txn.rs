use bevy::prelude::*;
use frame::ScopeApp;
use net::{ClientActionInbox, ClientSet, LocalPresentClient, PresentedSnapshot};
use sim::{ClassId, ClientAction, ClientLifecycle};

use crate::classes::select::{
    ClassChangeAllowed, ClassChangeBlockReason, ClassSelectOverlayOpen, ClassSelectPhase,
    ClassSelectStatus, PendingClassEquip, accept_class_equip, reject_class_equip,
};
use crate::classes::store::SessionClassStore;

#[derive(bevy::ecs::system::SystemParam)]
pub struct PersonalClassSource<'w> {
    store: Res<'w, SessionClassStore>,
    catalog: Res<'w, crate::ClassLoadoutCatalog>,
    player_data: Res<'w, sim::LocalPlayerData>,
    weapons: Option<Res<'w, assets::PreparedWeapons>>,
}

impl PersonalClassSource<'_> {
    fn resolve(&self, index: usize) -> Result<sim::PersonalClass, String> {
        if let Some(slot) = self.store.slots.get(index) {
            self.store
                .lock_reason(index)
                .map_or_else(
                    || self.catalog.validate_progression(slot, &self.player_data),
                    Err,
                )
                .and_then(|()| {
                    let weapons = self
                        .weapons
                        .as_ref()
                        .ok_or_else(|| "Weapon catalog is not ready".to_owned())?;
                    let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
                    session::loadout::resolve_personal_class(&row, weapons.registry())
                })
        } else {
            Ok(sim::PersonalClass::default())
        }
    }
}

pub fn apply_pending_class_equip(
    mut pending: ResMut<PendingClassEquip>,
    source: PersonalClassSource,
    mut actions: ResMut<ClientActionInbox>,
    mut phase: ResMut<ClassSelectPhase>,
    mut status: ResMut<ClassSelectStatus>,
    local: Res<LocalPresentClient>,
    signon: Option<Res<net::SignonState>>,
) {
    let Some(index) = pending.0.take() else {
        return;
    };
    if let Some(signon) = signon.as_deref()
        && !signon.may_select_class()
    {
        let reason = signon
            .phase
            .status_line()
            .unwrap_or_else(|| "signon not Active".to_owned());
        diag::info!(Ui, "class equip: dropped class={} — {reason}", index);
        reject_class_equip(&mut phase, &mut status, reason);
        return;
    }
    let resolved = source.resolve(index);
    let loadout = match resolved {
        Ok(loadout) => loadout,
        Err(reason) => {
            reject_class_equip(&mut phase, &mut status, reason);
            return;
        }
    };

    if let Err(error) = actions.push(
        local.0,
        ClientAction::SelectClass {
            class_id: ClassId(index as u32),
            loadout,
        },
    ) {
        diag::info!(Ui, "class equip: class={} not queued — {error}", index);
        reject_class_equip(&mut phase, &mut status, format!("not queued — {error}"));
        return;
    }
    diag::info!(
        Ui,
        "class select: queued SelectClass class={} id={} (queued→gather→step→codec→present)",
        index,
        index
    );
}

pub fn sync_class_change_allowed(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    overlay: Res<ClassSelectOverlayOpen>,
    mut allowed: ResMut<ClassChangeAllowed>,
    mut block: ResMut<ClassChangeBlockReason>,
    signon: Option<Res<net::SignonState>>,
) {
    if let Some(signon) = signon.as_deref()
        && !signon.may_select_class()
    {
        let reason = signon
            .phase
            .status_line()
            .unwrap_or_else(|| "class change not allowed (signon not Active)".to_owned());
        if allowed.0 {
            allowed.0 = false;
        }
        if block.0.as_deref() != Some(reason.as_str()) {
            block.0 = Some(reason);
        }
        return;
    }
    let (next_allowed, reason) = match presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
    {
        Some(meta) => match meta.lifecycle {
            ClientLifecycle::Alive
            | ClientLifecycle::Dead
            | ClientLifecycle::ChoosingClass
            | ClientLifecycle::Spectating
            | ClientLifecycle::Connecting => (true, None),
            other => (
                false,
                Some(format!("class change not allowed (lifecycle={other:?})")),
            ),
        },
        None if overlay.0 => (true, None),
        None => (
            false,
            Some("class change not allowed (no match loaded)".to_owned()),
        ),
    };
    if allowed.0 != next_allowed {
        allowed.0 = next_allowed;
    }
    if block.0.as_deref() != reason.as_deref() {
        block.0 = reason;
    }
}

pub fn sync_class_equip_state(
    mut equipped: ResMut<crate::classes::store::EquippedClass>,
    mut phase: ResMut<ClassSelectPhase>,
    mut overlay: ResMut<ClassSelectOverlayOpen>,
    mut status: ResMut<ClassSelectStatus>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    actions: Res<ClientActionInbox>,
) {
    let ClassSelectPhase::Pending { class_index } = *phase else {
        return;
    };
    if actions.is_pending() {
        return;
    }
    let Some(meta) = presented
        .snapshot()
        .and_then(|snapshot| snapshot.meta.for_client(local.0))
    else {
        return;
    };
    let class_id = ClassId(class_index as u32);
    if let Some((refused, reason)) = meta.class_refusal.filter(|(id, _)| *id == class_id) {
        reject_class_equip(&mut phase, &mut status, reason.as_str());
        diag::info!(
            Ui,
            "class select: state refused id={} reason={}",
            refused.0,
            reason.as_str()
        );
    } else if meta.equipped_class == Some(class_id) {
        accept_class_equip(&mut equipped, &mut phase, &mut overlay);
        diag::info!(Ui, "class select: state equipped id={}", class_id.0);
    }
}

pub(crate) fn register_equip_systems(app: &mut App) {
    app.add_systems(
        OnExit(frame::MatchScope::Live),
        release_equip.in_set(frame::ScopeSet::Release),
    )
    .scoped::<crate::classes::store::EquippedClass>(frame::MatchScope::Live)
    .init_resource::<frame::ClassSelectHandoff>()
    .add_systems(
        Update,
        (
            consume_class_select_handoff,
            sync_server_class_families,
            publish_class_availability,
            publish_signon_class_status.in_set(frame::InMatch),
            sync_class_change_allowed,
            apply_pending_class_equip.in_set(frame::InMatch),
        )
            .chain()
            .in_set(ClientSet::Present),
    )
    .add_systems(
        Update,
        sync_class_equip_state
            .in_set(frame::InMatch)
            .after(apply_pending_class_equip)
            .in_set(frame::ClassEquipResolved),
    );
}

fn sync_server_class_families(
    mut store: ResMut<SessionClassStore>,
    bridge: Option<Res<net::MasterBridge>>,
    families: Option<Res<frame::AssetFamilies>>,
    scope: Res<State<frame::MatchScope>>,
) {
    let selected = bridge
        .as_ref()
        .and_then(|bridge| bridge.state().families())
        .or_else(|| {
            (*scope.get() == frame::MatchScope::Live)
                .then(|| families.as_deref().cloned())
                .flatten()
        });
    if store.server_families() != selected {
        store.set_server_families(selected);
    }
}

fn publish_class_availability(
    store: Res<SessionClassStore>,
    catalog: Res<crate::ClassLoadoutCatalog>,
    player_data: Res<sim::LocalPlayerData>,
    mut dvars: ResMut<frame::UiMenuDvars>,
) {
    if !store.is_changed()
        && !catalog.is_changed()
        && !player_data.is_changed()
        && dvars.get("ui_class_unavailable_0").is_some()
    {
        return;
    }
    let feature_lock = catalog.progression_lock("cac", &player_data);
    dvars.set(
        "ui_class_feature_lock",
        feature_lock.clone().unwrap_or(String::new()),
    );
    for index in 0..sim::match_state::PERSONAL_CLASS_SLOTS {
        let reason = feature_lock
            .clone()
            .or_else(|| {
                (!player_data.custom_class_available(index))
                    .then(|| "Custom class slot is locked".to_owned())
            })
            .or_else(|| {
                store.slots.get(index).map_or_else(
                    || Some("Class is unavailable".to_owned()),
                    |slot| {
                        store.lock_reason(index).or_else(|| {
                            catalog
                                .validate_class(slot)
                                .and_then(|()| catalog.validate_progression(slot, &player_data))
                                .err()
                        })
                    },
                )
            });
        dvars.set(
            &format!("ui_class_unavailable_{index}"),
            reason.unwrap_or(String::new()),
        );
    }
}

fn consume_class_select_handoff(
    mut handoff: ResMut<frame::ClassSelectHandoff>,
    mut catalog: ResMut<crate::ClassLoadoutCatalog>,
    mut store: ResMut<SessionClassStore>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    menus: Res<asset_game::MenuCatalog>,
) {
    if !handoff.pending {
        return;
    }
    let Some(weapons) = weapons else {
        return;
    };
    let revision = catalog.revision.wrapping_add(1);
    let mut updated = crate::ClassLoadoutCatalog::from_editor_catalog(std::sync::Arc::new(
        weapons.registry().editor_catalog(),
    ));
    if let Some(table) = menus.string_table("mp/perkTable.csv") {
        updated = updated.with_perk_table(table);
    }
    updated.perks = catalog.perks.clone();
    updated.deathstreak = catalog.deathstreak.clone();
    for (key, preview) in &catalog.previews {
        if asset_core::AssetKey::parse(key).is_err() {
            updated.previews.insert(key.clone(), preview.clone());
        } else if let Some(current) = updated.previews.get_mut(key) {
            if current.desc_key.is_empty() {
                current.desc_key.clone_from(&preview.desc_key);
            }
            current.bars.clone_from(&preview.bars);
        }
    }
    *catalog = updated;
    catalog.revision = revision;
    catalog.primary = std::mem::take(&mut handoff.primary);
    catalog.secondary = std::mem::take(&mut handoff.secondary);
    catalog.lethal = std::mem::take(&mut handoff.lethal);
    catalog.tactical = std::mem::take(&mut handoff.tactical);
    catalog.excluded = std::mem::take(&mut handoff.excluded);
    store.apply_coverage_locks(std::mem::take(&mut handoff.lock_reasons));
    handoff.pending = false;
}

fn publish_signon_class_status(
    signon: Option<Res<net::SignonState>>,
    phase: Res<ClassSelectPhase>,
    mut status: ResMut<ClassSelectStatus>,
) {
    if phase.is_pending() {
        return;
    }
    let Some(signon) = signon else {
        return;
    };
    match signon.phase.status_line() {
        Some(line) => {
            if status.0.as_deref() != Some(line.as_str()) {
                status.0 = Some(line);
            }
        }
        None => {
            if status.0.as_deref().is_some_and(|line| {
                line.starts_with("waiting for host") || line.starts_with("handshake rejected")
            }) {
                status.0 = None;
            }
        }
    }
}

fn release_equip(mut handoff: ResMut<frame::ClassSelectHandoff>) {
    *handoff = frame::ClassSelectHandoff::default();
}
