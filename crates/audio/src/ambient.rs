use std::sync::Arc;

use asset_audio::SoundCatalog;
use asset_core::FamilyId;
use asset_transport::GamesRoot;
use assets::{NamespaceSoundIwd, NamespaceTrees};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool, futures_lite::future},
};

use crate::backend::AudioScope;
use crate::playback::SoundBank;
use crate::sources::{DesiredSource, SourceCueRequest, SourceKey, SourceRenderGroup};

pub(crate) const MAP_BED_SLOT: u32 = 2;
const MAP_EMITTER_SLOT: u32 = 3;

#[derive(Resource, Default)]
pub(crate) struct MapSources {
    pub desired: Vec<DesiredSource>,
}

#[derive(Resource, Clone)]
pub struct SoundIwd(pub Arc<NamespaceSoundIwd>);

#[derive(Resource, Default)]
pub struct MapAmbientBooted(pub bool);

#[derive(Resource, Default)]
pub struct LoadingAudio {
    owner: Option<frame::MatchKey>,
    attempted: Option<frame::MatchKey>,
    status: LoadingAudioStatus,
}

impl LoadingAudio {
    pub fn owns(&self, key: frame::MatchKey) -> bool {
        self.owner == Some(key)
    }

    pub fn status(&self) -> &LoadingAudioStatus {
        &self.status
    }
}

