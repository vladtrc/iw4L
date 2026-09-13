use std::collections::HashMap;
use std::sync::Arc;

use asset_iw4::snd_attenuate;
use assets::{
    AssetNamespace, GamesRoot, LoadedSoundBank, NamespaceSoundIwd, NamespaceTrees, SoundCatalog,
    load_mp_sound_bank, namespace_for_zone,
};
use bevy::{
    audio::{AudioSink, AudioSinkPlayback, Volume},
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::MatchTornDown;

use crate::backend::{AudioScope, MatchEpoch, Voice};
use crate::pcm::{LoopingPcmAudio, PcmAudio};
use crate::playback::{
    AmbientListener, MissingAliasGaps, SharedPlayAssets, SoundBank, world_oneshot_channel_gains,
};
use crate::space::{distance_inches, transform_inches};

#[derive(Component)]
pub struct MapAmbient;

pub const MAX_ACTIVE_MAP_EMITTERS: usize = 8;

pub const MIN_AUDIBLE_EMITTER_GAIN: f32 = 0.002;

#[derive(Component)]
pub struct MapEmitter {
    pub origin_inches: [f32; 3],
    pub dist_min: f32,
    pub dist_max: f32,
    pub knots: Arc<[[f32; 2]]>,
    pub base_gain: f32,
    pub pcm: Handle<PcmAudio>,

    pub live_pan: Option<crate::pcm::LivePan>,
}

#[derive(Resource, Clone)]
pub struct SoundIwd(pub Arc<NamespaceSoundIwd>);

#[derive(Resource, Default)]
pub struct MapAmbientBooted(pub bool);

#[derive(Resource, Default)]
pub(crate) struct SoundBankLoadAttempted(pub bool);

/// How long a sound bank walk may run before it is worth a line of its own.
const SOUND_BANK_WALK_STALL: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Resource)]
pub(crate) struct SoundBankWalk {
    load_key: frame::LocalLoadKey,
    zone: String,
    task: Task<SoundBankWalked>,
    started: std::time::Instant,
    stall_reported: bool,
}

struct SoundBankWalked {
    loaded: Result<LoadedSoundBank, String>,
    indices: NamespaceSoundIwd,
    lines: Vec<String>,
    namespace: AssetNamespace,
}

#[derive(Resource)]
pub(crate) struct SoundBankNamespace {
    pub(crate) zone: String,
    pub(crate) namespace: AssetNamespace,
}

pub fn stop_map_ambient(commands: &mut Commands, ambient: &Query<Entity, With<MapAmbient>>) {
    for entity in ambient.iter() {
        commands.entity(entity).despawn();
    }
}

pub(crate) fn stop_map_ambient_on_match_torn_down(
    mut torn: MessageReader<MatchTornDown>,
    ambient: Query<Entity, With<MapAmbient>>,
    mut booted: ResMut<MapAmbientBooted>,
    mut attempted: ResMut<SoundBankLoadAttempted>,
    mut epoch: ResMut<MatchEpoch>,
    walk: Option<Res<SoundBankWalk>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    mut commands: Commands,
) {
    let retired: Vec<_> = torn.read().map(|fact| fact.world_generation).collect();
    if retired.is_empty() {
        return;
    }
    epoch.bump();
    stop_map_ambient(&mut commands, &ambient);
    booted.0 = false;
    commands.remove_resource::<assets::CreateFxOneshotEmitters>();

    let walk_is_for_the_incoming_map =
        walk.as_ref()
            .zip(accepted.as_ref())
            .is_some_and(|(walk, accepted)| {
                walk.load_key == accepted.load_key
                    && !retired.contains(&frame::WorldGeneration::from_install(
                        walk.load_key.local_load_request_id,
                    ))
            });
    if !walk_is_for_the_incoming_map {
        commands.remove_resource::<SoundBankWalk>();
        attempted.0 = false;
    }
    commands.remove_resource::<SoundBankNamespace>();
    commands.remove_resource::<SoundBank>();
    commands.remove_resource::<SoundIwd>();

    commands.remove_resource::<crate::ClipStore>();
    perf::ambient_hold(i64::from(booted.0));
    diag::info!(Audio, "audio: map ambient stopped (SND_StopAmbient)");
}

