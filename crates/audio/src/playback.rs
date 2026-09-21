use std::collections::HashMap;
use std::f32::consts::FRAC_PI_4;
use std::sync::Arc;
use std::time::Instant;

use asset_iw4::{SND_CURVE_MAX_KNOTS, snd_attenuate, snd_has_free_voice};
use assets::{AssetNamespace, NamespaceSoundIwd, SoundCatalog, lerp_range, snd_unit_random};
use bevy::{
    audio::{AddAudioSource, AudioSink, AudioSinkPlayback, Volume},
    prelude::*,
};
use frame::{ClientSet, FxSoundPublished, MatchTornDown, SessionSwapApplied};
use net::{LastAdoptedSnapshot, SvcLocalSound};

use crate::ambient::SoundIwd;
use crate::backend::MatchEpoch;
use crate::clip_store::{
    ClipError, ClipKey, ClipStore, PendingOneshot, PendingStarts, clip_key_for_variant,
    clip_keys_for_alias, deadline_for,
};
use crate::messages::{
    AliasCommand, Footstep, LandSound, PlayAlias, SND_ENT_LOCAL, ViewmodelNotetracks, WeaponSound,
};
use crate::pcm::{LoopingPcmAudio, PcmAudio};
use crate::space::{distance_inches, transform_inches};
use crate::start::{
    SoundClass, StartDecision, StartDecisions, StartFailure, StartOutcome, SuppressReason,
};
use crate::voice::{VoiceLease, VoiceOccupancy, reclaim_finished_voices};

#[derive(Component, Default)]
pub struct AmbientListener;

#[derive(Component)]
pub struct Channel3d {
    pub origin_inches: [f32; 3],
    pub dist_min: f32,
    pub dist_max: f32,
    pub knots: Arc<[[f32; 2]]>,
    pub base_volume: f32,

    pub live_pan: crate::pcm::LivePan,
}

#[derive(Resource, Clone)]
pub struct SoundBank(pub Arc<SoundCatalog>);

#[derive(Resource)]
pub struct SoundPickState {
    pub lcg: u32,
    pub last_variant: HashMap<(AssetNamespace, String), usize>,
}

impl Default for SoundPickState {
    fn default() -> Self {
        Self {
            lcg: 0x00a5_5a5a,
            last_variant: HashMap::new(),
        }
    }
}

#[derive(Component)]
pub struct AliasPlayback {
    pub namespace: AssetNamespace,
    pub snd_ent: Option<u32>,
    pub alias: String,
}

#[derive(Resource, Default, Debug)]
pub struct MissingAliasGaps {
    pub aliases: Vec<String>,
}

impl MissingAliasGaps {
    pub fn record(&mut self, alias: &str) {
        if !self.aliases.iter().any(|a| a == alias) {
            diag::warn!(Audio, "audio: missing alias `{alias}` (typed gap)");
            self.aliases.push(alias.to_owned());
        }
    }

    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }
}

#[derive(Resource, Default)]
pub(crate) struct SharedPlayAssets {
    dry: HashMap<ClipKey, Handle<PcmAudio>>,
    curves: HashMap<String, Arc<[[f32; 2]]>>,
}

impl SharedPlayAssets {
    fn dry_handle(
        &mut self,
        pcm_assets: &mut Assets<PcmAudio>,
        clip: &ClipKey,
        pcm: PcmAudio,
    ) -> Handle<PcmAudio> {
        self.dry
            .entry(clip.clone())
            .or_insert_with(|| pcm_assets.add(pcm))
            .clone()
    }

    pub(crate) fn intern_curve(&mut self, name: &str, knots: &[(f32, f32)]) -> Arc<[[f32; 2]]> {
        if let Some(existing) = self.curves.get(name) {
            return Arc::clone(existing);
        }
        let packed: Vec<[f32; 2]> = knots
            .iter()
            .take(SND_CURVE_MAX_KNOTS)
            .map(|&(x, y)| [x, y])
            .collect();
        let arc: Arc<[[f32; 2]]> = packed.into();
        self.curves.insert(name.to_owned(), Arc::clone(&arc));
        arc
    }
}

pub(crate) struct PlayerSoundPlugin;

