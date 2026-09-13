use assets::AssetNamespace;
use bevy::{audio::Volume, prelude::*};
use frame::{AppScreen, LifeEnded, MatchTornDown};
use hud_iw4::{shellshock_remaining_ms, shellshock_sound_parms};
use net::{CgFrameClock, LocalPresentClient, PresentedSnapshot};

use crate::{
    PlayAlias, SND_ENT_LOCAL,
    clip_store::{ClipStore, clip_keys_for_alias},
    pcm::LoopingPcmAudio,
    playback::{MissingAliasGaps, SoundBank},
};

#[derive(Component)]
pub(crate) struct ShellshockTinnitus {
    alias: String,
}

pub(crate) fn update_shellshock_tinnitus(
    mut torn: MessageReader<MatchTornDown>,
    mut died: MessageReader<LifeEnded>,
    screen: Option<Res<AppScreen>>,
    cg_clock: Option<Res<CgFrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    bank: Option<Res<SoundBank>>,
    mut clips: Option<ResMut<ClipStore>>,
    mut looping_assets: ResMut<Assets<LoopingPcmAudio>>,
    mut commands: Commands,
    mut play: MessageWriter<crate::AliasCommand>,
    mut gaps: ResMut<MissingAliasGaps>,
    epoch: Res<crate::backend::MatchEpoch>,
    playing: Query<(Entity, &ShellshockTinnitus)>,
    mut was_active: Local<bool>,
) {
    let torn = torn.read().next().is_some();
    let local_id = local.as_ref().map(|l| l.0.0);
    let died = died
        .read()
        .any(|ev| local_id.is_some_and(|id| ev.client == id));
    if torn || !screen.is_some_and(|s| matches!(*s, AppScreen::InGame)) {
        stop_loop(&mut commands, &playing);
        *was_active = false;
        return;
    }
    let Some(cg_clock) = cg_clock else {
        return;
    };
    let Some(presented) = presented else {
        return;
    };
    let Some(local) = local else {
        return;
    };
    let alive = presented.alive_player(local.0).is_some();
    let remaining = presented
        .player(local.0)
        .map(|ps| {
            shellshock_remaining_ms(cg_clock.time(), ps.shellshock_time, ps.shellshock_duration)
        })
        .unwrap_or(0);
    let parms = presented
        .player(local.0)
        .map(|ps| shellshock_sound_parms(ps.shellshock_index))
        .unwrap_or_else(|| shellshock_sound_parms(0));
    let want = remaining > 0 && parms.affect && alive && !died;
    if want {
        ensure_loop(
            &mut commands,
            &playing,
            bank.as_deref(),
            clips.as_deref_mut(),
            &mut looping_assets,
            &mut gaps,
            epoch.0,
            parms.loop_alias,
        );
        *was_active = true;
        return;
    }
    if *was_active {
        stop_loop(&mut commands, &playing);
        let alias = if died || !alive {
            parms.abort_alias
        } else {
            parms.end_alias
        };
        play.write(crate::AliasCommand::Play(PlayAlias {
            namespace: AssetNamespace::Iw4,
            alias: alias.to_owned(),
            fallback: None,
            origin_inches: None,
            snd_ent: Some(SND_ENT_LOCAL),
        }));
    }
    *was_active = false;
}

fn stop_loop(commands: &mut Commands, playing: &Query<(Entity, &ShellshockTinnitus)>) {
    for (entity, _) in playing.iter() {
        crate::backend::stop(commands, entity);
    }
}

fn ensure_loop(
    commands: &mut Commands,
    playing: &Query<(Entity, &ShellshockTinnitus)>,
    bank: Option<&SoundBank>,
    clips: Option<&mut ClipStore>,
    looping_assets: &mut Assets<LoopingPcmAudio>,
    gaps: &mut MissingAliasGaps,
    epoch: u64,
    alias: &str,
) {
    if playing.iter().any(|(_, bed)| bed.alias == alias) {
        return;
    }
    for (entity, _) in playing.iter() {
        crate::backend::stop(commands, entity);
    }
    let Some(bank) = bank else {
        return;
    };
    let Some(clips) = clips else {
        return;
    };
    let Some(key) = clip_keys_for_alias(&bank.0, AssetNamespace::Iw4, alias)
        .into_iter()
        .next()
    else {
        gaps.record(alias);
        return;
    };
    clips.request(key.clone());
    let Some(pcm) = clips.ready(&key) else {
        return;
    };
    let Ok(pcm) = pcm else {
        gaps.record(alias);
        return;
    };
    let handle = looping_assets.add(pcm.into_looping());
    let entity = crate::backend::spawn_loop(
        commands,
        handle,
        Volume::Linear(1.0),
        epoch,
        crate::backend::AudioScope::Match,
    );
    commands.entity(entity).insert(ShellshockTinnitus {
        alias: alias.to_owned(),
    });
}
