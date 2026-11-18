use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use asset_core::FamilyId;
use bevy::prelude::Resource;

use crate::material_images::{ZoneUiImage, ZoneUiRgba};

static NEXT_PUBLICATION: AtomicU64 = AtomicU64::new(1);
type UiKey = (FamilyId, String);

#[derive(Default)]
pub struct UiImageBuild {
    images: HashMap<UiKey, String>,
    fallbacks: HashMap<UiKey, String>,
    zone: HashMap<UiKey, ZoneUiImage>,
    archives: HashMap<FamilyId, Arc<asset_transport::IwdIndex>>,
    archive_paths: HashMap<std::path::PathBuf, Arc<asset_transport::IwdIndex>>,
}

struct UiImageData {
    id: u64,
    build: UiImageBuild,
}

#[derive(Resource, Clone)]
pub struct UiImagePublication(Arc<UiImageData>);

impl Default for UiImagePublication {
    fn default() -> Self {
        UiImageBuild::default().publish()
    }
}

impl std::fmt::Debug for UiImagePublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("UiImagePublication")
            .field(&self.id())
            .finish()
    }
}

fn key(namespace: FamilyId, material: &str) -> UiKey {
    (
        namespace,
        asset_core::AssetRef::bare_name(material).to_ascii_lowercase(),
    )
}

impl UiImageBuild {
    pub fn retain_materials(&mut self, materials: &crate::MaterialCatalog) {
        for material in &materials.materials {
            if let Some(image) = materials.hud_image_name(material) {
                self.images.insert(
                    key(material.namespace, material.name.as_str()),
                    image.to_owned(),
                );
            }
        }
    }

    pub fn preview_fallback(&mut self, namespace: FamilyId, material: &str, image: &str) {
        self.fallbacks
            .insert(key(namespace, material), image.to_owned());
    }

    pub fn zone_images(
        &mut self,
        namespace: FamilyId,
        images: impl IntoIterator<Item = (String, ZoneUiImage)>,
    ) {
        self.zone.extend(
            images
                .into_iter()
                .map(|(name, image)| (key(namespace, &name), image)),
        );
    }

    pub fn retain_archives(&mut self, games: &asset_transport::GamesRoot) -> Result<(), String> {
        let trees = asset_transport::NamespaceTrees::discover(games);
        let mut first_error = None;
        for namespace in asset_core::families().iter().map(|family| family.id()) {
            if let Some(main) = trees.main_for(namespace) {
                match asset_transport::IwdIndex::open(main) {
                    Ok(index) => self.archive_source(namespace, main.to_path_buf(), index),
                    Err(error) if first_error.is_none() => first_error = Some(error),
                    Err(_) => {}
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub fn archive_source(
        &mut self,
        namespace: FamilyId,
        main: std::path::PathBuf,
        index: Arc<asset_transport::IwdIndex>,
    ) {
        self.archive_paths.insert(main, index.clone());
        self.archives.insert(namespace, index);
    }

    pub fn publish(self) -> UiImagePublication {
        UiImagePublication(Arc::new(UiImageData {
            id: NEXT_PUBLICATION.fetch_add(1, Ordering::Relaxed),
            build: self,
        }))
    }
}

impl UiImagePublication {
    pub fn id(&self) -> u64 {
        self.0.id
    }

    pub(crate) fn archive(&self, namespace: FamilyId) -> Option<&Arc<asset_transport::IwdIndex>> {
        self.0.build.archives.get(&namespace)
    }
    pub(crate) fn archive_at(
        &self,
        main: &std::path::Path,
    ) -> Option<&Arc<asset_transport::IwdIndex>> {
        self.0.build.archive_paths.get(main)
    }

    pub fn material_image(&self, namespace: FamilyId, material: &str) -> Option<&str> {
        self.0
            .build
            .images
            .get(&key(namespace, material))
            .map(String::as_str)
    }

    pub fn preview_fallback(&self, namespace: FamilyId, material: &str) -> Option<&str> {
        self.0
            .build
            .fallbacks
            .get(&key(namespace, material))
            .map(String::as_str)
    }

    pub fn has_zone_image(&self, namespace: FamilyId, material: &str) -> bool {
        self.0.build.zone.contains_key(&key(namespace, material))
    }

    pub fn zone_material_state(
        &self,
        namespace: FamilyId,
        material: &str,
    ) -> Option<render_material::CompiledPassState> {
        self.0
            .build
            .zone
            .get(&key(namespace, material))
            .and_then(|image| image.state)
    }

    pub fn zone_image(&self, namespace: FamilyId, material: &str) -> Option<ZoneUiRgba> {
        let image = self.0.build.zone.get(&key(namespace, material))?;
        match crate::decode_iwi_rgba(&image.iwi) {
            Ok((width, height, rgba)) => Some((width, height, Arc::new(rgba))),
            Err(error) => {
                diag::warn!(Zone, "zone UI image {material}: {error}");
                None
            }
        }
    }
}
