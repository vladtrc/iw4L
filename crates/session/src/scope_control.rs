use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use frame::ScopeApp;
use frame::{MatchScope, RoundPhase, ScopeSet, TeardownReason};

#[derive(Resource, Default)]
pub struct ScopeControl {
    pending: Option<(u8, MatchScope)>,
    install: Option<(frame::MatchKey, CommandQueue)>,
    pub(crate) teardown: Option<TeardownReason>,
    round: Option<RoundPhase>,
    last_reason: Option<TeardownReason>,
}

impl ScopeControl {
    fn request(&mut self, priority: u8, target: MatchScope) {
        if let Some((current, previous)) = self.pending {
            if current > priority {
                return;
            }
            assert!(
                current != priority || previous == target,
                "conflicting scope requests"
            );
        }
        self.pending = Some((priority, target));
    }

    pub fn begin_load(&mut self) {
        self.request(1, MatchScope::Loading);
    }

    pub(crate) fn install(&mut self, key: frame::MatchKey, install: CommandQueue) {
        assert!(self.install.is_none(), "two prepared installs in one frame");
        self.install = Some((key, install));
        self.request(2, MatchScope::Live);
    }

    pub fn tear_down(&mut self, reason: TeardownReason) {
        self.teardown = Some(reason);
        self.last_reason = Some(reason);
        self.request(
            3,
            if reason == TeardownReason::Replaced {
                MatchScope::Loading
            } else {
                MatchScope::Absent
            },
        );
    }

    pub fn return_to_menu(&mut self, reason: Option<TeardownReason>) {
        self.last_reason = reason;
        self.request(3, MatchScope::Absent);
    }
    pub fn exit_reason(&self) -> Option<TeardownReason> {
        self.last_reason
    }

    pub fn begin_round(&mut self) {
        self.round = Some(RoundPhase::Playing);
    }
    pub fn end_round(&mut self) {
        self.round = Some(RoundPhase::Between);
    }
}

pub(crate) fn accepts_install(world: &World, key: frame::MatchKey) -> bool {
    world.resource::<crate::SessionSwapRequest>().can_install()
        && (key.is_none()
            || world
                .get_resource::<net::MasterBridge>()
                .is_some_and(|bridge| {
                    let state = bridge.state();
                    state.in_match() && state.identity().match_key() == key
                }))
}

fn resolve_scope(world: &mut World) {
    let (pending, round) = {
        let mut control = world.resource_mut::<ScopeControl>();
        (control.pending.take(), control.round.take())
    };
    if let Some((_, target)) = pending {
        if target == MatchScope::Absent {
            if let Some(request) = world.remove_resource::<assets::MatchLoadRequest>() {
                request.progress.cancel();
                world.resource_mut::<frame::Retiring>().hand_over(request);
            }
            world
                .resource_mut::<Messages<frame::MapLoadApproved>>()
                .clear();
            world
                .resource_mut::<Messages<frame::MapLoadFailed>>()
                .clear();
        }
        if target != MatchScope::Live {
            if let Some(install) = world.resource_mut::<ScopeControl>().install.take() {
                world.resource_mut::<frame::Retiring>().hand_over(install);
            }
            world.resource_mut::<InstallAnnouncement>().1 = None;
            frame::scope::discard_staging(world);
            let announcement = std::mem::take(&mut world.resource_mut::<InstallAnnouncement>().0);
            world
                .resource_mut::<frame::Retiring>()
                .hand_over(announcement);
        }
        if target == MatchScope::Live {
            if let Some((key, mut install)) = world.resource_mut::<ScopeControl>().install.take() {
                if !accepts_install(world, key) {
                    world.resource_mut::<frame::Retiring>().hand_over(install);
                    return;
                }
                if world.resource::<frame::RuntimeRole>().runs_client()
                    && !world.contains_resource::<audio::AudioSilent>()
                    && audio::prepared_match_audio_key(world) != Some(key)
                {
                    world
                        .resource_mut::<Messages<frame::MapLoadFailed>>()
                        .write(frame::MapLoadFailed {
                            match_key: key,
                            zone: String::new(),
                            error: "Prepared audio does not belong to the current load".into(),
                        });
                    world.resource_mut::<frame::Retiring>().hand_over(install);
                    return;
                }
                world.resource_mut::<InstallAnnouncement>().1 = Some(key);
                install.apply(world);
            } else {
                return;
            }
        }
        world.resource_mut::<NextState<MatchScope>>().set(target);
    }
    if let Some(round) = round
        && world.contains_resource::<State<RoundPhase>>()
    {
        world.resource_mut::<NextState<RoundPhase>>().set(round);
    }
}

