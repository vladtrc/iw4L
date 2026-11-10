use std::sync::Arc;

use asset_audio::{SoundCatalog, load_mp_sound_bank};
use asset_transport::GamesRoot;
use assets::{NamespaceSoundIwd, NamespaceTrees};
use bevy::ecs::system::SystemParam;
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{
    ClientSet, LaunchIdentity, UiPlayMusic, UiPlaySound, UiStopMusic, register_ui_contracts,
};

use crate::{
    ClipStore, SoundClass,
    clip_store::CueFeedback,
    playback::{MissingAliasGaps, SoundBank, play_oneshot_recorded},
    start::StartDecisions,
};

#[derive(Resource)]
pub struct FrontendAudio {
    pub bank: Arc<SoundCatalog>,
    pub clips: ClipStore,
}

const FRONTEND_SOUND_ZONE: &str = "iw4:code_post_gfx_mp";

#[derive(Resource)]
struct FrontendAudioPrepare(Task<FrontendAudioWalked>);

struct FrontendAudioWalked {
    bank: Option<Arc<SoundCatalog>>,
    iwd: Arc<NamespaceSoundIwd>,
    lines: Vec<String>,
}

#[derive(Resource, Default)]
pub(crate) struct MenuSources {
    alias: Option<String>,
    next_version: u64,
    pub source: Option<crate::sources::DesiredSource>,
}

pub(crate) fn register_frontend_audio(app: &mut App) {
    register_ui_contracts(app);
    app.init_resource::<MenuSources>()
        .init_resource::<FrontendCueFeedback>()
        .add_systems(
            PreUpdate,
            (
                start_frontend_audio_prepare,
                install_frontend_audio_prepare.after(start_frontend_audio_prepare),
            )
                .in_set(ClientSet::Load),
        )
        .add_systems(
            Update,
            (play_ui_sound_messages, play_ui_music_messages).in_set(ClientSet::Effects),
        );
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn start_frontend_audio_prepare(
    frontend: Option<Res<FrontendAudio>>,
    running: Option<Res<FrontendAudioPrepare>>,
    identity: Option<Res<LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    loading: (
        Option<Res<assets::MatchLoadBusy>>,
        Option<Res<assets::MatchLoadRequest>>,
        Option<Res<assets::MatchLoadAccepted>>,
        Option<Res<crate::ambient::SoundBankCompose>>,
    ),
    scope: Res<State<frame::MatchScope>>,
    mut commands: Commands,
) {
    if frontend.is_some() || running.is_some() || silent.is_some() {
        return;
    }
    let (busy, request, accepted, walk) = loading;
    if busy.is_some_and(|busy| busy.0)
        || request.is_some()
        || accepted.is_some()
        || *scope.get() == frame::MatchScope::Loading
        || walk.is_some()
    {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if identity.games_root.as_os_str().is_empty() {
        return;
    }
    let games = GamesRoot(identity.games_root.clone());
    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move {
        let mut lines = Vec::new();
        let bank = match load_mp_sound_bank(&games, FRONTEND_SOUND_ZONE) {
            Ok(loaded) => {
                lines.extend(loaded.gap_lines());
                Some(Arc::new(loaded.catalog))
            }
            Err(error) => {
                lines.push(format!("frontend sound bank: {error}"));
                None
            }
        };
        let (indices, open_lines) = NamespaceSoundIwd::open(&NamespaceTrees::discover(&games));
        lines.extend(open_lines);
        FrontendAudioWalked {
            bank,
            iwd: Arc::new(indices),
            lines,
        }
    });
    commands.insert_resource(FrontendAudioPrepare(task));
}

fn install_frontend_audio_prepare(
    mut prepare: Option<ResMut<FrontendAudioPrepare>>,
    mut commands: Commands,
) {
    let Some(prepare) = prepare.as_deref_mut() else {
        return;
    };
    let Some(walked) = future::block_on(future::poll_once(&mut prepare.0)) else {
        return;
    };
    commands.remove_resource::<FrontendAudioPrepare>();
    for line in walked.lines {
        diag::info!(Audio, "audio: frontend {line}");
    }
    let Some(bank) = walked.bank else {
        diag::warn!(
            Audio,
            "audio: frontend sound bank did not build — the menu stays silent (typed gap)"
        );
        return;
    };
    let clips = ClipStore::start(Arc::clone(&bank), Some(walked.iwd));
    commands.insert_resource(FrontendAudio { bank, clips });
    diag::info!(Audio, "audio: frontend sound resident");
}

#[derive(Resource, Default)]
struct FrontendCueFeedback(CueFeedback);

#[derive(SystemParam)]
struct UiSoundBank<'w> {
    bank: Option<Res<'w, SoundBank>>,
    frontend: Option<Res<'w, FrontendAudio>>,
}

