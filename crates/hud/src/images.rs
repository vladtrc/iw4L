use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use assets::{AssetNamespace, HUD_CHROME_MENUS, MenuCatalog, NamespaceTrees, SessionCompass};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use gamemode_iw4::DAMAGE_FEEDBACK_SHADER;
use hud_iw4::COMPASS_ENEMY_FIRING_PING_IMAGE;

use crate::gaps::ImageMiss;

pub const HUD_CHROME_NAMESPACE: AssetNamespace = AssetNamespace::Iw4;

fn cache_key(name: &str) -> String {
    assets::AssetRef::bare_name(name).to_ascii_lowercase()
}

type IwdKey = (AssetNamespace, String);

type CachedRgba = Option<(u32, u32, Vec<u8>)>;

fn iwd_key(ns: AssetNamespace, name: &str) -> IwdKey {
    (ns, cache_key(name))
}

#[derive(Resource, Default)]
pub struct HudImages {
    games_root: PathBuf,

    trees: NamespaceTrees,

    map_namespace: AssetNamespace,
    by_name: HashMap<IwdKey, Option<Handle<Image>>>,
    rgba_by_name: HashMap<IwdKey, CachedRgba>,
    zone_rgba: HashMap<String, (u32, u32, Arc<Vec<u8>>)>,
    zone_handles: HashMap<String, Handle<Image>>,
    zone_image_name: HashMap<String, String>,
    zone_states: HashMap<String, Option<[u32; 2]>>,
    zone_installed: bool,
    zone_uploaded: bool,
    iwd_warmed: bool,
}

impl HudImages {
    pub fn set_games_root(&mut self, root: &Path) {
        if self.games_root == root {
            return;
        }
        self.games_root = root.to_path_buf();
        self.trees = NamespaceTrees::discover(&assets::GamesRoot(self.games_root.clone()));
        self.by_name.clear();
        self.rgba_by_name.clear();
        self.zone_uploaded = false;
        self.iwd_warmed = false;
        self.log_trees();
    }

    pub fn adopt_map_zone(&mut self, zone_ff: &Path) {
        let namespace = assets::zone_game_for_path(zone_ff)
            .map_or(AssetNamespace::Iw4, AssetNamespace::from_zone_game);
        let mut trees = self.trees.clone();
        trees.adopt_zone(zone_ff);
        if trees == self.trees && namespace == self.map_namespace {
            return;
        }
        self.trees = trees;
        self.map_namespace = namespace;
        self.by_name.clear();
        self.rgba_by_name.clear();
        self.iwd_warmed = false;
        self.log_trees();
    }

    pub fn map_namespace(&self) -> AssetNamespace {
        self.map_namespace
    }

    fn log_trees(&self) {
        for line in self.trees.report_lines() {
            diag::info!(Ui, "hud: {line}");
        }
    }

    pub fn has_games_root(&self) -> bool {
        !self.games_root.as_os_str().is_empty()
    }

    pub fn install_zone_catalog(&mut self, catalog: &MenuCatalog) {
        if self.zone_installed {
            return;
        }
        self.zone_installed = true;
        self.zone_uploaded = false;
        for (name, state) in &catalog.material_state_bits {
            self.zone_states.insert(name.clone(), state.agreed());
            if catalog.zone_images.contains_key(name) && state.agreed().is_none() {
                diag::warn!(Ui, "hud material state gap: {name}: {state:?}");
            }
        }
        for (key, atlas) in &catalog.zone_images {
            self.zone_rgba
                .insert(key.clone(), (atlas.width, atlas.height, atlas.rgba.clone()));
            self.zone_image_name
                .insert(key.clone(), atlas.image.clone());
            self.zone_handles.remove(key);
            self.by_name.retain(|(_, name), _| name != key);
            self.rgba_by_name.retain(|(_, name), _| name != key);
        }
    }

    pub fn material_state_bits(&self, ns: AssetNamespace, name: &str) -> Option<[u32; 2]> {
        (ns == HUD_CHROME_NAMESPACE)
            .then(|| self.zone_states.get(&cache_key(name)).copied().flatten())
            .flatten()
    }

    pub fn zone_installed(&self) -> bool {
        self.zone_installed
    }

    pub fn zone_image_name(&self, material: &str) -> Option<&str> {
        let key = cache_key(material);
        self.zone_image_name.get(&key).map(String::as_str)
    }

    pub fn miss_reason(&self) -> ImageMiss {
        if self.has_games_root() || self.zone_installed || !self.trees.is_empty() {
            ImageMiss::NotDecoded
        } else {
            ImageMiss::NoGamesRoot
        }
    }

