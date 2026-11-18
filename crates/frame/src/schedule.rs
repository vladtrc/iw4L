use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoritySet {
    Advance,

    Ingress,

    Gather,

    Step,

    Snapshot,

    Fanout,

    Bookkeeping,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientSet {
    Load,

    Receive,

    Reconcile,

    Input,

    Predict,

    Send,

    Present,

    Ui,

    Effects,

    Diag,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderSet {
    FrontendPrepare,
    Anim,
    Fx,
    FrontendAssemble,
    Gpu,
}

pub fn configure_render_sets(app: &mut App) {
    app.configure_sets(
        Update,
        (
            RenderSet::FrontendPrepare,
            RenderSet::Anim,
            RenderSet::Fx,
            RenderSet::FrontendAssemble,
            RenderSet::Gpu,
        )
            .chain()
            .in_set(ClientSet::Present),
    );
    app.configure_sets(
        PostUpdate,
        (
            RenderSet::FrontendPrepare,
            RenderSet::Anim,
            RenderSet::Fx,
            RenderSet::FrontendAssemble,
            RenderSet::Gpu,
        )
            .chain(),
    );
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientEdge(pub u8);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuthorityEdge(pub u8);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthorityBookkeeping;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassEquipResolved;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PresentedPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifeFrontPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FxSoundPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnerEventsPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionSwapApplied;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelLightingSeated;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerCmdSet {
    DynamicModels,
    SceneVisibility,
    WorldVisibility,

    ModelPose,
}

pub fn configure_authority_sets(app: &mut App) {
    for (index, set) in [
        AuthoritySet::Advance,
        AuthoritySet::Ingress,
        AuthoritySet::Gather,
        AuthoritySet::Step,
        AuthoritySet::Snapshot,
        AuthoritySet::Fanout,
        AuthoritySet::Bookkeeping,
    ]
    .into_iter()
    .enumerate()
    {
        let index = index as u8;
        app.configure_sets(
            FixedUpdate,
            (AuthorityEdge(index), set.clone(), AuthorityEdge(index + 1)).chain(),
        );
    }
    app.configure_sets(
        FixedUpdate,
        AuthorityBookkeeping.in_set(AuthoritySet::Bookkeeping),
    );
}

pub fn configure_client_sets(app: &mut App) {
    for (index, set) in [
        ClientSet::Load,
        ClientSet::Receive,
        ClientSet::Reconcile,
        ClientSet::Input,
        ClientSet::Predict,
        ClientSet::Send,
        ClientSet::Present,
        ClientSet::Ui,
        ClientSet::Effects,
        ClientSet::Diag,
    ]
    .into_iter()
    .enumerate()
    {
        let index = index as u8;
        app.configure_sets(
            if index < 2 {
                PreUpdate.intern()
            } else {
                Update.intern()
            },
            (ClientEdge(index), set.clone(), ClientEdge(index + 1)).chain(),
        );
    }
    app.configure_sets(Update, FxSoundPublished.in_set(ClientSet::Effects));
    app.configure_sets(Update, OwnerEventsPublished.in_set(ClientSet::Predict));
    configure_worker_cmd_sets(app);
}

pub fn configure_worker_cmd_sets(app: &mut App) {
    app.configure_sets(
        Update,
        (
            WorkerCmdSet::DynamicModels,
            WorkerCmdSet::SceneVisibility,
            WorkerCmdSet::WorldVisibility,
            WorkerCmdSet::ModelPose,
        )
            .in_set(ClientSet::Present),
    );
    app.configure_sets(Update, ModelLightingSeated.in_set(ClientSet::Present));
}
