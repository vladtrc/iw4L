use bevy::prelude::*;
use frame::ScopeApp;
use net::{ClientActionInbox, ClientSet, LocalPresentClient, PresentedSnapshot};
use sim::{ClassId, ClientAction, ClientLifecycle, SimEvent};

use crate::classes::select::{
    ClassChangeAllowed, ClassChangeBlockReason, ClassSelectOverlayOpen, ClassSelectPhase,
    ClassSelectStatus, PendingClassEquip, accept_class_equip, reject_class_equip,
};
use crate::classes::store::SessionClassStore;

#[derive(Resource, Debug, Default)]
pub struct EquipTxnWatch {
    request_id: Option<u32>,
    next_revision: u32,
    correlation: Option<EquipCorrelation>,
}

#[derive(Clone, Debug)]
struct EquipCorrelation {
    class_id: ClassId,
    revision: u32,
    load_key: frame::LocalLoadKey,
    class: frame::HostClassSlot,
    catalog_revision: u64,
    families: Option<frame::AssetFamilies>,
    capability_version: Option<(u32, frame::WorldProducts)>,
    policy: Vec<(String, String)>,
}

pub fn apply_pending_class_equip(
    mut pending: ResMut<PendingClassEquip>,
    store: Res<SessionClassStore>,
    catalog: Option<Res<crate::ClassLoadoutCatalog>>,
    player_data: Res<sim::LocalPlayerData>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    mut actions: ResMut<ClientActionInbox>,
    mut watch: ResMut<EquipTxnWatch>,
    mut phase: ResMut<ClassSelectPhase>,
    mut status: ResMut<ClassSelectStatus>,
    local: Res<LocalPresentClient>,
    signon: Option<Res<net::SignonState>>,
    context: (
        Res<frame::InstalledMatch>,
        Option<Res<frame::HostMatchRules>>,
        Option<Res<assets::prepared::PreparedCapabilities>>,
    ),
) {
    let Some(req) = pending.0.take() else {
        return;
    };
    let Some(catalog) = catalog else {
        reject_class_equip(
            &mut phase,
            &mut status,
            req.request_id,
            "Weapon catalog is not ready".to_owned(),
        );
        return;
    };
    if let Some(signon) = signon.as_deref()
        && !signon.may_select_class()
    {
        let reason = signon
            .phase
            .status_line()
            .unwrap_or_else(|| "signon not Active".to_owned());
        diag::info!(
            Ui,
            "class equip: dropped request_id={} — {reason}",
            req.request_id
        );
        reject_class_equip(&mut phase, &mut status, req.request_id, reason);
        return;
    }
    let index = req.class_index;
    let Some(slot) = store.slots.get(index) else {
        reject_class_equip(
            &mut phase,
            &mut status,
            req.request_id,
            "Class is unavailable".to_owned(),
        );
        return;
    };
    if let Err(reason) = store
        .lock_reason(index)
        .map_or_else(|| catalog.validate_progression(slot, &player_data), Err)
    {
        diag::info!(
            Ui,
            "class equip: refused request_id={} — {reason}",
            req.request_id
        );
        reject_class_equip(&mut phase, &mut status, req.request_id, reason);
        return;
    }
    let resolved = weapons
        .as_ref()
        .ok_or_else(|| "Weapon catalog is not ready".to_owned())
        .and_then(|weapons| {
            let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
            session::loadout::resolve_personal_class(&row, weapons.registry())
        });
    let loadout = match resolved {
        Ok(loadout) => loadout,
        Err(reason) => {
            diag::info!(
                Ui,
                "class equip: refused request_id={} — {reason}",
                req.request_id
            );
            reject_class_equip(&mut phase, &mut status, req.request_id, reason);
            return;
        }
    };

    let Some(revision) = watch.next_revision.checked_add(1) else {
        reject_class_equip(
            &mut phase,
            &mut status,
            req.request_id,
            "Class revision capacity exhausted",
        );
        return;
    };
    watch.next_revision = revision;
    let correlation = EquipCorrelation {
        class_id: ClassId(index as u32),
        revision,
        load_key: context.0.load_key,
        class: frame::HostClassSlot::from(slot),
        catalog_revision: catalog.revision,
        families: store.server_families(),
        capability_version: context
            .2
            .as_ref()
            .map(|facts| (facts.version, facts.products)),
        policy: context
            .1
            .as_ref()
            .map_or_else(Vec::new, |policy| policy.0.clone()),
    };
    if let Err(error) = actions.push(
        local.0,
        ClientAction::SelectClass {
            request_id: req.request_id,
            class_id: ClassId(index as u32),
            revision,
            loadout,
        },
    ) {
        diag::info!(
            Ui,
            "class equip: request_id={} not queued — {error}",
            req.request_id
        );
        let _ = reject_class_equip(
            &mut phase,
            &mut status,
            req.request_id,
            format!("not queued — {error}"),
        );
        return;
    }
    watch.request_id = Some(req.request_id);
    watch.correlation = Some(correlation);
    diag::info!(
        Ui,
        "class select: queued SelectClass request_id={} id={} rev={revision} (queued→gather→step→codec→present)",
        req.request_id,
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

pub fn resolve_class_equip_transaction(
    mut equipped: ResMut<crate::classes::store::EquippedClass>,
    mut phase: ResMut<ClassSelectPhase>,
    mut overlay: ResMut<ClassSelectOverlayOpen>,
    mut status: ResMut<ClassSelectStatus>,
    mut watch: ResMut<EquipTxnWatch>,
    mut reliable: MessageReader<net::ReliableControlEvent>,
    context: (
        Res<SessionClassStore>,
        Res<crate::ClassLoadoutCatalog>,
        Res<frame::InstalledMatch>,
        Option<Res<frame::HostMatchRules>>,
        Option<Res<assets::prepared::PreparedCapabilities>>,
    ),
) {
    let Some(request_id) = watch.request_id else {
        reliable.clear();
        return;
    };

    let (store, catalog, installed, policy, capabilities) = context;
    let current_policy = policy
        .as_ref()
        .map_or_else(Vec::new, |policy| policy.0.clone());
    let Some(correlation) = watch.correlation.clone() else {
        reliable.clear();
        return;
    };
    let unchanged = correlation.load_key == installed.load_key
        && correlation.catalog_revision == catalog.revision
        && correlation.policy == current_policy
        && correlation.families == store.server_families()
        && correlation.capability_version
            == capabilities
                .as_ref()
                .map(|facts| (facts.version, facts.products))
        && store
            .slots
            .get(correlation.class_id.0 as usize)
            .is_some_and(|slot| frame::HostClassSlot::from(slot) == correlation.class);
    if !unchanged {
        reject_class_equip(
            &mut phase,
            &mut status,
            request_id,
            "Class or session policy changed while awaiting admission",
        );
        watch.request_id = None;
        watch.correlation = None;
        reliable.clear();
        return;
    }
    for event in reliable.read() {
        match event.0 {
            SimEvent::ClassRejected {
                request_id: rid,
                reason,
                class_id,
                revision,
            } if rid == request_id
                && class_id == correlation.class_id
                && revision == correlation.revision =>
            {
                let reason = reason.as_str();
                if reject_class_equip(&mut phase, &mut status, request_id, reason) {
                    diag::info!(
                        Ui,
                        "class select: reliable Reject request_id={request_id} reason={reason}"
                    );
                }
                watch.request_id = None;
                watch.correlation = None;
                return;
            }
            SimEvent::ClassAccepted {
                request_id: rid,
                class_id,
                revision,
            } if rid == request_id
                && class_id == correlation.class_id
                && revision == correlation.revision =>
            {
                if accept_class_equip(&mut equipped, &mut phase, &mut overlay, request_id) {
                    diag::info!(
                        Ui,
                        "class select: reliable Accept request_id={request_id} (overlay closed)"
                    );
                }
                watch.request_id = None;
                watch.correlation = None;
                return;
            }
            _ => {}
        }
    }
}

pub(crate) fn register_equip_systems(app: &mut App) {
    app.add_systems(
        OnExit(frame::MatchScope::Live),
        release_equip.in_set(frame::ScopeSet::Release),
    )
    .scoped::<crate::classes::store::EquippedClass>(frame::MatchScope::Live)
    .scoped::<EquipTxnWatch>(frame::MatchScope::Live)
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
        resolve_class_equip_transaction
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
                .then(|| families.as_deref().copied())
                .flatten()
        });
    if store.server_families() != selected {
        store.set_server_families(selected);
    }
}

fn publish_class_availability(
    store: Res<SessionClassStore>,
    catalog: Option<Res<crate::ClassLoadoutCatalog>>,
    player_data: Res<sim::LocalPlayerData>,
    mut dvars: ResMut<frame::UiMenuDvars>,
) {
    let Some(catalog) = catalog else {
        return;
    };
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
        feature_lock.clone().unwrap_or_default(),
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
            reason.unwrap_or_default(),
        );
    }
}

fn consume_class_select_handoff(
    mut handoff: ResMut<frame::ClassSelectHandoff>,
    catalog: Option<ResMut<crate::ClassLoadoutCatalog>>,
    mut store: ResMut<SessionClassStore>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    menus: Res<asset_game::MenuCatalog>,
) {
    if !handoff.pending {
        return;
    }
    let (Some(weapons), Some(mut catalog)) = (weapons, catalog) else {
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
