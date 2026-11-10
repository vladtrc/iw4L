use bevy::prelude::Resource;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyRole {
    Authority,
    Presentation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyImportance {
    Required,
    Optional,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedDependency {
    pub key: String,
    pub source: String,
    pub role: DependencyRole,
    pub importance: DependencyImportance,
    pub available: bool,
}

#[derive(Clone, Debug, Resource)]
pub struct PreparedCapabilities {
    pub version: u32,
    pub products: frame::WorldProducts,
    pub dependencies: Vec<PreparedDependency>,
    pub script_fx: std::collections::BTreeSet<String>,
}

impl PreparedCapabilities {
    pub fn from_content(
        products: frame::WorldProducts,
        fx: &asset_game::FxDefinitions,
        references: impl IntoIterator<Item = (String, String)>,
        inputs: asset_game::WeaponPresentationInputs<'_>,
    ) -> Self {
        let mut script_fx = std::collections::BTreeSet::new();
        let mut dependencies = Vec::new();
        for family in asset_core::families().iter() {
            let namespace = family.id();
            for name in fx.names() {
                if fx.get_in(namespace, name).is_none() {
                    continue;
                }
                let key = format!("{}:{name}", namespace.as_str()).to_ascii_lowercase();
                script_fx.insert(key.clone());
                script_fx.insert(name.to_ascii_lowercase());
                dependencies.push(PreparedDependency {
                    key,
                    source: format!("{} effect corpus", namespace.as_str()),
                    role: DependencyRole::Presentation,
                    importance: DependencyImportance::Optional,
                    available: true,
                });
            }
        }
        for (key, source) in references {
            let available = script_fx.contains(&key);
            script_fx.insert(key.clone());
            dependencies.push(PreparedDependency {
                key,
                source,
                role: DependencyRole::Presentation,
                importance: DependencyImportance::Optional,
                available,
            });
        }

        for (_, policy) in asset_game::weapon_policies() {
            if let Some(requirements) = policy.runtime.presentation_requirements {
                for requirement in requirements(inputs) {
                    dependencies.push(PreparedDependency {
                        key: requirement.key,
                        source: requirement.source.into(),
                        role: DependencyRole::Presentation,
                        importance: DependencyImportance::Required,
                        available: requirement.available,
                    });
                }
            }
        }
        Self {
            version: 1,
            products,
            dependencies,
            script_fx,
        }
    }
    pub fn validate(&self, role: frame::RuntimeRole) -> Result<(), String> {
        for dependency in &self.dependencies {
            let required_role = match dependency.role {
                DependencyRole::Authority => role.runs_authority(),
                DependencyRole::Presentation => role.runs_client(),
            };
            if required_role
                && dependency.importance == DependencyImportance::Required
                && !dependency.available
            {
                return Err(format!(
                    "Required capability {} missing ({})",
                    dependency.key, dependency.source
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn effect_available(fx: &asset_game::FxDefinitions, owner: &str, name: &str) -> bool {
    asset_core::FamilyId::parse(owner).is_some_and(|id| fx.get_in(id, name).is_some())
}
pub(crate) fn sound_available(
    aliases: Option<&std::collections::BTreeMap<String, Option<bool>>>,
    key: &str,
) -> bool {
    aliases.is_some_and(|aliases| aliases.get(key).is_some_and(|facts| facts.is_some()))
}
