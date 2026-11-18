use super::*;
mod binding;
use asset_core::{AssetKey, AssetKind, FamilyId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum WeaponComponent {
    ViewModel,
    WorldModel,
    Hands,
    Animation,
    Material,
    Effect,
    Projectile,
    Sound,
    HudIconImage,
    OverlayImage,
    Tracer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentPreparationPolicy {
    Native,
    Unsupported(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedComponentTarget {
    Pending,
    Fpv(AssetEdge<FpvMeshSpace>),
    World(AssetEdge<WorldWeaponSpace>),
    Animation(AssetEdge<XAnimSpace>),
    Material(AssetEdge<MaterialSpace>),
    Effect(AssetEdge<FxSpace>),
    Projectile(AssetEdge<ProjectileModelSpace>),
    Tracer(AssetEdge<TracerSpace>),
    SoundPublication,
    UiImages,
    Unsupported(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedComponentReference {
    component: WeaponComponent,
    source_namespace: FamilyId,
    source_name: String,
    name: String,
    storage_namespace: FamilyId,
    policy: ComponentPreparationPolicy,
    target: PreparedComponentTarget,
}

impl PreparedComponentReference {
    pub fn target(&self) -> PreparedComponentTarget {
        self.target
    }
    pub fn component(&self) -> WeaponComponent {
        self.component
    }
    pub fn source_name(&self) -> &str {
        &self.source_name
    }
    pub fn source_namespace(&self) -> FamilyId {
        self.source_namespace
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn storage_namespace(&self) -> FamilyId {
        self.storage_namespace
    }
    pub fn policy(&self) -> &ComponentPreparationPolicy {
        &self.policy
    }
}

#[derive(Clone, Debug)]
pub struct WeaponPreparationRecipe {
    host_namespace: FamilyId,
    attachment_mount_on_root: bool,
    hide_mode: crate::FpvHideMode,
    ads_overlay: crate::AdsOverlayConvention,
    source: AssetKey,
    routes: [Option<FamilyId>; 11],
    references: Vec<PreparedComponentReference>,
    pub(in crate::weapon_catalog) refusal: Option<String>,
    closed_references: bool,
}

impl WeaponPreparationRecipe {
    pub fn host_namespace(&self) -> FamilyId {
        self.host_namespace
    }
    pub fn attachment_mount_on_root(&self) -> bool {
        self.attachment_mount_on_root
    }
    pub fn hide_mode(&self) -> crate::FpvHideMode {
        self.hide_mode
    }
    pub fn ads_overlay_convention(&self) -> crate::AdsOverlayConvention {
        self.ads_overlay
    }
    pub fn source(&self) -> &AssetKey {
        &self.source
    }
    pub(super) fn set_source(&mut self, namespace: FamilyId, name: &str) {
        self.source.namespace = namespace;
        self.source.name = name.to_owned();
    }
    pub fn references(&self) -> &[PreparedComponentReference] {
        &self.references
    }
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }
    pub fn namespace(&self, component: WeaponComponent) -> Option<FamilyId> {
        self.routes[component as usize]
    }
    pub(super) fn native(namespace: FamilyId) -> Self {
        Self {
            host_namespace: namespace,
            attachment_mount_on_root: false,
            hide_mode: crate::FpvHideMode::Surfaces,
            ads_overlay: crate::AdsOverlayConvention::WeightIsFrac,
            source: AssetKey {
                namespace,
                kind: AssetKind::Weapon,
                name: String::new(),
            },
            routes: [Some(namespace); 11],
            references: Vec::new(),
            refusal: None,
            closed_references: false,
        }
    }
    pub(super) fn for_capture(namespace: FamilyId, weapon: &str) -> Self {
        let mut recipe = Self::native(namespace);
        recipe.set_source(namespace, weapon);
        let runtime = &crate::weapon_policy(namespace)
            .expect("weapon runtime policy required before capture")
            .runtime;
        recipe.attachment_mount_on_root = runtime.attachment_mount_on_root;
        recipe.hide_mode = runtime.hide_mode;
        recipe.ads_overlay = runtime.ads_overlay;
        recipe
    }
}
