use crate::{HeadlessSession, SceneRecipe, SimulationOptions};
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
        let games = GamesRoot(self.config.games.clone());
        let found = asset_transport::find_zone_file(&games, scene.zone())?;
        let common =
            asset_transport::find_runtime_common_mp(&games, &found.path).map(|zone| zone.path);
        let trees = asset_transport::NamespaceTrees::discover(&games);
        let inventory = net::content_inventory(trees.present().map(|(family, _)| family.as_str()));
        let families =
            frame::AssetFamilies::from_inventory(&inventory, &frame::UiMenuDvars::default())?;
        let atlas = assets::session_load::atlas_load(Some(&games), Some(&scene));
        let prepared = match bevy::tasks::futures_lite::future::block_on(
            assets::session_load::load_prepared_match_with_atlas(
                Ok(found.path),
                common,
                progress.clone(),
                families,
                atlas,
            ),
        ) {
            assets::MatchLoadOutcome::Ready(prepared) => *prepared,
            assets::MatchLoadOutcome::Canceled => return Err("scene preparation canceled".into()),
            assets::MatchLoadOutcome::Refused(reason) => return Err(reason),
        };
        if progress.is_canceled() {
            return Err("scene preparation canceled".into());
        }
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
}
