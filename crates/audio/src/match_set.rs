use std::collections::{BTreeSet, HashSet};

use assets::{
    AssetNamespace, MapLoadProcess, MatchType10SoundHints, PreparedWeapons, SoundCatalog,
    WeaponRegistry,
};
use bevy::prelude::*;
use frame::{ClientSet, LaunchIdentity, MatchTornDown, ReturnedToMenu};

use crate::aliases::movement_prepare_names;
use crate::ambient::{SoundBankCompose, SoundBankLoadAttempted, SoundBankNamespace};
use crate::clip_store::{ClipKey, ClipStore, clip_keys_for_alias};
use crate::map_doors::RADIATION_DOOR_ALIASES;
use crate::playback::SoundBank;

#[derive(Resource, Default)]
pub struct AudioReady(pub bool);

#[derive(Resource)]
pub struct AudioSilent;

impl AudioSilent {
    pub fn active() -> bool {
        static SILENT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *SILENT.get_or_init(|| {
            std::env::var("IW4L_SOUND").is_ok_and(|value| matches!(value.as_str(), "off" | "0"))
        })
    }
}

const MATCH_HUD_PULSE: &[&str] = &["ui_pulse_text_type", "ui_pulse_text_delete"];

const MENU_CODE: [&str; 2] = ["mouse_over", "mouse_click"];

const MATCH_CLOCK: &[&str] = &[gamemode_iw4::match_clock::COUNTDOWN_TICK_ALIAS];

#[derive(Default)]
struct MatchRequests {
    required: HashSet<ClipKey>,
    missing: BTreeSet<String>,
    resolved_aliases: usize,
}

#[derive(Resource, Default)]
struct MatchClipPrep {
    submitted: bool,
    required: HashSet<ClipKey>,
    /// Clips this match still had to convert when the set was queued — not the
    /// alias count, and not the whole cache.
    total: usize,
    stage: Option<assets::StageHandle>,
    /// Resident sample bytes this process had already produced when the set
    /// was queued. The decoders count for the life of the process, so what
    /// this match cost is the distance from here.
    sample_bytes_at_queue: u64,
}

/// Resident `f32` samples every decoder has produced so far.
fn prepared_sample_bytes() -> u64 {
    crate::clip_prep_cost()
        .paths
        .iter()
        .map(|(_, cost)| cost.sample_bytes)
        .sum()
}

pub(crate) fn register(app: &mut App) {
    if AudioSilent::active() {
        diag::info!(Audio, "audio: Silent (IW4L_SOUND=off)");
        app.insert_resource(AudioSilent);
    }
    app.init_resource::<AudioReady>()
        .init_resource::<MatchClipPrep>()
        .add_systems(
            Update,
            (
                queue_match_clips.after(crate::ambient::install_sound_bank),
                poll_match_audio_ready.after(queue_match_clips),
                reset_match_audio_on_match_end,
            )
                .in_set(ClientSet::Load),
        );
}

fn reset_match_audio_on_match_end(
    mut torn: MessageReader<MatchTornDown>,
    mut returned: MessageReader<ReturnedToMenu>,
    mut ready: ResMut<AudioReady>,
    mut prep: ResMut<MatchClipPrep>,
) {
    if torn.read().count() == 0 && returned.read().count() == 0 {
        return;
    }
    *ready = AudioReady(false);
    if let Some(stage) = prep.stage.take() {
        stage.cancel();
    }
    *prep = MatchClipPrep::default();
}

