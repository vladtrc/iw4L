use std::collections::HashSet;

use assets::{
    AssetNamespace, LoadingScreen, MatchType10SoundHints, PreparedWeapons, PreparedXAnims,
    SoundCatalog, WeaponRegistry,
};
use bevy::prelude::*;
use frame::{ClientSet, LaunchIdentity, MatchTornDown};

use crate::aliases::movement_prepare_names;
use crate::ambient::{SoundBankLoadAttempted, SoundBankNamespace, SoundBankWalk};
use crate::clip_store::{ClipKey, ClipStore, clip_keys_for_alias};
use crate::map_doors::RADIATION_DOOR_ALIASES;
use crate::playback::SoundBank;

#[derive(Resource, Default)]
pub struct AudioReady(pub bool);

const MATCH_HUD_PULSE: &[&str] = &["ui_pulse_text_type", "ui_pulse_text_delete"];

#[derive(Resource, Default)]
struct MatchClipPrep {
    submitted: bool,
    required: HashSet<ClipKey>,
    total: usize,
    stage: Option<assets::LoadStage>,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<AudioReady>()
        .init_resource::<MatchClipPrep>()
        .add_systems(
            Update,
            (
                queue_match_clips.after(crate::playback::stamp_weapon_sound_edges),
                poll_match_audio_ready.after(queue_match_clips),
                reset_match_audio_on_torn_down,
            )
                .in_set(ClientSet::Load),
        );
}

fn reset_match_audio_on_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut ready: ResMut<AudioReady>,
    mut prep: ResMut<MatchClipPrep>,
) {
    if torn.read().count() == 0 {
        return;
    }
    *ready = AudioReady(false);
    *prep = MatchClipPrep::default();
}

fn queue_match_clips(
    attempted: Res<SoundBankLoadAttempted>,
    walk: Option<Res<SoundBankWalk>>,
    mut clips: Option<ResMut<ClipStore>>,
    weapons: Option<Res<PreparedWeapons>>,
    xanims: Option<Res<PreparedXAnims>>,
    type10: Option<Res<MatchType10SoundHints>>,
    bank: Option<Res<SoundBank>>,
    identity: Option<Res<LaunchIdentity>>,
    catalog: Option<Res<assets::MenuCatalog>>,
    script_sound: Option<Res<assets::SessionMapScriptSound>>,
    namespace: Option<Res<SoundBankNamespace>>,
    loading: Option<Res<LoadingScreen>>,
    mut prep: ResMut<MatchClipPrep>,
    mut ready: ResMut<AudioReady>,
) {
    if ready.0 || prep.submitted {
        return;
    }
    if !attempted.0 {
        return;
    }
    if walk.is_some() {
        return;
    }
    let Some(clips) = clips.as_mut() else {
        ready.0 = true;
        diag::info!(
            Audio,
            "audio: AudioReady skipped — sound bank did not install"
        );
        return;
    };
    let Some(weapons) = weapons else {
        return;
    };
    let Some(type10) = type10 else {
        return;
    };
    let Some(bank) = bank else {
        return;
    };
    let Some(namespace) = namespace else {
        return;
    };
    let Some(script_sound) = script_sound else {
        return;
    };
    let mut required = HashSet::new();
    let mut aliases = 0usize;
    for weapon in 1..=weapons.0.len() as u32 {
        aliases += request_weapon_aliases(
            clips,
            &bank.0,
            &weapons.0,
            xanims.as_deref(),
            weapon,
            &mut required,
        );
    }
    for alias in movement_prepare_names() {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut required);
    }
    if let Some(identity) = identity.as_deref() {
        let ns = namespace.namespace;
        let zone = namespace.zone.as_str();
        if zone != identity.zone {
            return;
        }
        for emitter in bank.0.createfx_loop_sounds(ns, zone) {
            aliases += 1;
            request_named(clips, &bank.0, ns, &emitter.soundalias, &mut required);
        }
    }
    for alias in RADIATION_DOOR_ALIASES {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::T5, alias, &mut required);
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut required);
    }
    if let Some(identity) = identity.as_deref() {
        let (allies, axis) = crate::policy::music::voice_prefixes_for_zone(
            catalog.as_deref(),
            Some(bank.as_ref()),
            identity,
        );
        for alias in
            crate::policy::music::match_script_alias_names(allies.as_deref(), axis.as_deref())
        {
            aliases += 1;
            request_named(clips, &bank.0, AssetNamespace::Iw4, &alias, &mut required);
        }
        for alias in
            crate::policy::music::match_voice_alias_names(allies.as_deref(), axis.as_deref())
        {
            aliases += 1;
            request_named(clips, &bank.0, AssetNamespace::Iw4, &alias, &mut required);
        }
    }
    if let Some(alias) = script_sound.0.ambient_alias.as_deref() {
        aliases += 1;
        request_named(clips, &bank.0, namespace.namespace, alias, &mut required);
    }
    for alias in MATCH_HUD_PULSE.iter().chain(crate::objectives::EFFECTS) {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut required);
    }
    for alias in &type10.0 {
        aliases += 1;
        request_fx_type10(clips, &bank.0, alias, &mut required);
    }
    prep.total = required.len();
    prep.required = required;
    prep.submitted = true;
    if let Some(loading) = loading {
        let stage = loading.progress.stage("preparing match audio");
        stage.total(prep.total as u64);
        prep.stage = Some(stage);
    }
    diag::info!(
        Audio,
        "audio: match-set queued {} aliases ({} type-10), {} clips still converting ({} workers)",
        aliases,
        type10.0.len(),
        prep.total,
        clips.workers()
    );
    if prep.total == 0 {
        mark_ready(&mut ready, &mut prep, Some(&mut **clips));
    }
}