    pub fn get(
        &mut self,
        ns: AssetNamespace,
        name: &str,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        let key = iwd_key(ns, name);
        if ns == HUD_CHROME_NAMESPACE && self.zone_states.get(&key.1) == Some(&None) {
            return None;
        }
        if let Some(cached) = self.by_name.get(&key) {
            return cached.clone();
        }
        if ns == HUD_CHROME_NAMESPACE {
            if let Some(handle) = self.upload_zone(name, images) {
                return Some(handle);
            }
        }
        let decoded = self.decode_iwd(ns, name, images);
        self.by_name.insert(key, decoded.clone());
        decoded
    }

    pub fn ensure_rgba(&mut self, ns: AssetNamespace, name: &str) {
        let key = iwd_key(ns, name);
        if self.rgba_by_name.contains_key(&key) {
            return;
        }
        if ns == HUD_CHROME_NAMESPACE {
            if let Some((width, height, rgba)) = self.zone_lookup(name) {
                self.rgba_by_name
                    .insert(key, Some((width, height, rgba.to_vec())));
                return;
            }
        }
        let decoded = self.decode_iwd_rgba(ns, name);
        self.rgba_by_name.insert(key, decoded);
    }

    pub fn rgba(&self, ns: AssetNamespace, name: &str) -> Option<&(u32, u32, Vec<u8>)> {
        self.rgba_by_name
            .get(&iwd_key(ns, name))
            .and_then(Option::as_ref)
    }

    pub fn warm_present_stems(
        &mut self,
        images: &mut Assets<Image>,
        catalog: Option<&MenuCatalog>,
        compass: Option<&SessionCompass>,
    ) {
        self.upload_pending_zone(images);
        if self.iwd_warmed {
            return;
        }
        if self.trees.is_empty() {
            return;
        }

        for name in [
            "blood_defocus_color",
            "blood_defocus_mask",
            DAMAGE_FEEDBACK_SHADER,
            COMPASS_ENEMY_FIRING_PING_IMAGE,
        ] {
            let _ = self.get(HUD_CHROME_NAMESPACE, name, images);
        }
        if let Some(compass) = compass {
            if let Some(name) = compass.declaration.image.as_deref() {
                let ns = self.map_namespace;
                let _ = self.get(ns, name, images);
                self.ensure_rgba(ns, name);
            }
        }
        if let Some(catalog) = catalog {
            for font in catalog.fonts.values() {
                if !font.material.is_empty() {
                    let _ = self.get(HUD_CHROME_NAMESPACE, &font.material, images);
                }
            }
            for menu_name in HUD_CHROME_MENUS {
                let Some(menu) = catalog.get(menu_name) else {
                    continue;
                };
                if !menu.window_background.is_empty() {
                    let _ = self.get(HUD_CHROME_NAMESPACE, &menu.window_background, images);
                }
                for item in &menu.items {
                    if !item.background.is_empty() {
                        let _ = self.get(HUD_CHROME_NAMESPACE, &item.background, images);
                    }
                }
            }
        }
        self.iwd_warmed = true;
    }

    fn upload_pending_zone(&mut self, images: &mut Assets<Image>) {
        if self.zone_uploaded || !self.zone_installed {
            return;
        }
        let keys: Vec<String> = self.zone_rgba.keys().cloned().collect();
        for key in keys {
            let _ = self.upload_zone(&key, images);
        }
        self.zone_uploaded = true;
    }

    fn decode_iwd(
        &self,
        ns: AssetNamespace,
        name: &str,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        match self.decode_iwd_rgba(ns, name) {
            Some((width, height, rgba)) => Some(images.add(Image::new(
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            ))),
            None => None,
        }
    }

    fn decode_iwd_rgba(&self, ns: AssetNamespace, name: &str) -> CachedRgba {
        let main = self.trees.main_for(ns)?;
        match assets::decode_ui_image_from_main(main, name) {
            Ok(image) => image,
            Err(error) => {
                diag::warn!(
                    Ui,
                    "hud: decode `{}:{name}` from {}: {error}",
                    ns.as_str(),
                    main.display()
                );
                None
            }
        }
    }

    fn zone_lookup(&self, name: &str) -> Option<(u32, u32, Arc<Vec<u8>>)> {
        self.zone_rgba.get(&cache_key(name)).cloned()
    }

    fn upload_zone(&mut self, name: &str, images: &mut Assets<Image>) -> Option<Handle<Image>> {
        let key = cache_key(name);
        if let Some(handle) = self.zone_handles.get(&key) {
            return Some(handle.clone());
        }
        let (width, height, rgba) = self.zone_lookup(name)?;
        let handle = images.add(Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba.as_ref().clone(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        self.zone_handles.insert(key, handle.clone());
        Some(handle)
    }
}