#[derive(Resource, Default)]
pub(crate) struct InstallAnnouncement(pub CommandQueue, pub Option<frame::MatchKey>);

fn announce_install(world: &mut World) {
    let (mut queue, key) = {
        let mut announcement = world.resource_mut::<InstallAnnouncement>();
        (std::mem::take(&mut announcement.0), announcement.1.take())
    };
    if key.is_none_or(|key| !accepts_install(world, key)) {
        world.resource_mut::<frame::Retiring>().hand_over(queue);
        world
            .resource_mut::<NextState<MatchScope>>()
            .set(MatchScope::Loading);
        return;
    }
    queue.apply(world);
}

pub(crate) fn register(app: &mut App) {
    if !app.is_plugin_added::<frame::ScopePlugin>() {
        app.add_plugins(frame::ScopePlugin);
    }
    app.staged::<assets::PreparedCapabilities>(MatchScope::Live)
        .staged::<frame::WorldProducts>(MatchScope::Live)
        .staged::<crate::LiveWorldIdentity>(MatchScope::Live)
        .staged::<render_frontend::prepare::scene::world::WorldScene>(MatchScope::Live)
        .staged::<crate::SessionContentManifest>(MatchScope::Live)
        .staged::<MatchUiImages>(MatchScope::Live)
        .scoped::<RoundClock>(frame::Timeline::Live)
        .init_resource::<ScopeControl>()
        .init_resource::<InstallAnnouncement>()
        .add_systems(
            Update,
            follow_round
                .in_set(frame::InMatch)
                .in_set(frame::ClientSet::Present)
                .after(net::publish_presented),
        )
        .add_systems(Last, resolve_scope.in_set(frame::scope::ScopeCommit))
        .add_systems(
            OnEnter(MatchScope::Live),
            publish_ui_images.in_set(ScopeSet::Derive),
        )
        .add_systems(
            OnExit(MatchScope::Live),
            release_ui_images.in_set(ScopeSet::Release),
        )
        .add_systems(
            OnEnter(MatchScope::Live),
            announce_install.in_set(ScopeSet::Announce),
        );
}

#[derive(Resource, Default)]
struct RoundClock(Option<(u32, bool)>);

fn follow_round(
    adopted: Res<net::LastAdoptedSnapshot>,
    mut clock: ResMut<RoundClock>,
    mut control: ResMut<ScopeControl>,
) {
    let Some(snapshot) = adopted.next() else {
        return;
    };
    let playing = snapshot.meta.phase == sim::MatchPhase::Playing;
    let next = (snapshot.meta.round_serial, playing);
    if clock.0 == Some(next) {
        return;
    }
    if playing {
        control.begin_round();
    } else {
        control.end_round();
    }
    clock.0 = Some(next);
}

#[derive(Resource)]
pub(crate) struct MatchUiImages(pub asset_material::UiImagePublication);

fn publish_ui_images(world: &mut World) {
    let publication = world.resource::<MatchUiImages>().0.clone();
    if let Some(previous) = world.remove_resource::<asset_material::UiImagePublication>() {
        world.resource_mut::<frame::Retiring>().hand_over(previous);
    }
    world.insert_resource(publication);
}

fn release_ui_images(world: &mut World) {
    if let Some(publication) = world.remove_resource::<asset_material::UiImagePublication>() {
        world
            .resource_mut::<frame::Retiring>()
            .hand_over(publication);
    }
}
