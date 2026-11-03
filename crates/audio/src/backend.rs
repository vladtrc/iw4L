use bevy::prelude::*;
use frame::ClientSet;

pub(crate) use crate::render_core::AudioScope;
use crate::runtime::AudioRuntime;

use frame::MatchScope;

pub(crate) fn register(app: &mut App) {
    app.add_systems(OnExit(frame::Timeline::Live), stop_timeline_audio)
        .add_systems(OnExit(frame::Timeline::Killcam), stop_timeline_audio);

    app.add_systems(
        OnExit(MatchScope::Live),
        release_audio.in_set(frame::ScopeSet::Release),
    );
    app.init_resource::<AudioRuntime>();
    app.add_systems(
        Update,
        publish_audio_context
            .in_set(ClientSet::Predict)
            .after(frame::OwnerEventsPublished),
    )
    .add_systems(
        PostUpdate,
        (
            submit_presented_audio,
            cancel_audio_on_exit.after(submit_presented_audio),
        ),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AudioPresentation<'w> {
    presented: Option<Res<'w, net::PresentedSnapshot>>,
    local: Option<Res<'w, net::LocalPresentClient>>,
    adopted: Option<Res<'w, net::LastAdoptedSnapshot>>,
    prediction: Option<Res<'w, net::ClientPredictionState>>,
    view: Option<Res<'w, frame::ViewSubject>>,
}

impl AudioPresentation<'_> {
    fn event_context(&self) -> Option<crate::event::EventContext> {
        self.presented
            .as_ref()
            .and_then(|presented| {
                presented.snapshot().map(|snapshot| {
                    if self.view.as_ref().is_some_and(|view| view.in_killcam()) {
                        self.local
                            .as_ref()
                            .map_or(snapshot.tick, |local| snapshot.view_tick(local.0))
                            .0
                    } else {
                        self.adopted
                            .as_ref()
                            .and_then(|adopted| adopted.next())
                            .map_or(snapshot.tick.0, |latest| latest.tick.0.max(snapshot.tick.0))
                    }
                })
            })
            .map(|tick| crate::event::EventContext {
                killcam: self.view.as_ref().is_some_and(|view| view.in_killcam()),
                local_life: self.local.as_ref().and_then(|local| {
                    let snapshot = if self.view.as_ref().is_some_and(|view| view.in_killcam()) {
                        self.presented.as_ref()?.snapshot()?
                    } else {
                        self.adopted
                            .as_ref()
                            .and_then(|adopted| adopted.next())
                            .or_else(|| self.presented.as_ref()?.snapshot())?
                    };
                    let meta = snapshot.meta.for_client(local.0)?;
                    (meta.lifecycle == sim::ClientLifecycle::Alive)
                        .then_some((local.0.0, meta.life_sequence.0))
                }),
                tick,
                owner_tick: self
                    .prediction
                    .as_ref()
                    .and_then(|prediction| prediction.0.history().newest())
                    .map(|latest| latest.tick.0),
            })
    }

    fn snapshot(&self) -> Option<&sim::Snapshot> {
        self.presented.as_deref().and_then(|p| p.snapshot())
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct AudioMediaSource<'w> {
    clips: Option<Res<'w, crate::ClipStore>>,
    frontend: Option<Res<'w, crate::frontend::FrontendAudio>>,
}

impl AudioMediaSource<'_> {
    fn service(&self) -> Option<crate::clip_store::MediaService> {
        self.clips
            .as_ref()
            .map(|clips| clips.service())
            .or_else(|| {
                self.frontend
                    .as_ref()
                    .map(|frontend| frontend.clips.service())
            })
    }
}

pub(crate) fn publish_audio_context(
    runtime: Res<AudioRuntime>,
    presentation: AudioPresentation,
    media: AudioMediaSource,
    mix: Option<Res<crate::script_mix::ScriptAudioMix>>,
    channels: Option<Res<crate::script_mix::ChannelAudioMix>>,
    listeners: Query<&Transform, With<crate::AmbientListener>>,
    level: Option<Res<net::ClientLevel>>,
) {
    runtime.set_event_context(presentation.event_context());
    runtime.set_media_service(media.service());
    runtime.set_cue_mix(
        mix.zip(channels)
            .map(|(mix, channels)| crate::cue_execution::CueMix {
                gain: mix.gain.clone(),
                channels: channels.bindings(),
            }),
    );
    let mut listener = listeners.iter();
    let pose = listener.next();
    assert!(listener.next().is_none(), "more than one ambient listener");
    runtime.set_listener(
        pose.map(|pose| listener_snapshot(pose, level.as_deref(), presentation.snapshot())),
    );
}