impl Plugin for PlayerSoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundPickState>()
            .init_resource::<MissingAliasGaps>()
            .init_resource::<SharedPlayAssets>()
            .init_resource::<StartDecisions>()
            .init_resource::<PendingStarts>()
            .init_resource::<VoiceOccupancy>()
            .init_resource::<crate::ambient::MapAmbientBooted>()
            .init_resource::<crate::ambient::SoundBankLoadAttempted>()
            .init_resource::<crate::BobCycleTracker>()
            .add_audio_source::<PcmAudio>()
            .add_audio_source::<LoopingPcmAudio>()
            .add_message::<AliasCommand>()
            .add_message::<Footstep>()
            .add_message::<WeaponSound>()
            .add_message::<ViewmodelNotetracks>()
            .add_message::<LandSound>()
            .add_systems(
                Update,
                (
                    crate::ambient::boot_map_ambient_once,
                    reclaim_finished_voices
                        .before(drain_pending_oneshots)
                        .before(play_alias_messages)
                        .before(play_footstep_messages)
                        .before(play_weapon_sound_messages)
                        .before(play_land_sound_messages),
                    drain_pending_oneshots
                        .after(play_alias_messages)
                        .before(play_footstep_messages)
                        .before(play_weapon_sound_messages)
                        .before(play_land_sound_messages),
                    apply_svc_local_sound
                        .before(play_alias_messages)
                        .run_if(resource_exists::<LastAdoptedSnapshot>),
                    crate::shellshock::update_shellshock_tinnitus.before(play_alias_messages),
                    play_alias_messages.after(FxSoundPublished),
                    play_footstep_messages,
                    crate::entity_events::play_viewmodel_notetrack_messages
                        .before(play_weapon_sound_messages),
                    play_weapon_sound_messages,
                    play_land_sound_messages,
                    crate::map_doors::update.before(crate::ambient::update_map_emitter_gain),
                    crate::destructible_loops::update
                        .before(crate::ambient::update_map_emitter_gain),
                    crate::ambient::update_map_emitter_gain,
                    snd_update_all_channels,
                )
                    .in_set(ClientSet::Effects),
            )
            .add_systems(
                Update,
                (
                    crate::ambient::start_sound_bank_walk
                        .after(crate::ambient::stop_map_ambient_on_match_torn_down),
                    crate::ambient::install_sound_bank.after(crate::ambient::start_sound_bank_walk),
                    crate::ambient::stop_map_ambient_on_match_torn_down.after(SessionSwapApplied),
                    reset_clip_prep_on_match_torn_down,
                )
                    .in_set(ClientSet::Load),
            );
    }
}

fn reset_clip_prep_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    mut pending: ResMut<PendingStarts>,
    mut shared: ResMut<SharedPlayAssets>,
) {
    if torn.read().count() == 0 {
        return;
    }
    pending.clear();
    *shared = SharedPlayAssets::default();
}

fn listener_pose(listeners: &Query<&Transform, With<AmbientListener>>) -> Option<(Vec3, Vec3)> {
    let n = listeners.iter().len();
    if n > 1 {
        panic!("second listener / amp maxRadius gate not ported");
    }
    listeners
        .iter()
        .next()
        .map(|t| (t.translation, t.rotation * Vec3::X))
}

fn snd_update_all_channels(
    listeners: Query<&Transform, With<AmbientListener>>,
    mut channels: Query<(&Channel3d, &mut AudioSink)>,
    settings: Res<frame::GameSettings>,
) {
    let Some((ear, right)) = listener_pose(&listeners) else {
        return;
    };
    let ear_inches = transform_inches(ear);
    for (ch, mut sink) in &mut channels {
        let dist = distance_inches(ear_inches, ch.origin_inches);
        let atten = if ch.knots.is_empty() {
            0.0
        } else {
            let value = snd_attenuate(&ch.knots, dist, ch.dist_min, ch.dist_max);
            if value < 0.0 { 0.0 } else { value }
        };
        let emitter = Vec3::from_array(ch.origin_inches);
        let (pan_l, pan_r) = world_oneshot_channel_gains(ear, right, emitter, 1.0);
        ch.live_pan.set(pan_l, pan_r);
        sink.set_volume(Volume::Linear(
            (ch.base_volume * atten * settings.master_volume).max(0.0),
        ));
    }
}

fn apply_svc_local_sound(
    mut cmds: MessageReader<SvcLocalSound>,
    adopted: Res<LastAdoptedSnapshot>,
    mut play: MessageWriter<crate::AliasCommand>,
) {
    for cmd in cmds.read() {
        let Some(alias) = adopted.sound_alias_name(cmd.index).map(str::to_owned) else {
            diag::warn!(
                Audio,
                "audio: svc local sound CS index {} is unresolved (typed gap)",
                cmd.index
            );
            continue;
        };
        if cmd.stop {
            play.write(AliasCommand::Stop {
                namespace: AssetNamespace::Iw4,
                alias,
                snd_ent: Some(SND_ENT_LOCAL),
            });
        } else {
            play.write(crate::AliasCommand::Play(PlayAlias {
                namespace: AssetNamespace::Iw4,
                alias,
                fallback: None,
                origin_inches: None,
                snd_ent: Some(SND_ENT_LOCAL),
            }));
        }
    }
}