pub(crate) fn start_sound_bank_walk(
    mut attempted: ResMut<SoundBankLoadAttempted>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut commands: Commands,
) {
    if attempted.0 {
        return;
    }
    let Some(accepted) = accepted else {
        return;
    };
    if abort.is_some_and(|abort| abort.0 == accepted.request_id) {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if accepted.zone.is_empty() || identity.games_root.as_os_str().is_empty() {
        return;
    }
    attempted.0 = true;
    let games = GamesRoot(identity.games_root.clone());
    let zone = accepted.zone.clone();
    let walked_zone = zone.clone();

    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move {
        // Each stage announces itself as it finishes: the walk holds AudioReady,
        // the world spawn and the host's admission behind it, so a stage that
        // never returns has to be readable from the log of the run it hung.
        let started = std::time::Instant::now();
        let stage = |what: &str, at: std::time::Instant| {
            diag::info!(
                Audio,
                "audio: sound bank walk `{walked_zone}`: {what} at {:.0}ms",
                at.elapsed().as_secs_f32() * 1000.0
            );
        };
        let loaded = load_mp_sound_bank(&games, &walked_zone);
        stage("catalog", started);
        let mut trees = NamespaceTrees::discover(&games);
        stage("namespace trees", started);
        if let Ok(zone) = assets::find_zone_file(&games, &walked_zone) {
            trees.adopt_zone(&zone.path);
        }
        stage("zone anchor", started);
        let (indices, lines) = NamespaceSoundIwd::open(&trees);
        stage("iwd archives", started);
        let map_ns = namespace_for_zone(&games, &walked_zone);
        stage("done", started);
        SoundBankWalked {
            loaded,
            indices,
            lines,
            namespace: map_ns,
        }
    });
    commands.insert_resource(SoundBankWalk {
        load_key: accepted.load_key,
        zone,
        task,
        started: std::time::Instant::now(),
        stall_reported: false,
    });
}