fn queue_match_clips(
    attempted: Res<SoundBankLoadAttempted>,
    walk: Option<Res<SoundBankCompose>>,
    mut clips: Option<ResMut<ClipStore>>,
    weapons: Option<Res<PreparedWeapons>>,
    type10: Option<Res<MatchType10SoundHints>>,
    bank: Option<Res<SoundBank>>,
    identity: Option<Res<LaunchIdentity>>,
    catalog: Option<Res<assets::MenuCatalog>>,
    script_sound: Option<Res<assets::SessionMapScriptSound>>,
    namespace: Option<Res<SoundBankNamespace>>,
    loading: Option<Res<MapLoadProcess>>,
    mut prep: ResMut<MatchClipPrep>,
    mut ready: ResMut<AudioReady>,
    silent: Option<Res<AudioSilent>>,
) {
    if ready.0 || prep.submitted {
        return;
    }
    if silent.is_some() {
        ready.0 = true;
        if let Some(loading) = loading.as_ref() {
            loading.progress.record_skipped(assets::StageId::Audio);
        }
        diag::info!(Audio, "audio: AudioReady — Silent, no clips prepared");
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
        if let Some(loading) = loading.as_ref() {
            loading.progress.record_skipped(assets::StageId::Audio);
        }
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
    let mut set = MatchRequests::default();
    let mut aliases = 0usize;
    for weapon in 1..=weapons.0.len() as u32 {
        aliases += request_weapon_aliases(clips, &bank.0, &weapons.0, weapon, &mut set);
    }
    for alias in movement_prepare_names() {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut set);
    }
    if let Some(identity) = identity.as_deref() {
        let ns = namespace.namespace;
        let zone = namespace.zone.as_str();
        if zone != identity.zone {
            return;
        }
        for emitter in bank.0.createfx_loop_sounds(ns, zone) {
            aliases += 1;
            request_named(clips, &bank.0, ns, &emitter.soundalias, &mut set);
        }
    }
    let mut destructible_loops: Vec<&str> = Vec::new();
    for alias in gamemode_iw4::destructible_loop_sound_aliases() {
        if destructible_loops.contains(&alias) {
            continue;
        }
        destructible_loops.push(alias);
        aliases += 1;
        request_named(clips, &bank.0, namespace.namespace, alias, &mut set);
    }
    for alias in RADIATION_DOOR_ALIASES {
        aliases += 1;
        let ns = match bank.0.index_in(AssetNamespace::T5, alias) {
            Some(_) => AssetNamespace::T5,
            None => AssetNamespace::Iw4,
        };
        request_named(clips, &bank.0, ns, alias, &mut set);
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
            request_named(clips, &bank.0, AssetNamespace::Iw4, &alias, &mut set);
        }
        for alias in
            crate::policy::music::match_voice_alias_names(allies.as_deref(), axis.as_deref())
        {
            aliases += 1;
            request_named(clips, &bank.0, AssetNamespace::Iw4, &alias, &mut set);
        }
    }
    if let Some(alias) = script_sound.0.ambient_alias.as_deref() {
        aliases += 1;
        request_named(clips, &bank.0, namespace.namespace, alias, &mut set);
    }
    if let Some(catalog) = catalog.as_deref() {
        for alias in catalog.played_sound_aliases().into_iter().chain(MENU_CODE) {
            aliases += 1;
            request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut set);
        }
    }
    for alias in MATCH_HUD_PULSE
        .iter()
        .chain(MATCH_CLOCK)
        .chain(crate::objectives::EFFECTS)
    {
        aliases += 1;
        request_named(clips, &bank.0, AssetNamespace::Iw4, alias, &mut set);
    }
    for alias in &type10.0 {
        aliases += 1;
        request_fx_type10(clips, &bank.0, alias, &mut set);
    }
    if !set.missing.is_empty() {
        let names: Vec<&str> = set.missing.iter().map(String::as_str).collect();
        diag::warn!(
            Audio,
            "audio: match-set gap: {} aliases name no loaded sound: {}",
            names.len(),
            names.join(" ")
        );
    }
    prep.total = set.required.len();
    prep.required = set.required;
    prep.submitted = true;
    if let Some(loading) = loading {
        prep.sample_bytes_at_queue = prepared_sample_bytes();
        if prep.total == 0 && set.resolved_aliases > 0 {
            loading
                .progress
                .record_reused_scoped(assets::StageId::Audio, "clips");
        } else {
            prep.stage = Some(
                loading
                    .progress
                    .begin(assets::StageId::Audio, Some(prep.total as u64)),
            );
        }
    }
    diag::info!(
        Audio,
        "audio: match-set queued {} aliases ({} type-10), {} clips still converting ({} workers); resident reused={} clips/{}B",
        aliases,
        type10.0.len(),
        prep.total,
        clips.workers(),
        clips.reused_resident().0,
        clips.reused_resident().1,
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
    let decoded = prepared_sample_bytes().saturating_sub(prep.sample_bytes_at_queue);
    if let Some(stage) = prep.stage.as_ref() {
        stage.set_completed(done as u64);
        stage.set_bytes(decoded);
    }
    if prep.required.is_empty() {
        mark_ready(&mut ready, &mut prep, Some(&mut **clips));
    }
}

fn mark_ready(ready: &mut AudioReady, prep: &mut MatchClipPrep, clips: Option<&mut ClipStore>) {
    if let Some(stage) = prep.stage.take() {
        stage.set_completed(prep.total as u64);
        stage.set_bytes(prepared_sample_bytes().saturating_sub(prep.sample_bytes_at_queue));
        stage.done();
    }
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
    set: &mut MatchRequests,
) {
    let keys = clip_keys_for_alias(bank, ns, alias);
    if keys.is_empty() {
        set.missing.insert(alias.to_owned());
    } else {
        set.resolved_aliases += 1;
    }
    for key in keys {
        clips.request(key.clone());
        if clips.ready(&key).is_none() {
            set.required.insert(key);
        }
    }
}

fn request_fx_type10(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    alias: &str,
    set: &mut MatchRequests,
) {
    let ns = bank
        .index_in(AssetNamespace::Iw4, alias)
        .or_else(|| bank.index_unique(alias))
        .map(|index| bank.namespace_of_alias(index))
        .unwrap_or(AssetNamespace::Iw4);
    request_named(clips, bank, ns, alias, set);
}

fn request_weapon_aliases(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    registry: &WeaponRegistry,
    weapon: u32,
    set: &mut MatchRequests,
) -> usize {
    let sounds = registry.sounds_of(weapon);
    let ns = registry.namespace_of(weapon).unwrap_or(AssetNamespace::Iw4);
    let mut n = 0usize;
    if let Some(aliases) = sounds {
        for alias in aliases.reachable_aliases() {
            n += 1;
            request_named(clips, bank, ns, alias, set);
        }
    }
    let mut seen = HashSet::new();
    for alias in registry.notetrack_sound_aliases_of(weapon) {
        if seen.insert(alias) {
            n += 1;
            request_named(clips, bank, ns, alias, set);
        }
    }
    n
}