fn play_alias_messages(
    mut events: MessageReader<AliasCommand>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut pick: ResMut<SoundPickState>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pending: ResMut<PendingStarts>,
    mut occupancy: ResMut<VoiceOccupancy>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    epoch: Res<MatchEpoch>,
    listeners: Query<&Transform, With<AmbientListener>>,
) {
    let pose = listener_pose(&listeners);
    let iwd = iwd.as_deref().map(|s| s.0.as_ref());
    for command in events.read() {
        let event = match command {
            AliasCommand::Play(event) => event,
            AliasCommand::Stop {
                namespace,
                alias,
                snd_ent,
            } => {
                pending.cancel_alias(*namespace, alias, *snd_ent, epoch.0);
                let (namespace, alias, snd_ent, epoch) =
                    (*namespace, alias.clone(), *snd_ent, epoch.0);
                // Ordered after prior spawns, including ones queued by Play in
                // this same message batch; a later Play remains a new voice.
                commands.queue(move |world: &mut World| {
                    let entities: Vec<_> = world
                        .query::<(Entity, &AliasPlayback, &crate::backend::Voice)>()
                        .iter(world)
                        .filter(|(_, tag, voice)| {
                            tag.namespace == namespace
                                && tag.alias == alias
                                && tag.snd_ent == snd_ent
                                && voice.epoch == epoch
                                && voice.scope == crate::backend::AudioScope::Match
                        })
                        .map(|(entity, _, _)| entity)
                        .collect();
                    for entity in entities {
                        world.despawn(entity);
                    }
                });
                continue;
            }
        };
        let Some(bank) = bank.as_ref() else {
            drop_without_bank(std::iter::once(event.alias.as_str()), &mut decisions);
            continue;
        };
        let outcome = play_alias_oneshot(
            &mut commands,
            &mut pcm_assets,
            &mut shared,
            &bank.0,
            iwd,
            event.namespace,
            &event.alias,
            event.origin_inches,
            pose,
            &mut pick,
            clips.as_deref_mut(),
            &mut pending,
            &mut occupancy,
            &mut decisions,
            event.snd_ent,
            SoundClass::World,
            epoch.0,
        );
        finish_binding_start(
            outcome,
            &event.alias,
            event.fallback.as_deref(),
            |fb| {
                play_alias_oneshot(
                    &mut commands,
                    &mut pcm_assets,
                    &mut shared,
                    &bank.0,
                    iwd,
                    event.namespace,
                    fb,
                    event.origin_inches,
                    pose,
                    &mut pick,
                    clips.as_deref_mut(),
                    &mut pending,
                    &mut occupancy,
                    &mut decisions,
                    event.snd_ent,
                    SoundClass::World,
                    epoch.0,
                )
            },
            &mut gaps,
        );
    }
}

fn play_footstep_messages(
    mut events: MessageReader<Footstep>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut pick: ResMut<SoundPickState>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pending: ResMut<PendingStarts>,
    mut occupancy: ResMut<VoiceOccupancy>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    epoch: Res<MatchEpoch>,
    listeners: Query<&Transform, With<AmbientListener>>,
) {
    let Some(bank) = bank else {
        drop_without_bank(events.read().map(|e| e.alias), &mut decisions);
        return;
    };
    let pose = listener_pose(&listeners);
    let iwd = iwd.as_deref().map(|s| s.0.as_ref());
    for event in events.read() {
        let outcome = play_surface_alias_chain(
            &mut commands,
            &mut pcm_assets,
            &mut shared,
            &bank.0,
            iwd,
            event.alias,
            event.fallback,
            event.origin_inches,
            pose,
            &mut pick,
            clips.as_deref_mut(),
            &mut pending,
            &mut occupancy,
            &mut decisions,
            event.snd_ent,
            SoundClass::World,
            epoch.0,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(event.alias);
            gaps.record(event.fallback);
        }
    }
}

fn play_weapon_sound_messages(
    mut events: MessageReader<WeaponSound>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut pick: ResMut<SoundPickState>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pending: ResMut<PendingStarts>,
    mut occupancy: ResMut<VoiceOccupancy>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    epoch: Res<MatchEpoch>,
    listeners: Query<&Transform, With<AmbientListener>>,
) {
    let Some(bank) = bank else {
        drop_without_bank(events.read().map(|e| e.alias.as_str()), &mut decisions);
        return;
    };
    let pose = listener_pose(&listeners);
    let iwd = iwd.as_deref().map(|s| s.0.as_ref());
    for event in events.read() {
        let outcome = play_alias_oneshot(
            &mut commands,
            &mut pcm_assets,
            &mut shared,
            &bank.0,
            iwd,
            event.namespace,
            &event.alias,
            event.origin_inches,
            pose,
            &mut pick,
            clips.as_deref_mut(),
            &mut pending,
            &mut occupancy,
            &mut decisions,
            event.snd_ent,
            SoundClass::Weapon,
            epoch.0,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(&event.alias);
        }
    }
}

