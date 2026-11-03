use frame::ScopeApp;
use std::collections::{BTreeSet, HashSet};

use asset_audio::SoundCatalog;
use asset_core::FamilyId;
use asset_game::WeaponRegistry;
use assets::{MapLoadProcess, MatchType10SoundHints, PreparedWeapons};
use bevy::prelude::*;
use frame::{ClientSet, LaunchIdentity};

use crate::aliases::movement_prepare_names;
use crate::ambient::{SoundBankCompose, SoundBankNamespace};
use crate::clip_store::{ClipKey, ClipStore, clip_keys_for_alias};
use crate::playback::SoundBank;

#[derive(Resource, Default)]
pub struct AudioReady(pub frame::WorldReadiness);

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

const PREFETCH_PER_PASS: usize = 64;

const MATCH_HUD_PULSE: &[&str] = &["ui_pulse_text_type", "ui_pulse_text_delete"];

const MENU_CODE: [&str; 2] = ["mouse_over", "mouse_click"];

const MATCH_CLOCK: &[&str] = &["ui_mp_timer_countdown"];

#[derive(Default)]
struct MatchRequests {
    required: HashSet<ClipKey>,
    missing: BTreeSet<String>,
    resolved_aliases: usize,
    capacity_failure: bool,
}

#[derive(Resource, Default)]
struct MatchClipPrep {
    submitted: bool,
    capacity_failure: bool,
    required: HashSet<ClipKey>,
    total: usize,
    stage: Option<asset_transport::StageHandle>,
    /// Resident sample bytes this process had already produced when the set
    /// was queued. The decoders count for the life of the process, so what
    /// this match cost is the distance from here.
    sample_bytes_at_queue: u64,
}

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
    app.scoped::<AudioReady>(frame::MatchScope::Live)
        .scoped::<MatchClipPrep>(frame::MatchScope::Live)
        .scoped::<crate::match_voices::AnnouncerRoutes>(frame::MatchScope::Live)
        .add_systems(
            PreUpdate,
            (
                queue_match_clips
                    .in_set(frame::InMatch)
                    .after(crate::ambient::install_sound_bank),
                poll_match_audio_ready
                    .in_set(frame::InMatch)
                    .after(queue_match_clips),
            )
                .in_set(ClientSet::Load),
        );
}

#[derive(bevy::ecs::system::SystemParam)]
struct MatchClipInputs<'w> {
    weapons: Option<Res<'w, PreparedWeapons>>,
    type10: Option<Res<'w, MatchType10SoundHints>>,
    bank: Option<Res<'w, SoundBank>>,
    catalog: Option<Res<'w, asset_game::MenuCatalog>>,
    script_sound: Option<Res<'w, asset_audio::SessionMapScriptSound>>,
    teams: Option<Res<'w, asset_game::SessionTeamSettings>>,
    namespace: Option<Res<'w, SoundBankNamespace>>,
}

struct MatchSoundCatalogs<'w> {
    weapons: Res<'w, PreparedWeapons>,
    type10: Res<'w, MatchType10SoundHints>,
    bank: Res<'w, SoundBank>,
    namespace: Res<'w, SoundBankNamespace>,
    script_sound: Res<'w, asset_audio::SessionMapScriptSound>,
    catalog: Option<Res<'w, asset_game::MenuCatalog>>,
    teams: Option<Res<'w, asset_game::SessionTeamSettings>>,
}

impl<'w> MatchClipInputs<'w> {
    fn catalogs(self) -> Option<MatchSoundCatalogs<'w>> {
        let weapons = self.weapons?;
        let type10 = self.type10?;
        let bank = self.bank?;
        let namespace = self.namespace?;
        let script_sound = self.script_sound?;
        Some(MatchSoundCatalogs {
            weapons,
            type10,
            bank,
            namespace,
            script_sound,
            catalog: self.catalog,
            teams: self.teams,
        })
    }
}

