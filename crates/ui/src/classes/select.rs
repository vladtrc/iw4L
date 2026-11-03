use bevy::prelude::*;
use frame::ScopeApp;
use frame::{AppScreen, ClientSet, RuntimeRole};
use net::SignonState;

use crate::classes::icons::{ClassSelectIconCache, UiAssetRoot, warm_class_select_icons};
use crate::classes::store::SessionClassStore;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClassSelectOverlayOpen(pub bool);

pub(crate) fn overlay_open_after_screen(
    prev: Option<AppScreen>,
    now: AppScreen,
    replay: bool,
    failed: bool,
    currently_open: bool,
) -> bool {
    if failed || replay {
        return false;
    }
    if matches!(now, AppScreen::ClassSelect) {
        if !matches!(prev, Some(AppScreen::ClassSelect)) {
            return true;
        }
        return currently_open;
    }
    false
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClassChangeAllowed(pub bool);

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassChangeBlockReason(pub Option<String>);

#[derive(Resource, Debug, Clone, PartialEq, Eq, Default)]
pub enum ClassSelectPhase {
    #[default]
    Interactive,
    Pending {
        class_index: usize,
    },
}

impl ClassSelectPhase {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }
}

#[derive(Resource, Debug, Default)]
pub struct PendingClassEquip(pub Option<usize>);

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassSelectStatus(pub Option<String>);

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct ClassSelectHighlight(pub usize);

pub fn class_index_by_name(store: &SessionClassStore, name: &str) -> Option<usize> {
    if let Ok(index) = name.parse::<usize>()
        && index < store.slots.len()
    {
        return Some(index);
    }
    store
        .slots
        .iter()
        .position(|slot| slot.name.eq_ignore_ascii_case(name))
}

pub fn commit_class_equip(
    index: usize,
    store: &mut SessionClassStore,
    highlight: &mut ClassSelectHighlight,
    phase: &mut ClassSelectPhase,
    pending: &mut PendingClassEquip,
    status: &mut ClassSelectStatus,
) -> Result<(), String> {
    if phase.is_pending() {
        return Err("equip already pending".into());
    }
    u32::try_from(index).map_err(|_| "class index exceeds wire capacity".to_owned())?;
    if let Some(reason) = store.lock_reason(index) {
        status.0 = Some(reason.clone());
        return Err(reason);
    }
    if store.slots.get(index).is_some() {
        highlight.0 = index;
        store.selected = index;
    }
    pending.0 = Some(index);
    *phase = ClassSelectPhase::Pending { class_index: index };
    status.0 = None;
    Ok(())
}

pub fn accept_class_equip(
    equipped: &mut crate::classes::store::EquippedClass,
    phase: &mut ClassSelectPhase,
    overlay: &mut ClassSelectOverlayOpen,
) {
    if let ClassSelectPhase::Pending { class_index } = *phase {
        equipped.0 = Some(class_index);
        *phase = ClassSelectPhase::Interactive;
        overlay.0 = false;
    }
}

pub fn reject_class_equip(
    phase: &mut ClassSelectPhase,
    status: &mut ClassSelectStatus,
    reason: impl Into<String>,
) {
    *phase = ClassSelectPhase::Interactive;
    status.0 = Some(reason.into());
}

pub(crate) struct ClassSelectPlugin;

impl Plugin for ClassSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClassSelectOverlayOpen>()
            .init_resource::<ClassChangeAllowed>()
            .init_resource::<ClassChangeBlockReason>()
            .scoped::<PendingClassEquip>(frame::MatchScope::Live)
            .scoped::<ClassSelectPhase>(frame::MatchScope::Live)
            .scoped::<ClassSelectStatus>(frame::MatchScope::Live)
            .init_resource::<ClassSelectHighlight>()
            .init_resource::<ClassSelectIconCache>()
            .init_resource::<UiAssetRoot>()
            .init_resource::<SessionClassStore>()
            .add_systems(
                PostUpdate,
                sync_class_select_overlay_to_screen.before(crate::layers::ApplyUiLayers),
            )
            .add_systems(Update, warm_class_select_icons.in_set(ClientSet::Ui));
    }
}

fn sync_class_select_overlay_to_screen(
    screen: Res<AppScreen>,
    role: Option<Res<RuntimeRole>>,
    signon: Option<Res<SignonState>>,
    mut overlay: ResMut<ClassSelectOverlayOpen>,
    mut last: Local<Option<AppScreen>>,
) {
    let replay = role.is_some_and(|role| *role == RuntimeRole::Replay);
    let failed = signon.is_some_and(|signon| signon.phase.is_failed());
    let next = overlay_open_after_screen(*last, *screen, replay, failed, overlay.0);
    if overlay.0 != next {
        overlay.0 = next;
    }
    *last = Some(*screen);
}
