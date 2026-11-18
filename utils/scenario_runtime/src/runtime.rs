use crate::{CampaignSimulationOptions, HeadlessSession, SceneRecipe, SimulationOptions};
use asset_transport::{GamesProvider, GamesRoot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    pub games: PathBuf,
    pub artifacts: PathBuf,
    #[serde(default)]
    pub additional_games: Vec<PathBuf>,
}

struct Provider(RuntimeConfig);
impl GamesProvider for Provider {
    fn game_root(&self) -> Result<GamesRoot, String> {
        Ok(GamesRoot(self.0.games.clone()))
    }
    fn search_roots(&self, root: &Path) -> Vec<PathBuf> {
        let mut roots = vec![root.to_owned()];
        for path in &self.0.additional_games {
            if !roots.contains(path) {
                roots.push(path.clone());
            }
        }
        roots
    }
}

#[derive(Clone)]
pub struct Runtime {
    config: RuntimeConfig,
    catalog: std::sync::Arc<asset_game::MenuCatalog>,
}

impl Runtime {
    pub fn new(
        mut config: RuntimeConfig,
        families: &[&'static dyn game_family::GameFamily],
    ) -> Result<Self, String> {
        config.games = config
            .games
            .canonicalize()
            .map_err(|error| format!("games: {error}"))?;
        for path in &mut config.additional_games {
            *path = path
                .canonicalize()
                .map_err(|error| format!("additional games: {error}"))?;
        }
        std::fs::create_dir_all(&config.artifacts)
            .map_err(|error| format!("artifacts: {error}"))?;
        config.artifacts = config
            .artifacts
            .canonicalize()
            .map_err(|error| format!("artifacts: {error}"))?;
        static INITIALIZE: Mutex<()> = Mutex::new(());
        static CONFIG: OnceLock<(RuntimeConfig, Vec<String>)> = OnceLock::new();
        let _lock = INITIALIZE
            .lock()
            .map_err(|_| "runtime initialization poisoned")?;
        if families.is_empty() {
            return Err("at least one game family is required".into());
        }
        let ids = families
            .iter()
            .map(|family| family.id().as_str().to_owned())
            .collect::<Vec<_>>();
        if let Some((installed, installed_ids)) = CONFIG.get() {
            if installed != &config || installed_ids != &ids {
                return Err(
                    "asset registries already belong to another runtime configuration".into(),
                );
            }
        } else {
            game_family::install(families).map_err(str::to_owned)?;
            asset_transport::install_games_provider(Provider(config.clone()))
                .map_err(str::to_owned)?;
            asset_transport::install_artifacts_root(config.artifacts.clone())
                .map_err(str::to_owned)?;
            CONFIG
                .set((config.clone(), ids))
                .map_err(|_| "runtime initialization raced")?;
        }
        static CATALOG: OnceLock<std::sync::Arc<asset_game::MenuCatalog>> = OnceLock::new();
        let catalog = CATALOG
            .get_or_init(|| {
                let (catalog, _) =
                    asset_game::load_ui_menu_catalog(&GamesRoot(config.games.clone()));
                std::sync::Arc::new(catalog)
            })
            .clone();
        Ok(Self { config, catalog })
    }

    pub fn prepare(
        &self,
        name: &str,
        recipe: SceneRecipe,
        options: SimulationOptions,
    ) -> Result<HeadlessSession, String> {
        self.prepare_with_progress(
            name,
            recipe,
            options,
            asset_transport::LoadProgress::default(),
        )
    }

    pub fn prepare_with_progress(
        &self,
        name: &str,
        recipe: SceneRecipe,
        options: SimulationOptions,
        progress: asset_transport::LoadProgress,
    ) -> Result<HeadlessSession, String> {
        if progress.is_canceled() {
            return Err("scene preparation canceled".into());
        }
        let scene = assets::MergedMap::from_scene(name.to_owned(), recipe)?;
        let prepared = self.load_map(scene.zone(), Some(&scene), &progress, None)?;
        let configuration = serde_json::json!({
            "mode":options.mode.kind().token(),"seed":options.seed,"score_limit":options.score_limit,
            "time_limit_ms":options.time_limit_ms,"cheats":options.cheats,"dvars":options.dvars,
            "classes":options.classes.iter().map(|row|serde_json::json!({"weapons":row.weapons,"attachments":row.attachments,"perks":row.perks,"deathstreak":row.deathstreak,"camos":row.camos})).collect::<Vec<_>>(),
        });
        let prepared = session::prepare_simulation(
            prepared,
            Some(&scene),
            options,
            &self.config.games,
            &self.catalog,
        )?;
        Ok(HeadlessSession::new(
            prepared,
            scene.recording_recipe().clone(),
            configuration,
        ))
    }
    /// Prepare a native SP map and its local campaign player without an atlas,
    /// multiplayer admission or any presentation devices.
    pub fn prepare_campaign(
        &self,
        map: &str,
        options: CampaignSimulationOptions,
    ) -> Result<HeadlessSession, String> {
        self.prepare_campaign_with_progress(map, options, asset_transport::LoadProgress::default())
    }

    pub fn prepare_campaign_with_progress(
        &self,
        map: &str,
        options: CampaignSimulationOptions,
        progress: asset_transport::LoadProgress,
    ) -> Result<HeadlessSession, String> {
        let prepared = self.load_map(
            map,
            None,
            &progress,
            Some(asset_core::RuntimeProfile::Campaign),
        )?;
        let configuration = serde_json::json!({
            "profile":"campaign", "client":0, "seed":options.seed,
            "cheats":options.cheats, "dvars":options.dvars,
        });
        let prepared = session::prepare_campaign_simulation(
            prepared,
            options,
            &self.config.games,
            &self.catalog,
        )?;
        Ok(HeadlessSession::new(
            prepared,
            serde_json::json!({"map":map}),
            configuration,
        ))
    }

    fn load_map(
        &self,
        map: &str,
        scene: Option<&assets::MergedMap>,
        progress: &asset_transport::LoadProgress,
        profile: Option<asset_core::RuntimeProfile>,
    ) -> Result<assets::PreparedMatch, String> {
        if progress.is_canceled() {
            return Err("scene preparation canceled".into());
        }
        let games = GamesRoot(self.config.games.clone());
        let found = asset_transport::find_zone_file(&games, map)?;
        let sound_path = found.path.clone();
        let common =
            asset_transport::find_runtime_common_mp(&games, &found.path).map(|zone| zone.path);
        let trees = asset_transport::NamespaceTrees::discover(&games);
        let inventory = net::content_inventory(trees.present().map(|(family, _)| family.as_str()));
        let families =
            frame::AssetFamilies::from_inventory(&inventory, &frame::UiMenuDvars::default())?;
        let atlas = assets::session_load::atlas_load(Some(&games), scene);
        let mut prepared = match bevy::tasks::futures_lite::future::block_on(
            assets::session_load::load_prepared_match_with_atlas(
                Ok(found.path),
                common,
                progress.clone(),
                families.clone(),
                atlas,
                profile,
            ),
        ) {
            assets::MatchLoadOutcome::Ready(prepared) => *prepared,
            assets::MatchLoadOutcome::Canceled => return Err("scene preparation canceled".into()),
            assets::MatchLoadOutcome::Refused(reason) => return Err(reason),
        };
        if progress.is_canceled() {
            return Err("scene preparation canceled".into());
        }
        // Script sound validation needs authored aliases/channels even when no
        // playback device exists. Compose metadata through the normal asset path.
        let namespace = prepared
            .prepared_map
            .namespace
            .ok_or("prepared map has no family")?;
        let sources = asset_audio::gather_sound_sources_for_profile(
            &games,
            &sound_path,
            prepared.profile,
            |ns| families.enabled(ns.as_str()),
        );
        let map_sound = prepared
            .sound
            .take()
            .unwrap_or_else(|| Err("the map zone never opened".into()));
        let bank = asset_audio::compose_sound_bank(sources, map, namespace, map_sound);
        prepared.sound_gaps = bank.gaps.len();
        prepared.script_sound_aliases = Some(bank.catalog.script_alias_looping());
        prepared.script_sound_channels = bank.catalog.channels_in(namespace).map(|channels| {
            channels
                .iter()
                .map(|channel| channel.name.clone())
                .collect()
        });
        prepared.sound = Some(Ok(bank.catalog));
        if progress.is_canceled() {
            return Err("scene preparation canceled".into());
        }
        Ok(prepared)
    }
}