const SOUND_BANK_COMPOSE_STALL: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Resource)]
pub(crate) struct SoundBankCompose {
    match_key: frame::MatchKey,
    zone: String,
    families: frame::AssetFamilies,
    iwd: IwdOpen,
    bank: Option<Task<ComposedBank>>,
    common_profile_id: u64,
    started: std::time::Instant,
    wait: Option<SoundBankWait>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SoundBankWaitStage {
    Archives,
    MapSound,
    Composition,
}

struct SoundBankWait {
    stage: SoundBankWaitStage,
    started: std::time::Instant,
    reported: bool,
    progress: Option<Vec<asset_transport::StageSnapshot>>,
    sampled_at: Option<std::time::Instant>,
}

impl SoundBankWait {
    fn update(
        wait: &mut Option<Self>,
        stage: SoundBankWaitStage,
        now: std::time::Instant,
        progress: Option<&asset_transport::LoadProgress>,
    ) -> Option<std::time::Duration> {
        if wait.as_ref().is_none_or(|wait| wait.stage != stage) {
            *wait = Some(Self {
                stage,
                started: now,
                reported: false,
                progress: None,
                sampled_at: None,
            });
        }
        let wait = wait.as_mut()?;
        if stage == SoundBankWaitStage::MapSound
            && let Some(progress) = progress
            && wait.sampled_at.is_none_or(|sampled| {
                now.saturating_duration_since(sampled) >= std::time::Duration::from_secs(1)
            })
        {
            let stages: Vec<_> = progress
                .snapshot()
                .stages
                .into_iter()
                .filter(|stage| {
                    use asset_transport::StageId;
                    matches!(
                        stage.key.id,
                        StageId::MapAssets
                            | StageId::CommonAssets
                            | StageId::Localization
                            | StageId::Images
                    )
                })
                .collect();
            if wait.progress.as_ref() != Some(&stages) {
                wait.started = now;
                wait.reported = false;
                wait.progress = Some(stages);
            }
            wait.sampled_at = Some(now);
        }
        let elapsed = now.saturating_duration_since(wait.started);
        if !wait.reported && elapsed >= SOUND_BANK_COMPOSE_STALL {
            wait.reported = true;
            Some(elapsed)
        } else {
            None
        }
    }
}

enum IwdOpen {
    Opening(Task<(NamespaceSoundIwd, Vec<String>)>),
    Open(Arc<NamespaceSoundIwd>),
}

struct ComposedBank {
    loaded: Result<(Arc<SoundCatalog>, usize), String>,
    namespace: FamilyId,
}

#[derive(Resource, Default)]
pub(crate) struct ResidentSoundBank(Option<ResidentBank>);

struct ResidentBank {
    zone: String,
    families: frame::AssetFamilies,
    iwd: Arc<NamespaceSoundIwd>,
}

#[derive(Resource)]
pub(crate) struct SoundBankNamespace {
    pub(crate) zone: String,
    pub(crate) namespace: FamilyId,
}

pub(crate) fn start_sound_bank_compose(
    mut loading: ResMut<LoadingAudio>,
    accepted: Option<Res<assets::MatchLoadAccepted>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    silent: Option<Res<crate::AudioSilent>>,
    resident: Res<ResidentSoundBank>,
    mut commands: Commands,
) {
    let Some(accepted) = accepted else {
        return;
    };
    if loading.attempted == Some(accepted.match_key) {
        return;
    }
    loading.owner = Some(accepted.match_key);
    loading.status = LoadingAudioStatus::Pending;
    if silent.is_some() {
        loading.status = LoadingAudioStatus::Ready;
        loading.attempted = Some(accepted.match_key);
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if accepted.zone.is_empty() || identity.games_root.as_os_str().is_empty() {
        return;
    }
    loading.attempted = Some(accepted.match_key);
    let games = GamesRoot(identity.games_root.clone());
    let zone = accepted.zone.clone();
    let opened_zone = zone.clone();
    let families = accepted.families.clone();

    let iwd = match resident
        .0
        .as_ref()
        .filter(|resident| resident.zone == zone && resident.families == families)
    {
        Some(resident) => {
            diag::info!(Audio, "audio: sound archives for `{zone}` reused");
            IwdOpen::Open(Arc::clone(&resident.iwd))
        }
        None => {
            let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
            let families = families.clone();
            IwdOpen::Opening(pool.spawn(async move {
                let mut trees = NamespaceTrees::discover(&games);
                if let Ok(zone) = asset_transport::find_zone_file(&games, &opened_zone) {
                    trees.adopt_zone(&zone.path);
                }
                trees.retain(|ns| families.enabled(ns.as_str()));
                NamespaceSoundIwd::open(&trees)
            }))
        }
    };
    let match_key = accepted.match_key;
    commands.queue(move |world: &mut World| {
        if *world.resource::<State<frame::MatchScope>>().get() != frame::MatchScope::Loading {
            return;
        }
        frame::scope::insert(
            world,
            SoundBankCompose {
                match_key,
                zone,
                families,
                iwd,
                bank: None,
                common_profile_id: 0,
                started: std::time::Instant::now(),
                wait: None,
            },
            frame::MatchScope::Loading,
        );
    });
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct SoundBankResidency<'w> {
    clips: Res<'w, crate::clip_store::ResidentClipCache>,
    archives: ResMut<'w, ResidentSoundBank>,
}

impl SoundBankResidency<'_> {
    fn start_clips(
        &mut self,
        compose: &SoundBankCompose,
        bank: &Arc<SoundCatalog>,
        iwd: Arc<NamespaceSoundIwd>,
    ) -> crate::ClipStore {
        self.archives.0 = Some(ResidentBank {
            zone: compose.zone.clone(),
            families: compose.families.clone(),
            iwd: Arc::clone(&iwd),
        });

        crate::ClipStore::start_with_common(
            Arc::clone(bank),
            Some(iwd),
            compose.common_profile_id,
            Some(self.clips.clone()),
        )
    }
}

pub(crate) fn install_sound_bank(
    mut loading: ResMut<LoadingAudio>,
    mut compose: Option<ResMut<SoundBankCompose>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut map_sound: Option<ResMut<assets::PreparedMatchSound>>,
    process: Option<Res<assets::MapLoadProcess>>,
    mut residency: SoundBankResidency,
    mut commands: Commands,
) {
    let Some(compose) = compose.as_deref_mut() else {
        return;
    };

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
            if arrived.match_key == compose.match_key {
                compose.common_profile_id = arrived.common_profile_id;
                let map = std::mem::replace(&mut arrived.sound, Err(String::new()));
                let match_key = compose.match_key;
                commands.queue(move |world: &mut World| {
                    if world
                        .get_resource::<assets::PreparedMatchSound>()
                        .is_some_and(|sound| sound.match_key == match_key)
                    {
                        world.remove_resource::<assets::PreparedMatchSound>();
                    }
                });
                let namespace = arrived.namespace;
                let gaps = arrived.gaps;
                let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
                compose.bank = Some(pool.spawn(async move {
                    ComposedBank {
                        loaded: map.map(|catalog| (Arc::new(catalog), gaps)),
                        namespace,
                    }
                }));
            }
        }
    }

    let composed = match (&compose.iwd, compose.bank.as_mut()) {
        (IwdOpen::Open(_), Some(bank)) => future::block_on(future::poll_once(bank)),
        _ => None,
    };
    let Some(composed) = composed else {
        let stage = match (&compose.iwd, &compose.bank) {
            (IwdOpen::Opening(_), _) => SoundBankWaitStage::Archives,
            (IwdOpen::Open(_), None) => SoundBankWaitStage::MapSound,
            (IwdOpen::Open(_), Some(_)) => SoundBankWaitStage::Composition,
        };
        let progress = process
            .as_ref()
            .filter(|process| process.match_key == compose.match_key)
            .map(|process| &process.progress);
        if let Some(elapsed) = SoundBankWait::update(
            &mut compose.wait,
            stage,
            std::time::Instant::now(),
            progress,
        ) {
            diag::warn!(
                Audio,
                "audio: sound bank for `{}` waiting on {stage:?} with no preparation progress for {:.0}s; audio readiness is pending",
                compose.zone,
                elapsed.as_secs_f32()
            );
        }
        return;
    };
    let match_key = compose.match_key;
    commands.queue(move |world: &mut World| {
        if world
            .get_resource::<SoundBankCompose>()
            .is_some_and(|compose| compose.match_key == match_key)
        {
            world.remove_resource::<SoundBankCompose>();
        }
    });
    let IwdOpen::Open(iwd) = std::mem::replace(&mut compose.iwd, IwdOpen::Open(Arc::default()))
    else {
        unreachable!("the bank is polled only once the archives are open");
    };
    let ComposedBank { loaded, namespace } = composed;
    match loaded {
        Ok((bank, gaps)) => {
            loading.status = LoadingAudioStatus::Ready;
            if iwd.is_empty() {
                diag::warn!(
                    Audio,
                    "audio: no IWD sound archives for `{}` — streamed aliases will gap",
                    compose.zone
                );
            }
            let sound_iwd = SoundIwd(Arc::clone(&iwd));

            let new_clips = residency.start_clips(compose, &bank, iwd);

            diag::info!(
                Audio,
                "audio: sound bank ready for {} ({} zone gaps) in {:.0}ms",
                compose.zone,
                gaps,
                compose.started.elapsed().as_secs_f32() * 1000.0
            );
            let sound_namespace = SoundBankNamespace {
                zone: compose.zone.clone(),
                namespace,
            };
            let match_key = compose.match_key;
            commands.queue(move |world: &mut World| {
                if *world.resource::<State<frame::MatchScope>>().get() != frame::MatchScope::Loading
                {
                    return;
                }
                frame::scope::insert(
                    world,
                    PreparedMatchAudio {
                        match_key,
                        iwd: sound_iwd,
                        clips: new_clips,
                        bank: SoundBank(bank),
                        namespace: sound_namespace,
                    },
                    frame::MatchScope::Loading,
                );
            });
        }
        Err(e) => {
            loading.status = LoadingAudioStatus::Failed(e.to_string());
            diag::warn!(
                Audio,
                "audio: sound bank load failed for {}: {e}",
                compose.zone
            );
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MapAmbientBoot<'w, 's> {
    booted: ResMut<'w, MapAmbientBooted>,
    loading: Option<Res<'w, assets::LoadingScreen>>,
    ready: Res<'w, crate::AudioReady>,
    runtime: Res<'w, crate::AudioRuntime>,
    sources: ResMut<'w, MapSources>,
    commands: Commands<'w, 's>,
}

impl MapAmbientBoot<'_, '_> {
    fn boot(
        mut self,
        presented: Option<Res<net::PresentedSnapshot>>,
        bank: Option<Res<SoundBank>>,
        identity: Option<Res<frame::LaunchIdentity>>,
        script_sound: Option<Res<asset_audio::SessionMapScriptSound>>,
        namespace: Option<Res<SoundBankNamespace>>,
    ) {
        if self.booted.0 {
            return;
        }

        if self.loading.is_some_and(|screen| !screen.is_complete()) {
            return;
        }
        if !self.ready.0.is_ready() {
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
        let scripted = presented
            .as_ref()
            .and_then(|p| p.snapshot())
            .is_some_and(|s| s.meta.objectives.ambient.is_some());
        let ambient_alias = script_sound
            .as_deref()
            .and_then(|facts| facts.0.ambient_alias.as_deref())
            .filter(|_| !scripted);
        if self.runtime.media_for_bank(&bank.0).is_none() {
            return;
        }
        self.sources.desired.clear();
        let rows = ambient_alias
            .into_iter()
            .map(|alias| (0, MAP_BED_SLOT, alias.to_owned(), None, 0.55, None))
            .chain(
                bank.0
                    .createfx_loop_sounds(namespace.namespace, &identity.zone)
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, emitter)| {
                        (
                            ordinal as u64,
                            MAP_EMITTER_SLOT,
                            emitter.soundalias,
                            Some(emitter.origin_inches),
                            1.0,
                            Some(SourceRenderGroup::MapEmitter),
                        )
                    }),
            );
        for (object, slot, alias, origin, gain, group) in
            rows.take(crate::runtime::LOGICAL_INSTANCES)
        {
            let Some(cue) = self.runtime.source_cue(SourceCueRequest {
                bank: bank.0.clone(),
                namespace: namespace.namespace,
                alias,
                emitter: None,
                scope: AudioScope::Match,
                group,
            }) else {
                continue;
            };
            self.sources.desired.push(DesiredSource {
                key: SourceKey {
                    scope: AudioScope::Match,
                    object,
                    slot,
                },
                version: 1,
                cue,
                origin_inches: origin,
                start_frame: self.runtime.audio_frame(),
                gain,
                rate: 1.0,
                audible: true,
            });
        }
        self.commands
            .insert_resource(asset_audio::CreateFxOneshotEmitters(
                bank.0
                    .createfx_oneshots(namespace.namespace, &identity.zone),
            ));
        self.booted.0 = true;
        perf::instant!("ambient_boot", "iw4l.sim", |ctx| {
            let (zone, alias): (&str, Option<&str>) = (&identity.zone, ambient_alias);
            ctx.arg("zone", zone);
            ctx.arg("alias", alias);
        });
        diag::info!(
            Audio,
            "audio: map ambient boot complete for {}",
            identity.zone
        );
    }
}

pub(crate) fn boot_map_ambient_once(
    boot: MapAmbientBoot,
    presented: Option<Res<net::PresentedSnapshot>>,
    bank: Option<Res<SoundBank>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    script_sound: Option<Res<asset_audio::SessionMapScriptSound>>,
    namespace: Option<Res<SoundBankNamespace>>,
) {
    boot.boot(presented, bank, identity, script_sound, namespace);
}

#[derive(Default)]
pub enum LoadingAudioStatus {
    #[default]
    Pending,
    Ready,
    Failed(String),
}

#[derive(Resource)]
pub struct PreparedMatchAudio {
    pub match_key: frame::MatchKey,
    iwd: SoundIwd,
    clips: crate::ClipStore,
    bank: SoundBank,
    namespace: SoundBankNamespace,
}

pub fn prepared_match_audio_key(world: &World) -> Option<frame::MatchKey> {
    world
        .get_resource::<PreparedMatchAudio>()
        .map(|prepared| prepared.match_key)
}

pub fn stage_prepared_match_audio(world: &mut World) {
    if let Some(prepared) = world.remove_resource::<PreparedMatchAudio>() {
        frame::scope::stage(world, prepared.iwd);
        frame::scope::stage(world, prepared.clips);
        frame::scope::stage(world, prepared.bank);
        frame::scope::stage(world, prepared.namespace);
    }
}
