use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

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

#[derive(Resource)]
pub(crate) struct SoundBankWalk {
    zone: String,
    task: Task<SoundBankWalked>,
}

struct SoundBankWalked {
    loaded: Result<LoadedSoundBank, String>,
    indices: NamespaceSoundIwd,
    lines: Vec<String>,
    namespace: AssetNamespace,
}

#[derive(Resource)]
pub(crate) struct SoundBankNamespace {
    zone: String,
    namespace: AssetNamespace,
}

#[derive(Resource)]
pub(crate) struct MapAmbientPrepare {
    zone: String,
    namespace: AssetNamespace,
    task: Task<(HashMap<String, Option<PcmAudio>>, f32)>,
}

#[derive(Resource, Default)]
pub(crate) struct PreparedMapAmbientPcm {
    zone: String,
    namespace: AssetNamespace,
    by_alias: HashMap<String, Option<PcmAudio>>,
}

impl PreparedMapAmbientPcm {
    pub(crate) fn namespace(&self) -> AssetNamespace {
        self.namespace
    }

    pub(crate) fn zone(&self) -> &str {
        &self.zone
    }

    pub(crate) fn alias_names(&self) -> impl Iterator<Item = &str> {
        self.by_alias.keys().map(String::as_str)
    }
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
    mut prepared: ResMut<PreparedMapAmbientPcm>,
    mut epoch: ResMut<MatchEpoch>,
    walk: Option<Res<SoundBankWalk>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 {
        return;
    }
    epoch.bump();
    stop_map_ambient(&mut commands, &ambient);
    booted.0 = false;
    *prepared = PreparedMapAmbientPcm::default();
    commands.remove_resource::<assets::CreateFxOneshotEmitters>();
    commands.remove_resource::<MapAmbientPrepare>();

    let walk_is_for_the_incoming_map = walk
        .as_ref()
        .zip(accepted.as_ref())
        .is_some_and(|(walk, accepted)| walk.zone == accepted.zone);
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
    identity: Option<Res<frame::LaunchIdentity>>,
    mut commands: Commands,
) {
    if attempted.0 {
        return;
    }
    let Some(accepted) = accepted else {
        return;
    };
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
        let loaded = load_mp_sound_bank(&games, &walked_zone);

        let mut trees = NamespaceTrees::discover(&games);
        if let Ok(zone) = assets::find_zone_file(&games, &walked_zone) {
            trees.adopt_zone(&zone.path);
        }
        let (indices, lines) = NamespaceSoundIwd::open(&trees);
        let map_ns = namespace_for_zone(&games, &walked_zone);
        if let Ok(loaded) = loaded.as_ref() {
            crate::map_doors::prepare(&loaded.catalog, map_ns, Some(&indices));
        }
        SoundBankWalked {
            loaded,
            indices,
            lines,
            namespace: map_ns,
        }
    });
    commands.insert_resource(SoundBankWalk { zone, task });
}

pub(crate) fn start_map_ambient_prepare(
    bank: Option<Res<SoundBank>>,
    iwd: Option<Res<SoundIwd>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    script_sound: Option<Res<assets::SessionMapScriptSound>>,
    prepared_pcm: Res<PreparedMapAmbientPcm>,
    prepare: Option<Res<MapAmbientPrepare>>,
    namespace: Option<Res<SoundBankNamespace>>,
    mut commands: Commands,
) {
    if prepare.is_some() {
        return;
    }
    let (Some(bank), Some(iwd), Some(identity), Some(script_sound), Some(namespace)) =
        (bank, iwd, identity, script_sound, namespace)
    else {
        return;
    };
    if identity.zone.is_empty() || namespace.zone != identity.zone {
        return;
    }
    if prepared_pcm.zone == identity.zone {
        return;
    }
    let ambient_alias = script_sound.0.ambient_alias.clone();
    let catalog = Arc::clone(&bank.0);
    let indices = Arc::clone(&iwd.0);
    let map_ns = namespace.namespace;
    let zone = identity.zone.clone();
    let walked_zone = zone.clone();

    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move {
        let started = Instant::now();
        let mut aliases = HashSet::new();
        if let Some(alias) = ambient_alias {
            aliases.insert(alias);
        }
        for emitter in catalog.createfx_loop_sounds(map_ns, &walked_zone) {
            aliases.insert(emitter.soundalias);
        }
        let mut prepared = HashMap::with_capacity(aliases.len());
        for alias in aliases {
            let pcm = resolve_alias_pcm(&catalog, Some(indices.as_ref()), map_ns, &alias);
            prepared.insert(alias, pcm);
        }
        (prepared, started.elapsed().as_secs_f32() * 1000.0)
    });
    commands.insert_resource(MapAmbientPrepare {
        zone,
        namespace: map_ns,
        task,
    });
}