fn play_land_sound_messages(
    mut events: MessageReader<LandSound>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut pick: ResMut<SoundPickState>,
    mut gaps: ResMut<MissingAliasGaps>,
    mut clips: Option<ResMut<ClipStore>>,
    mut pending: ResMut<PendingStarts>,
    mut occupancy: ResMut<VoiceOccupancy>,
    mut decisions: ResMut<StartDecisions>,
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    epoch: Res<MatchEpoch>,
    listeners: Query<&Transform, With<AmbientListener>>,
) {
    let Some(bank) = bank else {
        drop_without_bank(events.read().map(|e| e.alias), &mut decisions);
        return;
    };
    let pose = listener_pose(&listeners);
    let iwd = iwd.as_deref().map(|s| s.0.as_ref());
    for event in events.read() {
        let outcome = play_surface_alias_chain(
            &mut commands,
            &mut pcm_assets,
            &mut shared,
            &bank.0,
            iwd,
            event.alias,
            event.fallback,
            event.origin_inches,
            pose,
            &mut pick,
            clips.as_deref_mut(),
            &mut pending,
            &mut occupancy,
            &mut decisions,
            event.snd_ent,
            SoundClass::World,
            epoch.0,
        );
        if outcome.allows_binding_fallback() {
            gaps.record(event.alias);
            gaps.record(event.fallback);
        }
    }
}

fn drop_without_bank<'a>(aliases: impl Iterator<Item = &'a str>, decisions: &mut StartDecisions) {
    for alias in aliases {
        diag::warn!(
            Audio,
            "audio: alias `{alias}` dropped — no SoundBank (typed gap)"
        );
        decisions.record(StartDecision {
            namespace: AssetNamespace::Iw4,
            alias: alias.to_owned(),
            variant: None,
            outcome: StartOutcome::Failed(StartFailure::BankMissing),
            secondary: None,
            detail: None,
        });
    }
}

fn finish_binding_start(
    outcome: StartOutcome,
    alias: &str,
    fallback: Option<&str>,
    play_fallback: impl FnOnce(&str) -> StartOutcome,
    gaps: &mut MissingAliasGaps,
) {
    if outcome.is_open() || !outcome.allows_binding_fallback() {
        return;
    }
    if let Some(fb) = fallback {
        let fb_out = play_fallback(fb);
        if fb_out.is_open() {
            return;
        }
        if fb_out.allows_binding_fallback() {
            gaps.record(fb);
        }
    }
    gaps.record(alias);
}

fn play_surface_alias_chain(
    commands: &mut Commands,
    pcm_assets: &mut Assets<PcmAudio>,
    shared: &mut SharedPlayAssets,
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    alias: &str,
    fallback: &str,
    origin_inches: Option<[f32; 3]>,
    listener: Option<(Vec3, Vec3)>,
    pick: &mut SoundPickState,
    mut clips: Option<&mut ClipStore>,
    pending: &mut PendingStarts,
    occupancy: &mut VoiceOccupancy,
    decisions: &mut StartDecisions,
    snd_ent: Option<u32>,
    class: SoundClass,
    epoch: u64,
) -> StartOutcome {
    let mut last = StartOutcome::Failed(StartFailure::MissingAlias);
    for candidate in crate::aliases::surface_alias_candidates(alias, fallback) {
        let outcome = play_alias_oneshot(
            commands,
            pcm_assets,
            shared,
            bank,
            iwd,
            AssetNamespace::Iw4,
            candidate,
            origin_inches,
            listener,
            pick,
            clips.as_deref_mut(),
            pending,
            occupancy,
            decisions,
            snd_ent,
            class,
            epoch,
        );
        if outcome.is_open() || !outcome.allows_binding_fallback() {
            return outcome;
        }
        last = outcome;
    }
    last
}

pub fn world_oneshot_pan(ear: Vec3, listener_right: Vec3, emitter: Vec3) -> f32 {
    let to = emitter - ear;
    if to.length_squared() < 1e-8 {
        0.0
    } else {
        to.normalize().dot(listener_right).clamp(-1.0, 1.0)
    }
}

pub fn world_oneshot_channel_gains(
    ear: Vec3,
    listener_right: Vec3,
    emitter: Vec3,
    atten: f32,
) -> (f32, f32) {
    let pan = world_oneshot_pan(ear, listener_right, emitter);
    let angle = (pan + 1.0) * FRAC_PI_4;
    (atten * angle.cos(), atten * angle.sin())
}

