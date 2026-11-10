use std::sync::Arc;

use asset_anim::{
    CompiledAnimTreeDefinition, ParsedPlayerAnimScript, PlayerAnimSources, XAnimCatalog,
};
use asset_core::{FamilyId, FpvMeshIndex};
use asset_model::{BodyMeshCatalog, BodyMeshEntry, FpvHands, FpvMeshCatalog, SoldierKit};
use bevy::prelude::Resource;
use xmodel_runtime::{AnimClip, DObj};

#[derive(Debug)]
pub struct SoldierBodyAnimation {
    bodies: Arc<BodyMeshCatalog>,
    body_name: String,
    tree: Arc<CompiledAnimTreeDefinition>,
    script: Arc<ParsedPlayerAnimScript>,
    catalog: Arc<XAnimCatalog>,
    rig: DObj,
    runtime: Result<Arc<xmodel_runtime::XAnimTreeDefinition>, String>,
}

impl SoldierBodyAnimation {
    fn prepare(
        bodies: &Arc<BodyMeshCatalog>,
        kit: &SoldierKit,
        sources: &PlayerAnimSources,
        catalog: &Arc<XAnimCatalog>,
    ) -> Result<Arc<Self>, String> {
        if !sources.decode_errors().is_empty() {
            return Err("character animation source decode failed".into());
        }
        let body = bodies.get(&kit.body).ok_or("character body missing")?;
        if sources.family() != Some(body.namespace) {
            return Err(format!(
                "{} character requires its own animation tree and script; supplied family {:?}",
                body.namespace.as_str(),
                sources.family()
            ));
        }
        let tree = sources
            .compiled()
            .and_then(|tree| tree.as_ref().ok())
            .ok_or("character tree is not compiled")?;
        let script = sources
            .parsed_script()
            .and_then(|script| script.as_ref().ok())
            .ok_or("character script is not parsed")?;
        let pose = body
            .skel
            .pose
            .as_ref()
            .ok_or("character body pose missing")?;
        let rig = DObj::build(&[(pose, None)]).map_err(|error| error.to_string())?;
        for name in ["legs", "torso"] {
            let index = tree
                .index_of(name)
                .ok_or_else(|| format!("player tree {name} branch missing"))?;
            if tree.node(index).is_none_or(|node| node.child_count == 0) {
                return Err(format!("player tree {name} is not a branch"));
            }
        }
        let runtime = tree
            .to_runtime_definition(|_, name| catalog.clip(body.namespace, name))
            .map_err(|error| error.to_string());
        Ok(Arc::new(Self {
            runtime,
            bodies: Arc::clone(bodies),
            body_name: kit.body.clone(),
            tree: Arc::clone(tree),
            script: Arc::clone(script),
            catalog: Arc::clone(catalog),
            rig,
        }))
    }

    pub fn body(&self) -> &BodyMeshEntry {
        self.bodies
            .get(&self.body_name)
            .expect("bound character body")
    }

    pub fn tree(&self) -> &Arc<CompiledAnimTreeDefinition> {
        &self.tree
    }

    pub fn script(&self) -> &Arc<ParsedPlayerAnimScript> {
        &self.script
    }

    pub fn runtime(&self) -> Result<&Arc<xmodel_runtime::XAnimTreeDefinition>, &str> {
        self.runtime.as_ref().map_err(String::as_str)
    }

    pub fn decode_leaf(&self, index: u16) -> Result<Arc<AnimClip>, String> {
        let leaf = self
            .tree
            .node(index)
            .filter(|node| node.child_count == 0)
            .ok_or_else(|| format!("packed index {index} is not a character animation leaf"))?;
        let clip = self
            .catalog
            .clip(self.body().namespace, &leaf.name)
            .ok_or_else(|| format!("decode failed for character clip `{}`", leaf.name))?;
        if !self.rig.tracks_for(&clip).iter().any(Option::is_some) {
            return Err(format!(
                "character clip `{}` has no tracks for body `{}`",
                leaf.name, self.body_name
            ));
        }
        Ok(clip)
    }
}

#[derive(Clone, Copy, Debug)]
struct HandMounts {
    posed: bool,
    weapon: bool,
    secondary: bool,
}

#[derive(Clone, Debug)]
pub struct SoldierHands {
    choice: FpvHands,
    model: FpvMeshIndex,
    mounts: HandMounts,
}

impl SoldierHands {
    pub fn choice(&self) -> &FpvHands {
        &self.choice
    }

    pub fn model(&self) -> FpvMeshIndex {
        self.model
    }
}

#[derive(Debug)]
pub struct SoldierHead {
    bodies: Arc<BodyMeshCatalog>,
    name: String,
    tag: String,
}