pub(crate) fn install_sound_bank(
    mut walk: Option<ResMut<SoundBankWalk>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    mut epoch: ResMut<MatchEpoch>,
    mut commands: Commands,
) {
    let Some(walk) = walk.as_deref_mut() else {
        return;
    };

    // `MatchLoadAccepted` is removed the moment the map walk returns, which is
    // normally *before* this one does — its absence means the load finished,
    // not that the session moved on, and dropping the bank on it loses the race
    // to whichever walk is slower on the machine. Only a *different* accepted
    // load supersedes this one.
    let superseded = accepted
        .as_ref()
        .is_some_and(|accepted| accepted.load_key != walk.load_key);
    if abort.is_some_and(|abort| abort.0 == walk.load_key.local_load_request_id) || superseded {
        diag::info!(
            Audio,
            "audio: sound bank for `{}` dropped — the session moved on",
            walk.zone
        );
        commands.remove_resource::<SoundBankWalk>();
        return;
    }
    // The load key above already says this walk belongs to the accepted load.
    // `identity.zone` is a separate display spelling the session stamps on its
    // own schedule, so gating the install on it means a bank that never
    // installs when the two never converge — and a bank that never installs
    // holds AudioReady, the world spawn, and with it the host's
    // `HostWorldReady`, down forever with nothing said.
    let identity_zone = identity.as_ref().map(|identity| identity.zone.as_str());
    if let Some(zone) = identity_zone.filter(|zone| *zone != walk.zone) {
        diag::warn!(
            Audio,
            "audio: launch identity says `{zone}` while the accepted load walked `{}` — installing on the load key",
            walk.zone
        );
    }
    let Some(walked) = future::block_on(future::poll_once(&mut walk.task)) else {
        if !walk.stall_reported && walk.started.elapsed() >= SOUND_BANK_WALK_STALL {
            walk.stall_reported = true;
            diag::warn!(
                Audio,
                "audio: sound bank walk for `{}` still running after {:.0}s — AudioReady, the world spawn and host admission all wait on it",
                walk.zone,
                walk.started.elapsed().as_secs_f32()
            );
        }
        return;
    };
    commands.remove_resource::<SoundBankWalk>();
    let SoundBankWalked {
        loaded,
        indices,
        lines,
        namespace,
    } = walked;
    for line in lines {
        diag::info!(Audio, "{line}");
    }
    match loaded {
        Ok(loaded) => {
            epoch.bump();
            if indices.is_empty() {
                diag::warn!(
                    Audio,
                    "audio: no IWD sound archives for `{}` — streamed aliases will gap",
                    walk.zone
                );
            }
            let iwd = Arc::new(indices);
            commands.insert_resource(SoundIwd(Arc::clone(&iwd)));
            let bank = Arc::new(loaded.catalog);

            commands.insert_resource(crate::ClipStore::start(Arc::clone(&bank), Some(iwd)));
            commands.insert_resource(crate::clip_store::PendingStarts::default());
            commands.insert_resource(crate::playback::SharedPlayAssets::default());
            commands.insert_resource(SoundBank(bank));
            diag::info!(
                Audio,
                "audio: sound bank ready for {} ({} zone gaps)",
                walk.zone,
                loaded.gaps.len()
            );
            commands.insert_resource(SoundBankNamespace {
                zone: walk.zone.clone(),
                namespace,
            });
        }
        Err(e) => {
            diag::warn!(
                Audio,
                "audio: sound bank load failed for {}: {e}",
                walk.zone
            );
        }
    }
}

pub(crate) fn boot_map_ambient_once(
    mut booted: ResMut<MapAmbientBooted>,
    loading: Option<Res<assets::LoadingScreen>>,
    bank: Option<Res<SoundBank>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    script_sound: Option<Res<assets::SessionMapScriptSound>>,
    namespace: Option<Res<SoundBankNamespace>>,
    epoch: Res<MatchEpoch>,
    clips: Option<Res<crate::ClipStore>>,
    ready: Res<crate::AudioReady>,
    mut commands: Commands,
    mut pcm_assets: ResMut<Assets<PcmAudio>>,
    mut looping_assets: ResMut<Assets<LoopingPcmAudio>>,
    mut shared: ResMut<SharedPlayAssets>,
    mut gaps: ResMut<MissingAliasGaps>,
) {
    if booted.0 {
        return;
    }

    if loading.is_some_and(|screen| !screen.is_complete()) {
        return;
    }
    if !ready.0 {
        return;
    }
    let Some(bank) = bank else {
        return;
    };
    let Some(identity) = identity else {
        return;
    };
    if identity.zone.is_empty() {
        return;
    }
    let Some(namespace) = namespace.filter(|ns| ns.zone == identity.zone) else {
        return;
    };
    let ambient_alias = script_sound
        .as_deref()
        .and_then(|facts| facts.0.ambient_alias.as_deref());
    start_map_ambient_prepared(
        &mut commands,
        &mut pcm_assets,
        &mut looping_assets,
        &mut shared,
        bank.0.as_ref(),
        namespace.namespace,
        &identity.zone,
        ambient_alias,
        clips.as_deref(),
        epoch.0,
        &mut gaps,
    );
    booted.0 = true;
    perf::ambient_boot(&identity.zone, ambient_alias);
    diag::info!(
        Audio,
        "audio: map ambient boot complete for {}",
        identity.zone
    );
}