fn drain_pending_oneshots(
    mut pending: ResMut<PendingStarts>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut pick: ResMut<SoundPickState>,
    mut occupancy: ResMut<VoiceOccupancy>,
    mut decisions: ResMut<StartDecisions>,
    mut clips: Option<ResMut<ClipStore>>,
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    epoch: Res<MatchEpoch>,
    listeners: Query<&Transform, With<AmbientListener>>,
) {
    if pending.entries.is_empty() {
        return;
    }
    let Some(bank) = bank else {
        pending.clear();
        return;
    };
    let Some(clips) = clips.as_mut() else {
        return;
    };
    let pose = listener_pose(&listeners);
    let iwd = iwd.as_deref().map(|s| s.0.as_ref());
    let now = Instant::now();
    let waiting = std::mem::take(&mut pending.entries);
    for entry in waiting {
        if entry.class.scope() == crate::backend::AudioScope::Match && entry.epoch != epoch.0 {
            continue;
        }
        if now >= entry.deadline {
            diag::warn!(
                Audio,
                "audio: alias `{}:{}` expired awaiting decode (typed gap)",
                entry.namespace.as_str(),
                entry.alias
            );
            decisions.record(StartDecision {
                namespace: entry.namespace,
                alias: entry.alias,
                variant: Some(entry.variant),
                outcome: StartOutcome::Failed(StartFailure::Expired),
                secondary: None,
                detail: None,
            });
            continue;
        }
        match clips.ready(&entry.clip) {
            None => pending.entries.push(entry),
            Some(Err(_)) => {
                decisions.record(StartDecision {
                    namespace: entry.namespace,
                    alias: entry.alias,
                    variant: Some(entry.variant),
                    outcome: StartOutcome::Failed(StartFailure::DecodeFailed),
                    secondary: None,
                    detail: None,
                });
            }
            Some(Ok(pcm)) => {
                let started = submit_prepared_oneshot(
                    &mut commands,
                    &mut pcm_assets,
                    &mut shared,
                    &bank.0,
                    iwd,
                    entry.namespace,
                    &entry.alias,
                    entry.origin_inches,
                    pose,
                    &mut pick,
                    Some(clips.as_mut()),
                    &mut pending,
                    &mut occupancy,
                    entry.snd_ent,
                    0,
                    entry.class,
                    entry.epoch,
                    pcm,
                    &entry.clip,
                    entry.variant,
                    entry.volume,
                    entry.pitch,
                    entry.layer.as_deref(),
                );
                if let Some((sec, sec_out)) = &started.secondary
                    && !sec_out.is_open()
                {
                    diag::warn!(
                        Audio,
                        "audio: secondary `{sec}` of `{}:{}` result={sec_out}",
                        entry.namespace.as_str(),
                        entry.alias
                    );
                }
                decisions.record(StartDecision {
                    namespace: entry.namespace,
                    alias: entry.alias,
                    variant: started.variant,
                    outcome: started.outcome,
                    secondary: started.secondary,
                    detail: started.detail,
                });
            }
        }
    }
}

pub(crate) fn play_alias_oneshot(
    commands: &mut Commands,
    pcm_assets: &mut Assets<PcmAudio>,
    shared: &mut SharedPlayAssets,
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    namespace: AssetNamespace,
    alias: &str,
    origin_inches: Option<[f32; 3]>,
    listener: Option<(Vec3, Vec3)>,
    pick: &mut SoundPickState,
    clips: Option<&mut ClipStore>,
    pending: &mut PendingStarts,
    occupancy: &mut VoiceOccupancy,
    decisions: &mut StartDecisions,
    snd_ent: Option<u32>,
    class: SoundClass,
    epoch: u64,
) -> StartOutcome {
    let started = play_alias_oneshot_at(
        commands,
        pcm_assets,
        shared,
        bank,
        iwd,
        namespace,
        alias,
        origin_inches,
        listener,
        pick,
        clips,
        pending,
        occupancy,
        snd_ent,
        0,
        class,
        epoch,
    );
    if let Some((sec, sec_out)) = &started.secondary
        && !sec_out.is_open()
    {
        diag::warn!(
            Audio,
            "audio: secondary `{sec}` of `{}:{alias}` result={sec_out}",
            namespace.as_str()
        );
    }
    let outcome = started.outcome.clone();
    decisions.record(StartDecision {
        namespace,
        alias: alias.to_owned(),
        variant: started.variant,
        outcome: started.outcome,
        secondary: started.secondary,
        detail: started.detail,
    });
    outcome
}

struct OneshotStart {
    outcome: StartOutcome,
    variant: Option<usize>,
    secondary: Option<(String, StartOutcome)>,
    detail: Option<String>,
}

impl OneshotStart {
    fn failed(failure: StartFailure) -> Self {
        Self {
            outcome: StartOutcome::Failed(failure),
            variant: None,
            secondary: None,
            detail: None,
        }
    }

    fn with_variant(mut self, variant: usize) -> Self {
        self.variant = Some(variant);
        self
    }
}