impl MatchSoundCatalogs<'_> {
    fn request(
        &self,
        clips: &mut ResMut<'_, ClipStore>,
        announcer: &mut ResMut<'_, crate::match_voices::AnnouncerRoutes>,
        identity: Option<&LaunchIdentity>,
    ) -> Option<(MatchRequests, usize)> {
        let mut set = MatchRequests::default();
        let mut aliases = 0usize;
        for weapon in 1..=self.weapons.registry().len() as u32 {
            aliases += request_weapon_aliases(
                clips,
                &self.bank.0,
                &self.weapons.registry(),
                weapon,
                &mut set,
            );
        }
        for alias in movement_prepare_names() {
            aliases += 1;
            request_named(clips, &self.bank.0, FamilyId::IW4, alias, &mut set);
        }
        if let Some(identity) = identity {
            let ns = self.namespace.namespace;
            let zone = self.namespace.zone.as_str();
            if zone != identity.zone {
                return None;
            }
            for emitter in self.bank.0.createfx_loop_sounds(ns, zone) {
                aliases += 1;
                request_named(clips, &self.bank.0, ns, &emitter.soundalias, &mut set);
            }
        }
        if let Some(identity) = identity {
            let (allies, axis) = crate::match_voices::voice_prefixes_for_zone(
                self.catalog.as_deref(),
                Some(self.bank.as_ref()),
                identity,
            );
            let voices = crate::match_voices::team_voice_aliases(
                &self.bank.0,
                allies.as_deref(),
                axis.as_deref(),
            );
            let native = self.teams.as_deref().map(|teams| &teams.0);
            **announcer = crate::match_voices::AnnouncerRoutes::build(
                &self.bank.0,
                self.namespace.namespace,
                [allies.as_deref(), axis.as_deref()],
                native,
                &voices,
            );
            diag::info!(
                Audio,
                "audio: announcer: {} of {} team voice aliases play as {:?}",
                announcer.len(),
                voices.len(),
                self.namespace.namespace
            );
            for alias in &voices {
                aliases += 1;
                let (ns, alias) = announcer.route(alias);
                request_named(clips, &self.bank.0, ns, alias, &mut set);
            }
        }
        if let Some(alias) = self.script_sound.0.ambient_alias.as_deref() {
            aliases += 1;
            request_named(
                clips,
                &self.bank.0,
                self.namespace.namespace,
                alias,
                &mut set,
            );
        }
        if let Some(catalog) = self.catalog.as_deref() {
            for alias in catalog.played_sound_aliases().into_iter().chain(MENU_CODE) {
                aliases += 1;
                request_named(clips, &self.bank.0, FamilyId::IW4, alias, &mut set);
            }
        }
        for alias in MATCH_HUD_PULSE
            .iter()
            .chain([&"motiontracker_ping", &"motiontracker_pong"])
            .chain(crate::weapon_lock::ALIASES.iter())
            .chain([&gamemode_iw4::damage_feedback::HIT_ALERT_ALIAS])
            .chain(MATCH_CLOCK)
        {
            aliases += 1;
            let (ns, alias) = crate::aliases::match_ui_alias(self.namespace.namespace, alias);
            request_named(clips, &self.bank.0, ns, alias, &mut set);
        }
        let mut breath_policies = HashSet::new();
        for weapon in 1..=self.weapons.registry().len() as u32 {
            if self
                .weapons
                .registry()
                .bind_published_row(weapon)
                .ok()
                .and_then(|weapon| weapon.hud_facts())
                .is_some_and(|facts| facts.can_hold_breath)
                && let Some(policy) = self.weapons.registry().semantic_policy_of(weapon)
                && breath_policies.insert((policy.cue_namespace, policy.breath_cues))
            {
                let ns = policy.cue_namespace;
                for alias in policy.breath_cues.aliases() {
                    aliases += 1;
                    request_named(clips, &self.bank.0, ns, alias, &mut set);
                }
            }
        }
        for alias in &self.type10.0 {
            aliases += 1;
            request_fx_type10(clips, &self.bank.0, alias, &mut set);
        }
        if !set.missing.is_empty() {
            let names: Vec<&str> = set.missing.iter().map(String::as_str).collect();
            diag::info!(
                Audio,
                "audio: prefetch catalog gaps: {} aliases name no loaded sound: {}",
                names.len(),
                names.join(" ")
            );
        }
        Some((set, aliases))
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct MatchClipPreparation<'w> {
    walk: Option<Res<'w, SoundBankCompose>>,
    clips: Option<ResMut<'w, ClipStore>>,
    loading: Option<Res<'w, MapLoadProcess>>,
    prep: ResMut<'w, MatchClipPrep>,
    announcer: ResMut<'w, crate::match_voices::AnnouncerRoutes>,
    ready: ResMut<'w, AudioReady>,
    silent: Option<Res<'w, AudioSilent>>,
    frontend: Option<Res<'w, crate::frontend::FrontendAudio>>,
}

