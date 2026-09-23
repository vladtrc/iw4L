use std::collections::HashMap;
use std::sync::Arc;

use asset_iw4::snd_attenuate;
use assets::{
    AssetNamespace, GamesRoot, LoadedSoundBank, NamespaceSoundIwd, NamespaceTrees, SoundCatalog,
    compose_sound_bank, gather_sound_sources, namespace_for_zone,
};
use bevy::{
    audio::{AudioSink, AudioSinkPlayback, Volume},
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};
use frame::{MatchTornDown, ReturnedToMenu};

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

const SOUND_BANK_COMPOSE_STALL: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Resource)]
pub(crate) struct SoundBankCompose {
    load_key: frame::LocalLoadKey,
    zone: String,
    iwd: IwdOpen,
    bank: Option<Task<ComposedBank>>,
    common_profile_id: u64,
    products_id: u64,
    started: std::time::Instant,
    stall_reported: bool,
}

enum IwdOpen {
    Opening(Task<(NamespaceSoundIwd, Vec<String>)>),
    Open(Arc<NamespaceSoundIwd>),
}

struct ComposedBank {
    loaded: Result<(Arc<SoundCatalog>, usize), String>,
    namespace: AssetNamespace,
    reused: bool,
}

#[derive(Resource, Default)]
pub(crate) struct ResidentSoundBank(Option<ResidentBank>);

struct ResidentBank {
    zone: String,
    iwd: Arc<NamespaceSoundIwd>,
    bank: Option<ResidentComposed>,
}