fn falloff_detail(dist: f32, dist_min: f32, dist_max: f32, atten: f32) -> String {
    format!("dist={dist:.0} dist_min={dist_min:.0} dist_max={dist_max:.0} atten={atten:.3}")
}

fn prepare_voice(
    commands: &mut Commands,
    occupancy: &mut VoiceOccupancy,
    bank: &SoundCatalog,
    channel: Option<u32>,
    snd_ent: Option<u32>,
) -> Result<(), SuppressReason> {
    let Some(ch) = channel else {
        return Ok(());
    };
    let Some(info) = bank.ent_channel(ch) else {
        return Ok(());
    };
    if info.is_restricted {
        if let Some(ent) = snd_ent {
            let stopped = occupancy.take_entity_channel(ent, ch);

            for entity in stopped {
                crate::backend::stop(commands, entity);
            }
        }
    }
    let voice_n = occupancy.voice_count(ch);
    if !snd_has_free_voice(voice_n, 0, info.max_voices) {
        return Err(SuppressReason::VoiceLimit);
    }
    Ok(())
}

fn voice_lease(
    bank: &SoundCatalog,
    channel: Option<u32>,
    snd_ent: Option<u32>,
) -> Option<VoiceLease> {
    let ch = channel?;
    bank.ent_channel(ch)?;
    Some(VoiceLease {
        channel: ch,
        snd_ent,
    })
}

fn track_voice(occupancy: &mut VoiceOccupancy, entity: Entity, lease: Option<VoiceLease>) {
    if let Some(lease) = lease {
        occupancy.track(entity, lease);
    }
}

fn streamed_row_volume_pitch(row: &assets::CapturedAlias, rng: &mut u32) -> (f32, f32) {
    let t_vol = snd_unit_random(rng);
    let t_pitch = snd_unit_random(rng);
    let volume = if row.vol_min == 0.0 && row.vol_max == 0.0 {
        1.0
    } else {
        lerp_range(row.vol_min, row.vol_max, t_vol)
    };
    let pitch = if row.pitch_min == 0.0 && row.pitch_max == 0.0 {
        1.0
    } else {
        lerp_range(row.pitch_min, row.pitch_max, t_pitch)
    };
    (volume, pitch)
}

fn play_alias_oneshot_at(
    commands: &mut Commands,
    pcm_assets: &mut Assets<PcmAudio>,
    shared: &mut SharedPlayAssets,
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    namespace: AssetNamespace,
    alias: &str,
    origin_inches: Option<[f32; 3]>,
    listener: Option<(Vec3, Vec3)>,
    pick: &mut SoundPickState,
    mut clips: Option<&mut ClipStore>,
    pending: &mut PendingStarts,
    occupancy: &mut VoiceOccupancy,
    snd_ent: Option<u32>,
    depth: u8,
    class: SoundClass,
    epoch: u64,
) -> OneshotStart {
    let avoid = pick
        .last_variant
        .get(&(namespace, alias.to_owned()))
        .copied();
    let Some(outcome) = bank.pick_loaded_outcome(namespace, alias, &mut pick.lcg, avoid) else {
        return OneshotStart::failed(StartFailure::MissingAlias);
    };
    let variant_index = outcome.variant_index;
    let loaded_name = outcome.picked.as_ref().map(|p| p.sound.name.as_str());
    let loaded_ns = outcome
        .picked
        .as_ref()
        .map(|p| AssetNamespace::from_zone_game(p.sound.game));
    let Some(clip) = clip_key_for_variant(
        bank,
        namespace,
        alias,
        variant_index,
        loaded_name,
        loaded_ns,
    ) else {
        return OneshotStart::failed(StartFailure::NoPcm).with_variant(variant_index);
    };
    let (volume, pitch, layer) = if let Some(picked) = &outcome.picked {
        (picked.volume, picked.pitch, picked.layer.clone())
    } else {
        let row = bank
            .sound_in(namespace, alias)
            .and_then(|s| s.aliases.get(variant_index));
        let (volume, pitch) = row
            .map(|row| streamed_row_volume_pitch(row, &mut pick.lcg))
            .unwrap_or((1.0, 1.0));
        (volume, pitch, row.and_then(|r| r.secondary.clone()))
    };
    let pcm = match take_or_pending_clip(
        bank,
        clips.as_deref_mut(),
        pending,
        clip.clone(),
        namespace,
        alias,
        variant_index,
        volume,
        pitch,
        origin_inches,
        snd_ent,
        layer.clone(),
        class,
        epoch,
    ) {
        ClipTake::Ready(pcm) => pcm,
        ClipTake::Pending => {
            return OneshotStart {
                outcome: StartOutcome::Pending,
                variant: Some(variant_index),
                secondary: None,
                detail: None,
            };
        }
        ClipTake::Failed(failure) => {
            return OneshotStart::failed(failure).with_variant(variant_index);
        }
    };
    submit_prepared_oneshot(
        commands,
        pcm_assets,
        shared,
        bank,
        iwd,
        namespace,
        alias,
        origin_inches,
        listener,
        pick,
        clips,
        pending,
        occupancy,
        snd_ent,
        depth,
        class,
        epoch,
        pcm,
        &clip,
        variant_index,
        volume,
        pitch,
        layer.as_deref(),
    )
}