pub(crate) fn install_map_ambient_pcm(
    prepare: Option<ResMut<MapAmbientPrepare>>,
    mut prepared_pcm: ResMut<PreparedMapAmbientPcm>,
    mut commands: Commands,
) {
    let Some(mut prepare) = prepare else {
        return;
    };
    let Some((prepared, prepare_ms)) = future::block_on(future::poll_once(&mut prepare.task))
    else {
        return;
    };
    let ready = prepared.values().filter(|pcm| pcm.is_some()).count();
    diag::info!(
        Audio,
        "audio: prepared map ambient PCM {ready}/{} for {} in {prepare_ms:.1}ms off the frame",
        prepared.len(),
        prepare.zone,
    );
    prepared_pcm.by_alias = prepared;
    prepared_pcm.zone.clone_from(&prepare.zone);
    prepared_pcm.namespace = prepare.namespace;
    commands.remove_resource::<MapAmbientPrepare>();
}

pub(crate) fn install_sound_bank(
    mut walk: Option<ResMut<SoundBankWalk>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    mut epoch: ResMut<MatchEpoch>,
    mut commands: Commands,
) {
    let Some(walk) = walk.as_deref_mut() else {
        return;
    };

    if identity
        .as_ref()
        .is_some_and(|identity| identity.zone != walk.zone)
    {
        if accepted.is_some_and(|accepted| accepted.zone == walk.zone) {
            return;
        }

        let zone = walk.zone.clone();
        commands.remove_resource::<SoundBankWalk>();
        diag::info!(
            Audio,
            "audio: sound bank for `{zone}` dropped — the session moved on"
        );
        return;
    }
    let Some(walked) = future::block_on(future::poll_once(&mut walk.task)) else {
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
    prepared: Res<PreparedMapAmbientPcm>,
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
    if prepared.zone != identity.zone {
        return;
    }
    let ambient_alias = script_sound
        .as_deref()
        .and_then(|facts| facts.0.ambient_alias.as_deref());
    start_map_ambient_prepared(
        &mut commands,
        &mut pcm_assets,
        &mut looping_assets,
        &mut shared,
        bank.0.as_ref(),
        prepared.namespace,
        &identity.zone,
        ambient_alias,
        &prepared.by_alias,
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
    prepared: &HashMap<String, Option<PcmAudio>>,
    clips: Option<&crate::ClipStore>,
    epoch: u64,
    gaps: &mut MissingAliasGaps,
) {
    if let Some(alias) = ambient_alias {
        if let Some(pcm) = pcm_for_map_alias(prepared, clips, bank, map_ns, alias) {
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
            let Some(pcm) = pcm_for_map_alias(prepared, clips, bank, map_ns, &emitter.soundalias)
            else {
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
    prepared: &HashMap<String, Option<PcmAudio>>,
    clips: Option<&crate::ClipStore>,
    bank: &SoundCatalog,
    ns: AssetNamespace,
    alias: &str,
) -> Option<PcmAudio> {
    if let Some(pcm) = prepared.get(alias).and_then(Option::as_ref).cloned() {
        return Some(pcm);
    }
    let clips = clips?;
    for key in crate::clip_store::clip_keys_for_alias(bank, ns, alias) {
        if let Some(Ok(pcm)) = clips.ready(&key) {
            return Some(pcm);
        }
    }
    None
}

pub(crate) fn resolve_alias_pcm(
    bank: &SoundCatalog,
    iwd: Option<&NamespaceSoundIwd>,
    ns: AssetNamespace,
    alias: &str,
) -> Option<PcmAudio> {
    for key in crate::clip_store::clip_keys_for_alias(bank, ns, alias) {
        if let Ok(prepared) = crate::clip_store::prepare_clip_now(bank, iwd, &key)
            && let Some(pcm) = prepared.into_audio()
        {
            return Some(pcm);
        }
    }
    None
}
