use std::sync::Arc;

use assets::{GamesRoot, NamespaceSoundIwd, NamespaceTrees, SoundCatalog, load_mp_sound_bank};
use bevy::{
    audio::Volume,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{
    ClientSet, LaunchIdentity, ReturnedToMenu, UiPlayMusic, UiPlaySound, UiStopMusic,
    register_ui_sound,
};

use crate::{
    ClipStore, SoundClass,
    ambient::SoundIwd,
    clip_store::{PendingStarts, clip_keys_for_alias},
    pcm::{LoopingPcmAudio, PcmAudio},
    playback::{MissingAliasGaps, SharedPlayAssets, SoundBank, SoundPickState, play_alias_oneshot},
    start::StartDecisions,
};

#[derive(Resource, Clone)]
pub struct FrontendAudio {
    pub bank: Arc<SoundCatalog>,
    pub iwd: Arc<NamespaceSoundIwd>,
}

const FRONTEND_SOUND_ZONE: &str = "iw4:code_post_gfx_mp";

#[derive(Resource)]
struct FrontendAudioPrepare(Task<FrontendAudioWalked>);

struct FrontendAudioWalked {
    bank: Option<Arc<SoundCatalog>>,
    iwd: Arc<NamespaceSoundIwd>,
    lines: Vec<String>,
}

#[derive(Component)]
struct MenuMusicBed {
    alias: String,
}

#[derive(Resource, Default)]
struct PendingMenuBed {
    alias: Option<String>,
}

pub(crate) fn register_frontend_audio(app: &mut App) {
    register_ui_sound(app);
    app.init_resource::<PendingMenuBed>()
        .add_message::<ReturnedToMenu>()
        .add_systems(
            Update,
            (
                start_frontend_audio_prepare,
                install_frontend_audio_prepare.after(start_frontend_audio_prepare),
                restore_frontend_audio_on_menu
                    .after(install_frontend_audio_prepare)
                    .after(crate::ambient::stop_map_ambient_on_match_end),
            )
                .in_set(ClientSet::Load),
        )
        .add_systems(
            Update,
            (
                play_ui_sound_messages.after(crate::voice::reclaim_finished_voices),
                play_ui_music_messages,
            )
                .in_set(ClientSet::Effects),
        );
}

fn start_frontend_audio_prepare(
    frontend: Option<Res<FrontendAudio>>,
    running: Option<Res<FrontendAudioPrepare>>,
    identity: Option<Res<LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    loading: (
        Option<Res<assets::MatchLoadBusy>>,
        Option<Res<assets::MatchLoadRequest>>,
        Option<Res<assets::MatchLoadAccepted>>,
        Option<Res<assets::PreparedMatchReady>>,
        Option<Res<crate::ambient::SoundBankCompose>>,
    ),
    mut commands: Commands,
) {
    if frontend.is_some() || running.is_some() || silent.is_some() {
        return;
    }
    let (busy, request, accepted, ready, walk) = loading;
    if busy.is_some_and(|busy| busy.0)
        || request.is_some()
        || accepted.is_some()
        || ready.is_some()
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
    commands.insert_resource(FrontendAudio {
        bank,
        iwd: walked.iwd,
    });
    diag::info!(Audio, "audio: frontend sound resident");
}

pub(crate) fn restore_frontend_audio_on_menu(
    mut returned: MessageReader<ReturnedToMenu>,
    frontend: Option<Res<FrontendAudio>>,
    live: Option<Res<SoundBank>>,
    silent: Option<Res<crate::AudioSilent>>,
    mut commands: Commands,
) {
    if returned.read().count() == 0 || silent.is_some() {
        return;
    }
    let Some(frontend) = frontend else {
        diag::warn!(
            Audio,
            "audio: no resident frontend bank — the menu runs without sound (typed gap)"
        );
        return;
    };
    if live.is_some_and(|live| Arc::ptr_eq(&live.0, &frontend.bank)) {
        return;
    }
    commands.insert_resource(PendingStarts::default());
    commands.insert_resource(SharedPlayAssets::default());
    commands.insert_resource(SoundBank(Arc::clone(&frontend.bank)));
    commands.insert_resource(SoundIwd(Arc::clone(&frontend.iwd)));
    commands.insert_resource(ClipStore::start(
        Arc::clone(&frontend.bank),
        Some(Arc::clone(&frontend.iwd)),
    ));
    diag::info!(Audio, "audio: frontend sound restored");
}

fn play_ui_sound_messages(
    mut events: MessageReader<UiPlaySound>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut pick: ResMut<SoundPickState>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pending: ResMut<PendingStarts>,
    mut occupancy: ResMut<crate::VoiceOccupancy>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    epoch: Res<crate::backend::MatchEpoch>,
) {
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
    let iwd = iwd.as_ref().map(|s| &s.0);
    for event in events.read() {
        if event.alias.is_empty() {
            continue;
        }
        let outcome = play_alias_oneshot(
            &mut commands,
            &mut pcm_assets,
            &mut shared,
            &bank.0,
            iwd.map(|a| a.as_ref()),
            assets::AssetNamespace::Iw4,
            &event.alias,
            None,
            None,
            &mut pick,
            clips.as_deref_mut(),
            &mut pending,
            &mut occupancy,
            &mut decisions,
            Some(crate::SND_ENT_LOCAL),
            SoundClass::Ui,
            epoch.0,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(&event.alias);
        }
    }
}

fn play_ui_music_messages(
    mut play_events: MessageReader<UiPlayMusic>,
    mut stop_events: MessageReader<UiStopMusic>,
    mut commands: Commands,
    mut looping_assets: ResMut<Assets<LoopingPcmAudio>>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut desired: ResMut<PendingMenuBed>,
    mut clips: Option<ResMut<ClipStore>>,
    bank: Option<Res<SoundBank>>,
    playing: Query<(Entity, &MenuMusicBed)>,
) {
    let stopped = stop_events.read().count() > 0;
    if stopped {
        desired.alias = None;
        for (entity, _) in &playing {
            crate::backend::stop(&mut commands, entity);
        }
        diag::info!(Audio, "audio: menu bed stopped");
    }
    for event in play_events.read() {
        if event.alias.is_empty() {
            continue;
        }
        desired.alias = Some(event.alias.clone());
    }
    let Some(alias) = desired.alias.clone() else {
        return;
    };
    let Some(bank) = bank else {
        return;
    };
    let Some(clips) = clips.as_mut() else {
        return;
    };
    if playing.iter().any(|(_, bed)| bed.alias == alias) {
        return;
    }
    for (entity, _) in playing.iter() {
        crate::backend::stop(&mut commands, entity);
    }
    let Some(key) = clip_keys_for_alias(&bank.0, assets::AssetNamespace::Iw4, &alias)
        .into_iter()
        .next()
    else {
        gaps.record(&alias);
        desired.alias = None;
        return;
    };
    clips.request(key.clone());
    let Some(pcm) = clips.ready(&key) else {
        return;
    };
    let Ok(pcm) = pcm else {
        gaps.record(&alias);
        desired.alias = None;
        return;
    };
    let handle = looping_assets.add(pcm.into_looping());
    let entity = crate::backend::spawn_loop(
        &mut commands,
        handle,
        Volume::Linear(0.55),
        0,
        crate::backend::AudioScope::Menu,
    );
    commands.entity(entity).insert(MenuMusicBed {
        alias: alias.clone(),
    });
    diag::info!(Audio, "audio: menu bed `{alias}`");
}