enum ClipTake {
    Ready(PcmAudio),
    Pending,
    Failed(StartFailure),
}

fn take_or_pending_clip(
    bank: &SoundCatalog,
    clips: Option<&mut ClipStore>,
    pending: &mut PendingStarts,
    clip: ClipKey,
    namespace: AssetNamespace,
    alias: &str,
    variant: usize,
    volume: f32,
    pitch: f32,
    origin_inches: Option<[f32; 3]>,
    snd_ent: Option<u32>,
    layer: Option<String>,
    class: SoundClass,
    epoch: u64,
) -> ClipTake {
    if let Some(store) = clips {
        store.request(clip.clone());
        if let Some(layer) = layer.as_deref() {
            for key in clip_keys_for_alias(bank, namespace, layer) {
                store.request(key);
            }
        }
        match store.ready(&clip) {
            Some(Ok(pcm)) => ClipTake::Ready(pcm),
            Some(Err(ClipError::Decode | ClipError::Read | ClipError::QueueClosed)) => {
                ClipTake::Failed(StartFailure::DecodeFailed)
            }
            None => {
                pending.push_oneshot(PendingOneshot {
                    namespace,
                    alias: alias.to_owned(),
                    variant,
                    volume,
                    pitch,
                    origin_inches,
                    snd_ent,
                    clip,
                    layer,
                    class,
                    epoch,
                    deadline: deadline_for(class),
                });
                ClipTake::Pending
            }
        }
    } else {
        ClipTake::Failed(StartFailure::NoPcm)
    }
}

