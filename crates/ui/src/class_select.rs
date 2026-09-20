use bevy::prelude::*;
use frame::{AppScreen, ClientSet, RuntimeRole};
use net::SignonState;

use crate::class_icons::{ClassSelectIconCache, UiAssetRoot, warm_class_select_icons};
use crate::class_store::SessionClassStore;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
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

impl Default for ClassSelectOverlayOpen {
    fn default() -> Self {
        Self(false)
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassChangeAllowed(pub bool);

impl Default for ClassChangeAllowed {
    fn default() -> Self {
        Self(false)
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassChangeBlockReason(pub Option<String>);

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum ClassSelectPhase {
    Interactive,
    Pending { request_id: u32, class_index: usize },
}

impl Default for ClassSelectPhase {
    fn default() -> Self {
        Self::Interactive
    }
}

impl ClassSelectPhase {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    pub fn pending_request_id(&self) -> Option<u32> {
        match *self {
            Self::Pending { request_id, .. } => Some(request_id),
            Self::Interactive => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassEquipRequest {
    pub request_id: u32,
    pub class_index: usize,
}

#[derive(Resource, Debug, Default)]
pub struct PendingClassEquip(pub Option<ClassEquipRequest>);

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClassSelectStatus(pub Option<String>);

#[derive(Resource, Debug, Clone, Copy)]
pub struct ClassSelectHighlight(pub usize);

impl Default for ClassSelectHighlight {
    fn default() -> Self {
        Self(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassEquipRefusal {
    AlreadyPending { request_id: u32 },

    UnknownClass(String),

    LockedContent { index: usize, reason: String },
}

impl std::fmt::Display for ClassEquipRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyPending { request_id } => {
                write!(f, "equip already pending (request_id={request_id})")
            }
            Self::UnknownClass(name) => write!(f, "unknown_class: `{name}`"),
            Self::LockedContent { index, reason } => {
                write!(f, "locked_content: preset {index} — {reason}")
            }
        }
    }
}

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
    seq: &mut net::ActionRequestIds,
) -> Result<u32, ClassEquipRefusal> {
    if let Some(request_id) = phase.pending_request_id() {
        return Err(ClassEquipRefusal::AlreadyPending { request_id });
    }
    let Some(slot) = store.slots.get(index) else {
        return Err(ClassEquipRefusal::UnknownClass(index.to_string()));
    };
    if let Some(reason) = slot.lock_reason.as_deref() {
        let refusal = ClassEquipRefusal::LockedContent {
            index,
            reason: reason.to_owned(),
        };
        status.0 = Some(refusal.to_string());
        return Err(refusal);
    }
    let request_id = seq.allocate();
    highlight.0 = index;
    store.commit_equip(index);
    pending.0 = Some(ClassEquipRequest {
        request_id,
        class_index: index,
    });
    *phase = ClassSelectPhase::Pending {
        request_id,
        class_index: index,
    };
    status.0 = None;
    Ok(request_id)
}

pub fn accept_class_equip(
    phase: &mut ClassSelectPhase,
    overlay: &mut ClassSelectOverlayOpen,
    request_id: u32,
) -> bool {
    match *phase {
        ClassSelectPhase::Pending {
            request_id: pending,
            ..
        } if pending == request_id => {
            *phase = ClassSelectPhase::Interactive;
            overlay.0 = false;
            true
        }
        _ => false,
    }
}

pub fn reject_class_equip(
    phase: &mut ClassSelectPhase,
    status: &mut ClassSelectStatus,
    request_id: u32,
    reason: impl Into<String>,
) -> bool {
    match *phase {
        ClassSelectPhase::Pending {
            request_id: pending,
            ..
        } if pending == request_id => {
            *phase = ClassSelectPhase::Interactive;
            status.0.replace(reason.into());
            true
        }
        _ => false,
    }
}

pub(crate) struct ClassSelectPlugin;

impl Plugin for ClassSelectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClassSelectOverlayOpen>()
            .init_resource::<ClassChangeAllowed>()
            .init_resource::<ClassChangeBlockReason>()
            .init_resource::<PendingClassEquip>()
            .init_resource::<ClassSelectPhase>()
            .init_resource::<ClassSelectStatus>()
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