fn poll_match_audio_ready(
    mut clips: Option<ResMut<ClipStore>>,
    mut prep: ResMut<MatchClipPrep>,
    mut ready: ResMut<AudioReady>,
) {
    if ready.0 || !prep.submitted {
        return;
    }
    let Some(clips) = clips.as_mut() else {
        return;
    };
    prep.required.retain(|key| clips.ready(key).is_none());
    let done = prep.total.saturating_sub(prep.required.len());
    if let Some(stage) = prep.stage.as_ref() {
        stage.set_done(done as u64);
    }
    if prep.required.is_empty() {
        mark_ready(&mut ready, &mut prep, Some(&mut **clips));
    }
}

fn mark_ready(ready: &mut AudioReady, prep: &mut MatchClipPrep, clips: Option<&mut ClipStore>) {
    let _ = prep.stage.take();
    if !ready.0 {
        ready.0 = true;
        diag::info!(Audio, "audio: AudioReady ({} clips prepared)", prep.total);
        if let Some(clips) = clips {
            clips.arm_match_live();
        }
    }
}

fn request_named(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
    required: &mut HashSet<ClipKey>,
) {
    for key in clip_keys_for_alias(bank, ns, alias) {
        clips.request(key.clone());
        if clips.ready(&key).is_none() {
            required.insert(key);
        }
    }
}

fn request_fx_type10(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    alias: &str,
    required: &mut HashSet<ClipKey>,
) {
    let ns = bank
        .index_in(AssetNamespace::Iw4, alias)
        .or_else(|| bank.index_unique(alias))
        .map(|index| bank.namespace_of_alias(index))
        .unwrap_or(AssetNamespace::Iw4);
    request_named(clips, bank, ns, alias, required);
}

fn request_weapon_aliases(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    registry: &WeaponRegistry,
    xanims: Option<&PreparedXAnims>,
    weapon: u32,
    required: &mut HashSet<ClipKey>,
) -> usize {
    let sounds = registry.sounds_of(weapon);
    let ns = registry.namespace_of(weapon).unwrap_or(AssetNamespace::Iw4);
    let mut n = 0usize;
    if let Some(aliases) = sounds {
        for alias in aliases.reachable_aliases() {
            n += 1;
            request_named(clips, bank, ns, alias, required);
        }
    }
    if let Some(xanims) = xanims {
        let notes = weapon_anim_notes(registry, &xanims.0, weapon);
        for alias in note_aliases_to_prepare(notes.iter().map(String::as_str), sounds) {
            n += 1;
            request_named(clips, bank, ns, &alias, required);
        }
    }
    n
}

fn weapon_anim_notes(
    registry: &WeaponRegistry,
    xanims: &assets::XAnimCatalog,
    weapon: u32,
) -> Vec<String> {
    let ns = registry.namespace_of(weapon).unwrap_or(AssetNamespace::Iw4);
    let mut seen_anim = HashSet::new();
    let mut seen_note = HashSet::new();
    let mut out = Vec::new();
    let mut take_clip = |clip: &assets::AnimClip| {
        if !seen_anim.insert(clip.name.clone()) {
            return;
        }
        for note in &clip.notifies {
            if seen_note.insert(note.name.clone()) {
                out.push(note.name.clone());
            }
        }
    };
    if let Some(row) = registry.sz_xanim_edges_of(weapon) {
        for edge in row {
            let Some(order) = edge.bound_index() else {
                continue;
            };
            if let Some(clip) = xanims.clip_at(order) {
                take_clip(&clip);
            }
        }
    }
    for leftover in [
        registry.sz_xanims_right_of(weapon),
        registry.sz_xanims_left_of(weapon),
    ]
    .into_iter()
    .flatten()
    .flatten()
    .flatten()
    {
        if let Some(clip) = xanims.clip(ns, leftover) {
            take_clip(&clip);
        }
    }
    out
}

fn note_aliases_to_prepare<'a>(
    notes: impl Iterator<Item = &'a str>,
    sounds: Option<&assets::WeaponSoundAliases>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for note in notes {
        let (_, sound) = crate::entity_events::notetrack_rumble_and_sound(note, sounds);
        if let Some(alias) = sound
            && seen.insert(alias.clone())
        {
            out.push(alias);
        }
    }
    out
}