impl SoldierHead {
    fn prepare(
        bodies: &Arc<BodyMeshCatalog>,
        kit: &SoldierKit,
        family: FamilyId,
    ) -> Result<Option<Self>, String> {
        let Some(name) = &kit.head else {
            return Ok(None);
        };
        let body = bodies.get(&kit.body).ok_or("soldier body missing")?;
        let entry = bodies
            .get(name)
            .ok_or_else(|| format!("soldier head `{name}` missing"))?;
        if entry.namespace != family {
            return Err("soldier head differs from body family".into());
        }
        let body_pose = body
            .skel
            .pose
            .as_ref()
            .ok_or("soldier body pose missing for head")?;
        let head_pose = entry
            .skel
            .pose
            .as_ref()
            .ok_or("soldier head pose missing")?;
        let tag = xmodel_runtime::tp_head_attach_tag(&body.skel.bone_names)
            .ok_or("soldier head mount missing")?;
        DObj::build(&[
            (body_pose, None),
            (
                head_pose,
                Some(xmodel_runtime::Attach {
                    parent_model: 0,
                    tag: tag.into(),
                }),
            ),
        ])
        .map_err(|error| error.to_string())?;
        Ok(Some(Self {
            bodies: Arc::clone(bodies),
            name: name.clone(),
            tag: tag.into(),
        }))
    }

    pub fn entry(&self) -> &BodyMeshEntry {
        self.bodies.get(&self.name).expect("prepared soldier head")
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }
}

#[derive(Debug)]
pub struct SoldierPresentation {
    kit: SoldierKit,
    family: FamilyId,
    bodies: Arc<BodyMeshCatalog>,
    meshes: Arc<FpvMeshCatalog>,
    mounts: Arc<[HandMounts]>,
    kit_hands: Result<Option<SoldierHands>, String>,
    head: Result<Option<SoldierHead>, String>,
    animation: Result<Arc<SoldierBodyAnimation>, String>,
}

impl SoldierPresentation {
    fn prepare(
        axis: bool,
        bodies: &Arc<BodyMeshCatalog>,
        meshes: &Arc<FpvMeshCatalog>,
        mounts: &Arc<[HandMounts]>,
        sources: &PlayerAnimSources,
        catalog: &Arc<XAnimCatalog>,
    ) -> Result<Arc<Self>, String> {
        if meshes.identity() == 0 {
            return Err("soldier meshes are not published".into());
        }
        let family = meshes.map_namespace.ok_or("soldier map family missing")?;
        let kit = bodies.kits().kit(axis).ok_or("no map soldier kit")?.clone();
        let body = bodies.get(&kit.body).ok_or("soldier body missing")?;
        if body.namespace != family {
            return Err("soldier body differs from map family".into());
        }
        let choice = (asset_model::model_family(family)
            .ok_or("soldier model family missing")?
            .soldier_hands)(meshes, family, &kit);
        let mut presentation = Self {
            head: SoldierHead::prepare(bodies, &kit, family),
            animation: SoldierBodyAnimation::prepare(bodies, &kit, sources, catalog),
            kit,
            family,
            bodies: Arc::clone(bodies),
            meshes: Arc::clone(meshes),
            mounts: Arc::clone(mounts),
            kit_hands: Ok(None),
        };
        presentation.kit_hands = choice.and_then(|choice| {
            choice
                .map(|choice| presentation.bind_hands(choice))
                .transpose()
        });
        Ok(Arc::new(presentation))
    }

    fn bind_hands(&self, choice: FpvHands) -> Result<SoldierHands, String> {
        let (family, name) = choice.key().ok_or("soldier hands unresolved")?;
        if family != self.family {
            return Err("hands differ from soldier family".into());
        }
        let index = self
            .meshes
            .index_by_name(family, name)
            .ok_or_else(|| format!("soldier hands `{name}` missing"))?;
        let mounts = *self
            .mounts
            .get(index)
            .ok_or("soldier hand mounts missing")?;
        if !mounts.posed || !mounts.weapon {
            return Err(format!(
                "soldier hands `{name}` require a pose and tag_weapon"
            ));
        }
        Ok(SoldierHands {
            choice,
            model: FpvMeshIndex::from_order(index),
            mounts,
        })
    }

    pub fn kit(&self) -> &SoldierKit {
        &self.kit
    }

    pub fn family(&self) -> FamilyId {
        self.family
    }

    pub fn body(&self) -> &BodyMeshEntry {
        self.bodies
            .get(&self.kit.body)
            .expect("prepared soldier body")
    }

    pub fn head(&self) -> Result<Option<&SoldierHead>, &str> {
        self.head
            .as_ref()
            .map(Option::as_ref)
            .map_err(String::as_str)
    }

    pub fn owns_bodies(&self, bodies: &Arc<BodyMeshCatalog>) -> bool {
        Arc::ptr_eq(&self.bodies, bodies)
    }

    pub fn animation(&self) -> Result<&Arc<SoldierBodyAnimation>, &str> {
        self.animation.as_ref().map_err(String::as_str)
    }

    pub fn connect_weapon(
        self: &Arc<Self>,
        hands: Option<(FamilyId, &str)>,
        secondary: bool,
    ) -> Result<FpvHandsPresentation, String> {
        let hands = match &self.kit_hands {
            Err(error) => return Err(error.clone()),
            Ok(Some(hands)) => hands.clone(),
            Ok(None) => {
                let (family, name) = hands.ok_or("soldier weapon hands missing")?;
                self.bind_hands(FpvHands::FromWeaponDef {
                    namespace: family,
                    name: name.to_owned(),
                })?
            }
        };
        if secondary && !hands.mounts.secondary {
            return Err("soldier hands require tag_weapon1 for a secondary gun".into());
        }
        Ok(FpvHandsPresentation {
            meshes: Arc::clone(&self.meshes),
            hands,
        })
    }
}