fn submit_prepared_oneshot(
    commands: &mut Commands,
    pcm_assets: &mut Assets<PcmAudio>,
    shared: &mut SharedPlayAssets,
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    namespace: AssetNamespace,
    alias: &str,
    origin_inches: Option<[f32; 3]>,
    listener: Option<(Vec3, Vec3)>,
    pick: &mut SoundPickState,
    clips: Option<&mut ClipStore>,
    pending: &mut PendingStarts,
    occupancy: &mut VoiceOccupancy,
    snd_ent: Option<u32>,
    depth: u8,
    class: SoundClass,
    epoch: u64,
    pcm: PcmAudio,
    clip: &ClipKey,
    variant_index: usize,
    volume: f32,
    pitch: f32,
    layer: Option<&str>,
) -> OneshotStart {
    let sound = bank.sound_in(namespace, alias);
    let row = sound.and_then(|s| s.aliases.get(variant_index));
    let channel = sound.and_then(|s| s.ent_channel(variant_index));
    let mut world_detail: Option<String> = None;
    // Whether a sound is positional is the ent channel's call, not the caller's:
    // `channels.def` marks channels like `auto2d`, `music` and `announcer` as 2D,
    // and those play unpanned and unattenuated even when handed an entity to
    // play on. `ui_mp_suitcasebomb_timer` rides `auto2d`, which is why the
    // planted bomb ticks across the whole map.
    let positional = channel
        .and_then(|ch| bank.ent_channel(ch))
        .is_none_or(|info| info.is_3d);

    match origin_inches.filter(|_| positional) {
        Some(pos) => {
            let Some((ear, right)) = listener else {
                diag::warn!(
                    Audio,
                    "audio: world alias `{alias}` has no listener (typed gap)"
                );
                return OneshotStart::failed(StartFailure::NoListener).with_variant(variant_index);
            };
            let Some(row) = row else {
                return OneshotStart::failed(StartFailure::NoPcm).with_variant(variant_index);
            };
            let ear_inches = transform_inches(ear);
            let dist = distance_inches(ear_inches, pos);
            let Some(curve) = row.volume_falloff.as_ref() else {
                diag::warn!(
                    Audio,
                    "audio: world alias `{alias}` has no falloff curve (typed gap)"
                );
                return OneshotStart::failed(StartFailure::NoFalloffCurve)
                    .with_variant(variant_index);
            };
            let knots = shared.intern_curve(&curve.name, &curve.knots);
            let atten = snd_attenuate(&knots, dist, row.dist_min, row.dist_max);
            let emitter = Vec3::from_array(pos);
            let (pan_l, pan_r) = world_oneshot_channel_gains(ear, right, emitter, 1.0);
            if atten < 0.0 {
                diag::warn!(
                    Audio,
                    "audio: world alias `{alias}` falloff curve `{}` failed evaluation (typed gap)",
                    curve.name
                );
                return OneshotStart::failed(StartFailure::FalloffEval).with_variant(variant_index);
            }
            if atten == 0.0 {
                return OneshotStart {
                    outcome: StartOutcome::Suppressed(SuppressReason::Inaudible),
                    variant: Some(variant_index),
                    secondary: None,
                    detail: Some(falloff_detail(dist, row.dist_min, row.dist_max, atten)),
                };
            }
            if let Err(reason) = prepare_voice(commands, occupancy, bank, channel, snd_ent) {
                return OneshotStart {
                    outcome: StartOutcome::Suppressed(reason),
                    variant: Some(variant_index),
                    secondary: None,
                    detail: Some(falloff_detail(dist, row.dist_min, row.dist_max, atten)),
                };
            }
            world_detail = Some(falloff_detail(dist, row.dist_min, row.dist_max, atten));
            pick.last_variant
                .insert((namespace, alias.to_owned()), variant_index);
            let _ = shared.dry_handle(pcm_assets, clip, pcm.clone());
            let live = pcm.with_live_pan();
            let live_pan = live.live_pan().expect("with_live_pan").clone();
            live_pan.set(pan_l, pan_r);
            let handle = pcm_assets.add(live);
            let lease = voice_lease(bank, channel, snd_ent);
            let entity = crate::backend::spawn_oneshot(
                commands,
                handle,
                Volume::Linear((volume * atten).max(0.0)),
                pitch,
                epoch,
                class.scope(),
            );
            {
                let mut spawned = commands.entity(entity);
                spawned.insert(AliasPlayback {
                    namespace,
                    snd_ent,
                    alias: alias.to_owned(),
                });
                spawned.insert(Channel3d {
                    origin_inches: pos,
                    dist_min: row.dist_min,
                    dist_max: row.dist_max,
                    knots,
                    base_volume: volume.max(0.0),
                    live_pan,
                });
                if let Some(lease) = lease {
                    spawned.insert(lease);
                }
            }
            track_voice(occupancy, entity, lease);
        }
        None => {
            if let Err(reason) = prepare_voice(commands, occupancy, bank, channel, snd_ent) {
                return OneshotStart {
                    outcome: StartOutcome::Suppressed(reason),
                    variant: Some(variant_index),
                    secondary: None,
                    detail: None,
                };
            }
            pick.last_variant
                .insert((namespace, alias.to_owned()), variant_index);
            let handle = shared.dry_handle(pcm_assets, clip, pcm);
            let lease = voice_lease(bank, channel, snd_ent);
            let entity = crate::backend::spawn_oneshot(
                commands,
                handle,
                Volume::Linear(volume.max(0.0)),
                pitch,
                epoch,
                class.scope(),
            );
            {
                let mut spawned = commands.entity(entity);
                spawned.insert(AliasPlayback {
                    namespace,
                    snd_ent,
                    alias: alias.to_owned(),
                });
                if let Some(lease) = lease {
                    spawned.insert(lease);
                }
            }
            track_voice(occupancy, entity, lease);
        }
    }
    let secondary = play_secondary_layer(
        commands,
        pcm_assets,
        shared,
        bank,
        iwd,
        namespace,
        layer,
        origin_inches,
        listener,
        pick,
        clips,
        pending,
        occupancy,
        snd_ent,
        depth,
        class,
        epoch,
    );
    OneshotStart {
        outcome: StartOutcome::Submitted,
        variant: Some(variant_index),
        secondary,
        detail: world_detail,
    }
}

fn play_secondary_layer(
    commands: &mut Commands,
    pcm_assets: &mut Assets<PcmAudio>,
    shared: &mut SharedPlayAssets,
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    namespace: AssetNamespace,
    layer: Option<&str>,
    origin_inches: Option<[f32; 3]>,
    listener: Option<(Vec3, Vec3)>,
    pick: &mut SoundPickState,
    clips: Option<&mut ClipStore>,
    pending: &mut PendingStarts,
    occupancy: &mut VoiceOccupancy,
    snd_ent: Option<u32>,
    depth: u8,
    class: SoundClass,
    epoch: u64,
) -> Option<(String, StartOutcome)> {
    let Some(sec) = layer.filter(|s| !s.is_empty()) else {
        return None;
    };
    if depth >= 10 {
        return Some((sec.to_owned(), StartOutcome::Failed(StartFailure::NoPcm)));
    }
    let started = play_alias_oneshot_at(
        commands,
        pcm_assets,
        shared,
        bank,
        iwd,
        namespace,
        sec,
        origin_inches,
        listener,
        pick,
        clips,
        pending,
        occupancy,
        snd_ent,
        depth + 1,
        class,
        epoch,
    );
    Some((sec.to_owned(), started.outcome))
}