fn start_map_ambient_prepared(
    commands: &mut Commands,
    pcm_assets: &mut Assets<PcmAudio>,
    looping_assets: &mut Assets<LoopingPcmAudio>,
    shared: &mut SharedPlayAssets,
    bank: &SoundCatalog,
    map_ns: AssetNamespace,
    map_name: &str,
    ambient_alias: Option<&str>,
    clips: Option<&crate::ClipStore>,
    epoch: u64,
    gaps: &mut MissingAliasGaps,
) {
    if let Some(alias) = ambient_alias {
        if let Some(pcm) = pcm_for_map_alias(clips, bank, map_ns, alias) {
            let handle = looping_assets.add(pcm.into_looping());
            let entity = crate::backend::spawn_loop(
                commands,
                handle,
                Volume::Linear(0.55),
                epoch,
                AudioScope::Match,
            );
            commands.entity(entity).insert(MapAmbient);
            diag::info!(Audio, "audio: ambient loop `{alias}`");
        } else {
            gaps.record(alias);
            diag::warn!(
                Audio,
                "audio: ambient alias `{alias}` unresolved for {map_name}"
            );
        }
    } else {
        diag::warn!(
            Audio,
            "audio: no ambientPlay in map script for `{map_name}` (typed gap)"
        );
    }

    let loops = bank.createfx_loop_sounds(map_ns, map_name);
    let mut pcm_by_alias: HashMap<String, Handle<PcmAudio>> = HashMap::new();
    let mut started = 0usize;
    let mut missed = 0usize;
    for emitter in &loops {
        let handle = if let Some(handle) = pcm_by_alias.get(&emitter.soundalias) {
            handle.clone()
        } else {
            let Some(pcm) = pcm_for_map_alias(clips, bank, map_ns, &emitter.soundalias) else {
                gaps.record(&emitter.soundalias);
                missed += 1;
                continue;
            };
            let handle = pcm_assets.add(pcm);
            pcm_by_alias.insert(emitter.soundalias.clone(), handle.clone());
            handle
        };
        let origin_inches = emitter.origin_inches;
        let row = bank
            .sound_in(map_ns, &emitter.soundalias)
            .or_else(|| {
                bank.index_unique(&emitter.soundalias)
                    .and_then(|i| bank.sounds.get(i))
            })
            .and_then(|s| s.aliases.first());
        let (dist_min, dist_max, knots, base_gain) = if let Some(row) = row {
            let knots = row
                .volume_falloff
                .as_ref()
                .map(|c| shared.intern_curve(&c.name, &c.knots))
                .unwrap_or_else(|| Arc::from(Vec::<[f32; 2]>::new()));
            if knots.is_empty() {
                diag::warn!(
                    Audio,
                    "audio: CreateFX loop `{}` has no falloff curve (typed gap)",
                    emitter.soundalias
                );
            }
            (row.dist_min, row.dist_max, knots, row.vol_min.max(0.0))
        } else {
            (0.0, 0.0, Arc::from(Vec::<[f32; 2]>::new()), 0.0)
        };
        commands.spawn((
            MapAmbient,
            MapEmitter {
                origin_inches,
                dist_min,
                dist_max,
                knots,
                base_gain,
                pcm: handle,
                live_pan: None,
            },
            Transform::from_translation(Vec3::from_array(origin_inches)),
        ));
        started += 1;
    }
    if started > 0 {
        diag::info!(
            Audio,
            "audio: createfx emitters {started}/{} ({} unique, max {} active; {missed} miss) for {map_name}",
            loops.len(),
            pcm_by_alias.len(),
            MAX_ACTIVE_MAP_EMITTERS,
        );
    } else if !loops.is_empty() {
        diag::info!(
            Audio,
            "audio: createfx listed {} emitters but none resolved PCM",
            loops.len()
        );
    }

    let oneshots = bank.createfx_oneshots(map_ns, map_name);
    let oneshot_count = oneshots.len();
    commands.insert_resource(assets::CreateFxOneshotEmitters(oneshots));
    if oneshot_count > 0 {
        diag::info!(
            Audio,
            "audio: createfx oneshots {oneshot_count} parsed for {map_name} (host markers; no FX_Register play)"
        );
    }
}