struct ResidentComposed {
    products_id: u64,
    common_profile_id: u64,
    catalog: Arc<SoundCatalog>,
    namespace: AssetNamespace,
    gaps: usize,
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

pub(crate) fn stop_map_ambient_on_match_end(
    mut torn: MessageReader<MatchTornDown>,
    mut returned: MessageReader<ReturnedToMenu>,
    ambient: Query<Entity, With<MapAmbient>>,
    mut booted: ResMut<MapAmbientBooted>,
    mut attempted: ResMut<SoundBankLoadAttempted>,
    mut epoch: ResMut<MatchEpoch>,
    compose: Option<Res<SoundBankCompose>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    mut commands: Commands,
) {
    let retired: Vec<_> = torn.read().map(|fact| fact.world_generation).collect();
    let left_session = returned.read().count() > 0;
    if retired.is_empty() && !left_session {
        return;
    }
    epoch.bump();
    stop_map_ambient(&mut commands, &ambient);
    booted.0 = false;
    commands.remove_resource::<assets::CreateFxOneshotEmitters>();

    let compose_is_for_the_incoming_map = !left_session
        && compose
            .as_ref()
            .zip(accepted.as_ref())
            .is_some_and(|(compose, accepted)| {
                compose.load_key == accepted.load_key
                    && !retired.contains(&frame::WorldGeneration::from_install(
                        compose.load_key.local_load_request_id,
                    ))
            });
    if !compose_is_for_the_incoming_map {
        commands.remove_resource::<SoundBankCompose>();
        commands.remove_resource::<assets::PreparedMatchSound>();
        attempted.0 = false;
    }
    commands.queue(|world: &mut bevy::prelude::World| {
        frame::retire::retire_resources(world, |batch| {
            batch
                .resource::<SoundBankNamespace>()
                .resource::<SoundBank>()
                .resource::<SoundIwd>()
                .resource::<crate::ClipStore>();
        });
    });
    perf::ambient_hold(i64::from(booted.0));
    diag::info!(Audio, "audio: map ambient stopped (SND_StopAmbient)");
}

pub(crate) fn start_sound_bank_compose(
    mut attempted: ResMut<SoundBankLoadAttempted>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    resident: Res<ResidentSoundBank>,
    mut commands: Commands,
) {
    if attempted.0 {
        return;
    }
    if silent.is_some() {
        attempted.0 = true;
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
    let opened_zone = zone.clone();

    let iwd = match resident.0.as_ref().filter(|resident| resident.zone == zone) {
        Some(resident) => {
            diag::info!(Audio, "audio: sound archives for `{zone}` reused");
            IwdOpen::Open(Arc::clone(&resident.iwd))
        }
        None => {
            let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
            IwdOpen::Opening(pool.spawn(async move {
                let mut trees = NamespaceTrees::discover(&games);
                if let Ok(zone) = assets::find_zone_file(&games, &opened_zone) {
                    trees.adopt_zone(&zone.path);
                }
                NamespaceSoundIwd::open(&trees)
            }))
        }
    };
    commands.insert_resource(SoundBankCompose {
        load_key: accepted.load_key,
        zone,
        iwd,
        bank: None,
        common_profile_id: 0,
        products_id: 0,
        started: std::time::Instant::now(),
        stall_reported: false,
    });
}

pub(crate) fn install_sound_bank(
    mut compose: Option<ResMut<SoundBankCompose>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    abort: Option<Res<assets::MatchLoadAbort>>,
    mut map_sound: Option<ResMut<assets::PreparedMatchSound>>,
    mut epoch: ResMut<MatchEpoch>,
    resident_clips: Res<crate::clip_store::ResidentClipCache>,
    mut resident: ResMut<ResidentSoundBank>,
    mut commands: Commands,
) {
    let Some(compose) = compose.as_deref_mut() else {
        return;
    };

    let superseded = accepted
        .as_ref()
        .is_some_and(|accepted| accepted.load_key != compose.load_key);
    if abort.is_some_and(|abort| abort.0 == compose.load_key.local_load_request_id) || superseded {
        diag::info!(
            Audio,
            "audio: sound bank for `{}` dropped — the session moved on",
            compose.zone
        );
        commands.remove_resource::<SoundBankCompose>();
        return;
    }
    // Install on the load key, not `identity.zone`: gating on a display spelling
    // that never converges would hold AudioReady and the world spawn forever.
    let identity_zone = identity.as_ref().map(|identity| identity.zone.as_str());
    if let Some(zone) = identity_zone.filter(|zone| *zone != compose.zone) {
        diag::warn!(
            Audio,
            "audio: launch identity says `{zone}` while the accepted load walked `{}` — installing on the load key",
            compose.zone
        );
    }

    if let IwdOpen::Opening(task) = &mut compose.iwd {
        if let Some((indices, lines)) = future::block_on(future::poll_once(task)) {
            for line in lines {
                diag::info!(Audio, "{line}");
            }
            compose.iwd = IwdOpen::Open(Arc::new(indices));
        }
    }
    if compose.bank.is_none() {
        if let Some(arrived) = map_sound.as_deref_mut() {
            if arrived.load_key == compose.load_key {
                compose.common_profile_id = arrived.common_profile_id;
                let products_id = arrived.products_id;
                let map = std::mem::replace(&mut arrived.sound, Err(String::new()));
                commands.remove_resource::<assets::PreparedMatchSound>();
                let kept = resident
                    .0
                    .as_ref()
                    .filter(|resident| resident.zone == compose.zone)
                    .and_then(|resident| resident.bank.as_ref())
                    .filter(|bank| {
                        bank.products_id == products_id
                            && bank.common_profile_id == compose.common_profile_id
                    })
                    .map(|bank| ComposedBank {
                        loaded: Ok((Arc::clone(&bank.catalog), bank.gaps)),
                        namespace: bank.namespace,
                        reused: true,
                    });
                let games = identity
                    .as_ref()
                    .map(|identity| GamesRoot(identity.games_root.clone()));
                let zone = compose.zone.clone();
                let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
                compose.bank = Some(pool.spawn(async move {
                    if let Some(kept) = kept {
                        return kept;
                    }
                    let Some(games) = games else {
                        return ComposedBank {
                            loaded: Err("no launch identity to find the zones by".to_owned()),
                            namespace: AssetNamespace::Iw4,
                            reused: false,
                        };
                    };
                    let namespace = namespace_for_zone(&games, &zone);
                    let loaded = assets::find_zone_file(&games, &zone).map(|found| {
                        let sources = gather_sound_sources(&games, &found.path);
                        let LoadedSoundBank { catalog, gaps, .. } =
                            compose_sound_bank(sources, &zone, namespace, map);
                        (Arc::new(catalog), gaps.len())
                    });
                    ComposedBank {
                        loaded,
                        namespace,
                        reused: false,
                    }
                }));
                compose.products_id = products_id;
            }
        }
    }

    let composed = match (&compose.iwd, compose.bank.as_mut()) {
        (IwdOpen::Open(_), Some(bank)) => future::block_on(future::poll_once(bank)),
        _ => None,
    };
    let Some(composed) = composed else {
        if !compose.stall_reported && compose.started.elapsed() >= SOUND_BANK_COMPOSE_STALL {
            compose.stall_reported = true;
            diag::warn!(
                Audio,
                "audio: sound bank for `{}` still not composed after {:.0}s (archives {}, map sound {}) — AudioReady, the world spawn and host admission all wait on it",
                compose.zone,
                compose.started.elapsed().as_secs_f32(),
                if matches!(compose.iwd, IwdOpen::Open(_)) {
                    "open"
                } else {
                    "opening"
                },
                if compose.bank.is_some() {
                    "arrived"
                } else {
                    "pending"
                },
            );
        }
        return;
    };
    commands.remove_resource::<SoundBankCompose>();
    let IwdOpen::Open(iwd) = std::mem::replace(&mut compose.iwd, IwdOpen::Open(Arc::default()))
    else {
        unreachable!("the bank is polled only once the archives are open");
    };
    let ComposedBank {
        loaded,
        namespace,
        reused,
    } = composed;
    match loaded {
        Ok((bank, gaps)) => {
            epoch.bump();
            if iwd.is_empty() {
                diag::warn!(
                    Audio,
                    "audio: no IWD sound archives for `{}` — streamed aliases will gap",
                    compose.zone
                );
            }
            commands.insert_resource(SoundIwd(Arc::clone(&iwd)));
            resident.0 = Some(ResidentBank {
                zone: compose.zone.clone(),
                iwd: Arc::clone(&iwd),
                bank: Some(ResidentComposed {
                    products_id: compose.products_id,
                    common_profile_id: compose.common_profile_id,
                    catalog: Arc::clone(&bank),
                    namespace,
                    gaps,
                }),
            });

            let new_clips = crate::ClipStore::start_with_common(
                Arc::clone(&bank),
                Some(iwd),
                compose.common_profile_id,
                Some(resident_clips.clone()),
            );
            commands.queue(move |world: &mut World| {
                frame::retire::retire_resources(world, |batch| {
                    batch.resource::<crate::ClipStore>();
                });
                world.insert_resource(new_clips);
            });
            commands.insert_resource(crate::clip_store::PendingStarts::default());
            commands.insert_resource(crate::playback::SharedPlayAssets::default());
            commands.insert_resource(SoundBank(bank));
            diag::info!(
                Audio,
                "audio: sound bank {} for {} ({} zone gaps) in {:.0}ms",
                if reused { "reused" } else { "ready" },
                compose.zone,
                gaps,
                compose.started.elapsed().as_secs_f32() * 1000.0
            );
            commands.insert_resource(SoundBankNamespace {
                zone: compose.zone.clone(),
                namespace,
            });
        }
        Err(e) => {
            diag::warn!(
                Audio,
                "audio: sound bank load failed for {}: {e}",
                compose.zone
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
