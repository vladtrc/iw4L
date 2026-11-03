#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldDrawPolicy {
    pub family: asset_core::FamilyId,
    pub resolve_specular_env: bool,
    pub lightmap_requires_image: bool,
    pub decode_color_at_convert: bool,
    pub sun_sample_size_near: f32,
}

impl WorldDrawPolicy {
    pub const fn iw4() -> Self {
        Self {
            family: asset_core::FamilyId::IW4,
            resolve_specular_env: true,
            lightmap_requires_image: false,
            decode_color_at_convert: false,
            sun_sample_size_near: 0.25,
        }
    }
}

/// Completes authored objective scene models from a family's common model catalog.
pub type ObjectiveModelCompletion =
    fn(&mut crate::MapXModelSceneCatalog, &[&str], &asset_model::WorldWeaponCatalog);

#[derive(Clone, Copy)]
pub struct WorldFamily {
    pub draw: WorldDrawPolicy,
    pub complete_objective_models: Option<ObjectiveModelCompletion>,
    pub normalize_entities: fn(&str) -> String,
    pub spawn_points: fn(&str) -> Vec<crate::SpawnPoint>,
}

pub const REFERENCE_WORLD_FAMILY: WorldFamily = WorldFamily {
    draw: WorldDrawPolicy::iw4(),
    complete_objective_models: None,
    normalize_entities: |text| text.to_owned(),
    spawn_points: crate::parse_reference_spawn_points,
};

#[derive(Default)]
pub struct WorldFamilies(std::collections::BTreeMap<asset_core::FamilyId, WorldFamily>);

impl WorldFamilies {
    pub fn register(&mut self, family: asset_core::FamilyId, policy: WorldFamily) {
        assert_eq!(
            family, policy.draw.family,
            "world policy belongs to another family"
        );
        assert!(
            self.0.insert(family, policy).is_none(),
            "duplicate world family"
        );
    }
}

static FAMILIES: std::sync::OnceLock<WorldFamilies> = std::sync::OnceLock::new();

pub fn install_world_families(families: WorldFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "world families already installed")
}

pub fn world_family(family: asset_core::FamilyId) -> Option<&'static WorldFamily> {
    FAMILIES.get()?.0.get(&family)
}
