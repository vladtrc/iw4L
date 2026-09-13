use std::sync::Arc;

use assets::{GamesRoot, NamespaceSoundIwd, NamespaceTrees, load_mp_sound_bank};
use bevy::{audio::Volume, prelude::*};
use frame::{
    ClientSet, LaunchIdentity, MatchTornDown, TeardownReason, UiPlayMusic, UiPlaySound,
    UiStopMusic, register_ui_sound,
};

use crate::{
    ClipStore, SoundClass,
    ambient::SoundIwd,
    clip_store::{PendingStarts, clip_keys_for_alias},
    pcm::{LoopingPcmAudio, PcmAudio},
    playback::{MissingAliasGaps, SharedPlayAssets, SoundBank, SoundPickState, play_alias_oneshot},
    start::StartDecisions,
};

const FRONTEND_SOUND_ZONE: &str = "code_post_gfx_mp";

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
        .add_systems(
            Update,
            restore_frontend_bank_on_disconnect
                .after(crate::ambient::stop_map_ambient_on_match_torn_down)
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

pub(crate) fn restore_frontend_bank_on_disconnect(
    mut torn: MessageReader<MatchTornDown>,
    identity: Option<Res<LaunchIdentity>>,
    mut commands: Commands,
) {
    if !torn
        .read()
        .any(|fact| fact.reason == TeardownReason::Disconnect)
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
    let bank = match load_mp_sound_bank(&games, FRONTEND_SOUND_ZONE) {
        Ok(loaded) => {
            for line in loaded.gap_lines() {
                diag::warn!(Audio, "audio: frontend {line}");
            }
            let bank = Arc::new(loaded.catalog);
            commands.insert_resource(SoundBank(Arc::clone(&bank)));
            diag::info!(Audio, "audio: frontend sound bank ready");
            Some(bank)
        }
        Err(error) => {
            diag::warn!(Audio, "audio: frontend sound bank: {error}");
            None
        }
    };

    let (indices, lines) = NamespaceSoundIwd::open(&NamespaceTrees::discover(&games));
    for line in lines {
        diag::info!(Audio, "audio: frontend {line}");
    }
    let iwd = Arc::new(indices);
    commands.insert_resource(SoundIwd(Arc::clone(&iwd)));
    if let Some(bank) = bank {
        commands.insert_resource(ClipStore::start(bank, Some(iwd)));
    }
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
        for event in events.read() {
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