impl MatchClipPreparation<'_> {
    fn queue(mut self, inputs: MatchClipInputs, identity: Option<Res<LaunchIdentity>>) {
        if self.ready.0.state != frame::ReadinessState::Pending || self.prep.submitted {
            return;
        }
        if self.silent.is_some() {
            self.ready.0.state = frame::ReadinessState::Silent;
            if let Some(loading) = self.loading.as_ref() {
                loading
                    .progress
                    .record_skipped(asset_transport::StageId::Audio);
            }
            diag::info!(Audio, "audio: AudioReady — Silent, no clips prepared");
            return;
        }
        if self.walk.is_some() {
            return;
        }
        let Some(clips) = self.clips.as_mut() else {
            self.ready.0.state = frame::ReadinessState::Failed;
            if let Some(loading) = self.loading.as_ref() {
                loading
                    .progress
                    .begin(asset_transport::StageId::Audio, None)
                    .fail();
            }
            diag::warn!(
                Audio,
                "audio: preparation failed — sound bank did not install"
            );
            return;
        };
        let Some(catalogs) = inputs.catalogs() else {
            return;
        };
        if let Some(frontend) = self.frontend {
            frontend.clips.release_idle_streamed();
        }
        let Some((set, aliases)) =
            catalogs.request(clips, &mut self.announcer, identity.as_deref())
        else {
            return;
        };
        self.prep.total = set.required.len();
        self.prep.required = set.required;
        self.prep.capacity_failure = set.capacity_failure;
        self.prep.submitted = true;
        if let Some(loading) = self.loading {
            self.prep.sample_bytes_at_queue = prepared_sample_bytes();
            if self.prep.total == 0 && set.resolved_aliases > 0 {
                loading
                    .progress
                    .record_reused_scoped(asset_transport::StageId::Audio, "clips");
            } else {
                self.prep.stage = Some(loading.progress.begin(
                    asset_transport::StageId::Audio,
                    Some(self.prep.total as u64),
                ));
            }
        }
        diag::info!(
            Audio,
            "audio: match-set queued {} aliases ({} type-10), {} required clips ({} workers); resident reused={} clips/{}B",
            aliases,
            catalogs.type10.0.len(),
            self.prep.total,
            clips.workers(),
            clips.reused_resident().0,
            clips.reused_resident().1,
        );
        if self.prep.capacity_failure {
            fail_capacity(&mut self.ready, &mut self.prep, Some(&mut **clips));
        } else if self.prep.total == 0 {
            mark_ready(&mut self.ready, &mut self.prep, Some(&mut **clips));
        }
    }
}

fn queue_match_clips(
    preparation: MatchClipPreparation,
    inputs: MatchClipInputs,
    identity: Option<Res<LaunchIdentity>>,
) {
    preparation.queue(inputs, identity);
}