pub(crate) fn emitter_gain(emitter: &MapEmitter, ear_inches: [f32; 3]) -> f32 {
    let dist = distance_inches(ear_inches, emitter.origin_inches);
    if emitter.knots.is_empty() {
        return 0.0;
    }
    let atten = snd_attenuate(&emitter.knots, dist, emitter.dist_min, emitter.dist_max);
    if atten < 0.0 {
        0.0
    } else {
        emitter.base_gain * atten
    }
}

pub fn update_map_emitter_gain(
    listeners: Query<&Transform, With<AmbientListener>>,
    mut emitters: Query<(Entity, &mut MapEmitter, Has<Voice>)>,
    mut sinks: Query<&mut AudioSink, With<MapEmitter>>,
    pcm_assets: Res<Assets<PcmAudio>>,
    mut looping_assets: ResMut<Assets<LoopingPcmAudio>>,
    mut commands: Commands,
    settings: Res<frame::GameSettings>,
    epoch: Res<MatchEpoch>,
    mut ranked: Local<Vec<(Entity, f32, Handle<PcmAudio>, [f32; 3], bool)>>,
) {
    let n = listeners.iter().len();
    if n == 0 {
        return;
    }
    if n > 1 {
        panic!("second listener / amp maxRadius gate not ported");
    }
    let Some(listener) = listeners.iter().next() else {
        return;
    };
    let ear = listener.translation;
    let r = listener.rotation * Vec3::X;
    let right = Vec3::new(r.x, r.y, r.z);
    let ear_inches = transform_inches(ear);

    ranked.clear();
    ranked.extend(emitters.iter().map(|(entity, emitter, has_player)| {
        (
            entity,
            emitter_gain(&emitter, ear_inches),
            emitter.pcm.clone(),
            emitter.origin_inches,
            has_player,
        )
    }));
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (rank, (entity, gain, dry_pcm, origin, has_player)) in ranked.iter().enumerate() {
        let entity = *entity;
        let gain = *gain;
        let origin = *origin;
        let has_player = *has_player;
        let audible = rank < MAX_ACTIVE_MAP_EMITTERS && gain >= MIN_AUDIBLE_EMITTER_GAIN;
        let origin_v = Vec3::from_array(origin);
        let (pan_l, pan_r) = world_oneshot_channel_gains(ear, right, origin_v, 1.0);
        if audible {
            if !has_player {
                let Some(dry) = pcm_assets.get(dry_pcm).cloned() else {
                    continue;
                };
                let live = dry.with_live_pan();
                let live_pan = live.live_pan().expect("with_live_pan").clone();
                live_pan.set(pan_l, pan_r);
                let handle = looping_assets.add(live.into_looping());
                if let Ok((_, mut emitter, _)) = emitters.get_mut(entity) {
                    emitter.live_pan = Some(live_pan);
                }
                crate::backend::attach_loop(
                    &mut commands,
                    entity,
                    handle,
                    Volume::Linear(gain),
                    epoch.0,
                );
            } else {
                if let Ok((_, emitter, _)) = emitters.get(entity)
                    && let Some(pan) = emitter.live_pan.as_ref()
                {
                    pan.set(pan_l, pan_r);
                }
                if let Ok(mut sink) = sinks.get_mut(entity) {
                    sink.set_volume(Volume::Linear(gain * settings.master_volume));
                    if sink.is_paused() {
                        sink.play();
                    }
                }
            }
        } else if has_player {
            if let Ok((_, mut emitter, _)) = emitters.get_mut(entity) {
                emitter.live_pan = None;
            }
            crate::backend::detach_loop(&mut commands, entity);
        }
    }
}

fn pcm_for_map_alias(
    clips: Option<&crate::ClipStore>,
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
) -> Option<PcmAudio> {
    let clips = clips?;
    for key in crate::clip_store::clip_keys_for_alias(bank, ns, alias) {
        if let Some(Ok(pcm)) = clips.ready(&key) {
            return Some(pcm);
        }
    }
    None
}