fn cancel_audio_on_exit(mut exit: MessageReader<AppExit>, runtime: Res<AudioRuntime>) {
    if exit.read().count() == 0 {
        return;
    }
    runtime.cancel_all();
    diag::info!(
        Audio,
        "audio: admissions queue_full={} logical_budget={} physical_budget={} concurrency={} cancelled={} stale_scope={} output_unavailable={}",
        runtime.rejection_count(crate::AdmissionFailure::QueueFull),
        runtime.rejection_count(crate::AdmissionFailure::LogicalBudget),
        runtime.rejection_count(crate::AdmissionFailure::PhysicalBudget),
        runtime.rejection_count(crate::AdmissionFailure::Concurrency),
        runtime.rejection_count(crate::AdmissionFailure::Cancelled),
        runtime.rejection_count(crate::AdmissionFailure::StaleScope),
        runtime.rejection_count(crate::AdmissionFailure::OutputUnavailable)
    );
    let stats = runtime.diagnostics();
    diag::info!(
        Audio,
        "audio: exit frames={} device_blocks={} null_blocks={} busy_blocks={} underruns={} peak={:.6} source_revision={} sources={} source_pending_layers={} source_rendered={} source_virtual={} source_dropped={}",
        stats.audio_frame,
        stats.device_blocks,
        stats.null_blocks,
        stats.busy_blocks,
        stats.device_underruns,
        stats.peak,
        stats.source_revision,
        stats.logical_sources,
        stats.pending_source_layers,
        stats.rendered_sources,
        stats.virtual_sources,
        stats.dropped_sources
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct PresentedAudioSources<'w, 's> {
    destructibles: Option<Res<'w, crate::destructible_loops::DestructibleSources>>,
    projectiles: Query<'w, 's, &'static crate::projectile_loops::ProjectileLoop>,
    flames: Query<'w, 's, &'static crate::flame_loops::FlameStream>,
    map: Option<Res<'w, crate::ambient::MapSources>>,
    menu: Option<Res<'w, crate::frontend::MenuSources>>,
    shellshock: Option<Res<'w, crate::shellshock::ShellshockSources>>,
    breath: Option<Res<'w, crate::breath::BreathSources>>,
}

impl PresentedAudioSources<'_, '_> {
    fn collect(self) -> Vec<crate::sources::DesiredSource> {
        let mut desired = Vec::new();
        if let Some(destructibles) = self.destructibles {
            desired.extend(destructibles.desired.iter().cloned());
        }
        desired.extend(self.projectiles.iter().map(|source| source.source.clone()));
        desired.extend(
            self.flames
                .iter()
                .filter_map(|stream| stream.source.clone()),
        );
        if let Some(map) = self.map {
            desired.extend(map.desired.iter().cloned());
        }
        if let Some(menu) = self.menu {
            desired.extend(menu.source.iter().cloned());
        }
        if let Some(shellshock) = self.shellshock {
            desired.extend(shellshock.source.iter().cloned());
        }
        if let Some(breath) = self.breath {
            desired.extend(breath.source.iter().cloned());
        }
        desired
    }
}

fn submit_presented_audio(
    mut runtime: ResMut<AudioRuntime>,
    sources: PresentedAudioSources,
    settings: Option<Res<frame::GameSettings>>,
    listeners: Query<&Transform, With<crate::AmbientListener>>,
    level: Option<Res<net::ClientLevel>>,
    presented: Option<Res<net::PresentedSnapshot>>,
) {
    let mut listener = listeners.iter();
    let pose = listener.next();
    assert!(listener.next().is_none(), "more than one ambient listener");
    runtime.set_listener(pose.map(|pose| {
        listener_snapshot(
            pose,
            level.as_deref(),
            presented.as_deref().and_then(|p| p.snapshot()),
        )
    }));
    if let Some(settings) = settings {
        runtime.set_master_volume(settings.master_volume);
    }
    let desired = sources.collect();
    runtime.set_sources(desired);
}

fn release_audio(runtime: Res<AudioRuntime>) {
    runtime.set_media_service(None);
    runtime.set_event_context(None);
    runtime.set_cue_mix(None);
}

fn stop_timeline_audio(runtime: Res<AudioRuntime>) {
    runtime.stop_scope(AudioScope::Match);
}

fn listener_snapshot(
    pose: &Transform,
    level: Option<&net::ClientLevel>,
    snapshot: Option<&sim::Snapshot>,
) -> crate::spatial::ListenerSnapshot {
    let origin_inches = crate::transform_inches(pose.translation);
    let portals = level
        .and_then(|level| level.content.atlas())
        .filter(|atlas| !atlas.portals.is_empty())
        .map(|atlas| {
            std::sync::Arc::new(crate::portal_space::PortalSpace::new(
                atlas,
                snapshot.map_or(&[], |s| {
                    s.meta.world_objects.portal_blocked_exits.as_slice()
                }),
                origin_inches,
            ))
        });
    crate::spatial::ListenerSnapshot {
        origin_inches,
        right: (pose.rotation * Vec3::X).to_array(),
        portals,
    }
}