fn poll_match_audio_ready(
    mut clips: Option<ResMut<ClipStore>>,
    mut prep: ResMut<MatchClipPrep>,
    mut ready: ResMut<AudioReady>,
) {
    if ready.0.state != frame::ReadinessState::Pending || !prep.submitted || prep.capacity_failure {
        return;
    }
    let Some(clips) = clips.as_mut() else {
        return;
    };
    let mut capacity_failure = false;
    let mut submissions = PREFETCH_PER_PASS;
    prep.required.retain(|key| {
        if submissions != 0 {
            match clips.prefetch(key.clone()) {
                crate::clip_store::MediaRequest::Submitted
                | crate::clip_store::MediaRequest::Resident => submissions -= 1,
                crate::clip_store::MediaRequest::Deferred => submissions = 0,
                crate::clip_store::MediaRequest::Existing => {}
                crate::clip_store::MediaRequest::Refused => {
                    capacity_failure = true;
                    submissions = 0;
                }
            }
        }
        match clips.ready(key) {
            Some(Err(
                crate::clip_store::ClipError::RequestLimit
                | crate::clip_store::ClipError::InvalidPcm(crate::media::PcmError::MemoryLimit),
            )) => {
                capacity_failure = true;
                false
            }
            Some(_) => false,
            None => true,
        }
    });
    if capacity_failure {
        prep.capacity_failure = true;
        fail_capacity(&mut ready, &mut prep, Some(&mut **clips));
        return;
    }
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

fn fail_capacity(ready: &mut AudioReady, prep: &mut MatchClipPrep, clips: Option<&mut ClipStore>) {
    if let Some(stage) = prep.stage.take() {
        stage.fail();
    }
    diag::warn!(
        Audio,
        "audio: match media capacity exceeded; {} of {} clips pending; pcm={:?} decode={:?}",
        prep.required.len(),
        prep.total,
        crate::pcm_memory(),
        crate::decode_memory()
    );
    ready.0.state = frame::ReadinessState::Failed;
    if let Some(clips) = clips {
        clips.arm_match_live();
    }
}

fn mark_ready(ready: &mut AudioReady, prep: &mut MatchClipPrep, clips: Option<&mut ClipStore>) {
    if let Some(stage) = prep.stage.take() {
        stage.set_completed(prep.total as u64);
        stage.set_bytes(prepared_sample_bytes().saturating_sub(prep.sample_bytes_at_queue));
        stage.done();
    }
    if ready.0.state == frame::ReadinessState::Pending {
        ready.0.state = frame::ReadinessState::Ready;
        diag::info!(Audio, "audio: AudioReady ({} clips prepared)", prep.total);
        if let Some(clips) = clips {
            clips.arm_match_live();
        }
    }
}

fn request_named(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    ns: FamilyId,
    alias: &str,
    set: &mut MatchRequests,
) {
    let keys = clip_keys_for_alias(bank, ns, alias);
    if keys.is_empty() {
        set.missing.insert(alias.to_owned());
    } else {
        set.resolved_aliases += 1;
    }
    for key in keys.into_iter().filter(|key| key.is_loaded()) {
        match clips.ready(&key) {
            Some(Err(
                crate::clip_store::ClipError::RequestLimit
                | crate::clip_store::ClipError::InvalidPcm(crate::media::PcmError::MemoryLimit),
            )) => set.capacity_failure = true,
            Some(_) => {}
            None => {
                if set.required.len() == crate::clip_store::MEDIA_REQUEST_LIMIT
                    && !set.required.contains(&key)
                {
                    set.capacity_failure = true;
                } else {
                    set.required.insert(key);
                }
            }
        }
    }
}

fn request_fx_type10(
    clips: &mut ClipStore,
    bank: &SoundCatalog,
    alias: &str,
    set: &mut MatchRequests,
) {
    let Some(ns) = bank
        .index_unique(alias)
        .and_then(|index| bank.namespace_of_alias(index))
    else {
        return;
    };
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
    let mut n = 0usize;
    if let Some(aliases) = sounds {
        for alias in aliases.reachable_aliases() {
            n += 1;
            if let Some(ns) = registry.sound_namespace_for(weapon, alias) {
                request_named(clips, bank, ns, alias, set);
            }
        }
    }
    let mut seen = HashSet::new();
    for alias in registry.notetrack_sound_aliases_of(weapon) {
        if seen.insert(alias) {
            n += 1;
            if let Some(ns) = registry.sound_namespace_for(weapon, alias) {
                request_named(clips, bank, ns, alias, set);
            }
        }
    }
    n
}