impl UiSoundBank<'_> {
    fn resolve(&self) -> Option<&Arc<SoundCatalog>> {
        self.bank
            .as_ref()
            .map(|bank| &bank.0)
            .or_else(|| self.frontend.as_ref().map(|frontend| &frontend.bank))
    }
}

fn play_ui_sound_messages(
    mut events: MessageReader<UiPlaySound>,
    runtime: Res<crate::AudioRuntime>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut pending: ResMut<FrontendCueFeedback>,
    mut decisions: ResMut<StartDecisions>,
    sound_bank: UiSoundBank,
    namespace: Option<Res<crate::ambient::SoundBankNamespace>>,
) {
    let bank = sound_bank.resolve();
    let Some(bank) = bank else {
        for event in events.read().filter(|_| !crate::AudioSilent::active()) {
            diag::warn!(
                Audio,
                "audio: UiPlaySound `{}` dropped — no SoundBank (typed gap)",
                event.alias
            );
        }
        return;
    };
    for event in events.read() {
        if event.alias.is_empty() {
            continue;
        }
        let (ns, alias) = crate::aliases::match_ui_alias(
            namespace
                .as_deref()
                .map_or(asset_core::FamilyId::IW4, |ns| ns.namespace),
            &event.alias,
        );
        let outcome = play_oneshot_recorded(
            &runtime,
            &mut pending.0,
            &mut decisions,
            crate::cue_execution::CueTrigger {
                force_master: false,
                event: None,
                bank: Arc::clone(bank),
                namespace: ns,
                alias: alias.to_owned(),
                bound: None,
                origin_inches: None,
                emitter: Some(crate::SND_ENT_LOCAL),
                class: SoundClass::Ui,
                pitch_scale: 1.0,
                volume_scale: 1.0,
                fallbacks: Vec::new(),
            },
        );
        if outcome.allows_binding_fallback() {
            gaps.record(&event.alias);
        }
    }
}

fn play_ui_music_messages(
    mut play_events: MessageReader<UiPlayMusic>,
    mut stop_events: MessageReader<UiStopMusic>,
    mut desired: ResMut<MenuSources>,
    sound_bank: UiSoundBank,
    runtime: Res<crate::AudioRuntime>,
    scope: Res<State<frame::MatchScope>>,
) {
    if *scope.get() != frame::MatchScope::Absent {
        play_events.clear();
        stop_events.clear();
        desired.alias = None;
        desired.source = None;
        return;
    }
    if stop_events.read().count() > 0 {
        desired.alias = None;
        desired.source = None;
    }
    for event in play_events.read() {
        if !event.alias.is_empty() {
            desired.alias = Some(event.alias.clone());
        }
    }
    let Some(alias) = desired.alias.clone() else {
        return;
    };
    if desired
        .source
        .as_ref()
        .is_some_and(|source| source.cue.alias == alias)
    {
        return;
    }
    desired.source = None;
    let bank = sound_bank.resolve();
    let Some(bank) = bank else {
        return;
    };
    let Some(cue) = runtime.source_cue(crate::sources::SourceCueRequest {
        bank: bank.clone(),
        namespace: asset_core::FamilyId::IW4,
        alias,
        emitter: None,
        scope: crate::backend::AudioScope::Menu,
        group: None,
    }) else {
        return;
    };
    desired.next_version = desired
        .next_version
        .checked_add(1)
        .expect("source version exhausted");
    desired.source = Some(crate::sources::DesiredSource {
        key: crate::sources::SourceKey {
            scope: crate::backend::AudioScope::Menu,
            object: 0,
            slot: 4,
        },
        version: desired.next_version,
        cue,
        origin_inches: None,
        start_frame: runtime.audio_frame(),
        gain: 0.55,
        rate: 1.0,
        audible: true,
    });
}