#[derive(Clone, Debug)]
pub struct FpvHandsPresentation {
    meshes: Arc<FpvMeshCatalog>,
    hands: SoldierHands,
}

impl FpvHandsPresentation {
    pub fn from_viewmodel(
        meshes: &Arc<FpvMeshCatalog>,
        family: FamilyId,
        name: &str,
    ) -> Result<Self, String> {
        if meshes.identity() == 0 {
            return Err("viewmodel meshes are not published".into());
        }
        let index = meshes
            .index_by_name(family, name)
            .ok_or_else(|| format!("viewmodel `{name}` missing"))?;
        let entry = meshes.get_at(index).ok_or("viewmodel mesh missing")?;
        let mounts = HandMounts {
            posed: entry.skel.pose.is_some(),
            weapon: entry
                .skel
                .bone_names
                .iter()
                .any(|bone| bone == "tag_weapon"),
            secondary: entry
                .skel
                .bone_names
                .iter()
                .any(|bone| bone == "tag_weapon1"),
        };
        if !mounts.posed || !mounts.weapon {
            return Err(format!("viewmodel `{name}` requires a pose and tag_weapon"));
        }
        Ok(Self {
            meshes: Arc::clone(meshes),
            hands: SoldierHands {
                choice: FpvHands::FromScript {
                    namespace: family,
                    name: name.to_owned(),
                },
                model: FpvMeshIndex::from_order(index),
                mounts,
            },
        })
    }

    pub fn supports_secondary(&self) -> bool {
        self.hands.mounts.secondary
    }

    pub fn hands(&self) -> &SoldierHands {
        &self.hands
    }

    pub fn mesh_identity(&self) -> u64 {
        self.meshes.identity()
    }
}

#[derive(Clone, Debug, Resource)]
pub struct SoldierPresentations {
    sides: [Result<Arc<SoldierPresentation>, String>; 2],
    bodies: Arc<BodyMeshCatalog>,
    meshes: Arc<FpvMeshCatalog>,
    catalog: Arc<XAnimCatalog>,
    animation_family: Option<FamilyId>,
    tree: Option<Arc<CompiledAnimTreeDefinition>>,
    script: Option<Arc<ParsedPlayerAnimScript>>,
}

impl SoldierPresentations {
    pub fn prepare(
        bodies: &Arc<BodyMeshCatalog>,
        meshes: &Arc<FpvMeshCatalog>,
        sources: &PlayerAnimSources,
        catalog: &Arc<XAnimCatalog>,
    ) -> Self {
        let mounts: Arc<[HandMounts]> = (0..meshes.len())
            .map(|index| meshes.get_at(index).expect("published mesh row"))
            .map(|entry| HandMounts {
                posed: entry.skel.pose.is_some(),
                weapon: entry
                    .skel
                    .bone_names
                    .iter()
                    .any(|bone| bone == "tag_weapon"),
                secondary: entry
                    .skel
                    .bone_names
                    .iter()
                    .any(|bone| bone == "tag_weapon1"),
            })
            .collect();
        let allies = SoldierPresentation::prepare(false, bodies, meshes, &mounts, sources, catalog);
        let axis = if bodies.kits().kit(false) == bodies.kits().kit(true) {
            allies.clone()
        } else {
            SoldierPresentation::prepare(true, bodies, meshes, &mounts, sources, catalog)
        };
        Self {
            sides: [allies, axis],
            bodies: Arc::clone(bodies),
            meshes: Arc::clone(meshes),
            catalog: Arc::clone(catalog),
            animation_family: sources.family(),
            tree: sources
                .compiled()
                .and_then(|tree| tree.as_ref().ok())
                .cloned(),
            script: sources
                .parsed_script()
                .and_then(|script| script.as_ref().ok())
                .cloned(),
        }
    }

    pub fn owned_by(
        &self,
        bodies: &Arc<BodyMeshCatalog>,
        meshes: &Arc<FpvMeshCatalog>,
        catalog: &Arc<XAnimCatalog>,
        sources: &PlayerAnimSources,
    ) -> bool {
        Arc::ptr_eq(&self.bodies, bodies)
            && Arc::ptr_eq(&self.meshes, meshes)
            && Arc::ptr_eq(&self.catalog, catalog)
            && self.animation_family == sources.family()
            && match (
                self.tree.as_ref(),
                sources.compiled().and_then(|tree| tree.as_ref().ok()),
            ) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
            && match (
                self.script.as_ref(),
                sources
                    .parsed_script()
                    .and_then(|script| script.as_ref().ok()),
            ) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }

    pub fn mesh_identity(&self) -> u64 {
        self.meshes.identity()
    }

    pub fn side(&self, axis: bool) -> Result<&Arc<SoldierPresentation>, &str> {
        self.sides[usize::from(axis)]
            .as_ref()
            .map_err(String::as_str)
    }
}
