use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppearanceModelStatus {
    NativeReady,
    BaseByPolicy,
    DeclaredUnavailable {
        source: String,
        reason: &'static str,
    },
}

fn model_status<S: asset_core::IndexSpace>(
    edge: AssetEdge<S>,
    source: Option<&str>,
    base: bool,
) -> AppearanceModelStatus {
    let reason = match edge {
        AssetEdge::Bound(_) => {
            return if base {
                AppearanceModelStatus::BaseByPolicy
            } else {
                AppearanceModelStatus::NativeReady
            };
        }
        AssetEdge::Absent => "appearance model absent",
        AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss) => {
            "appearance model absent from catalog"
        }
        AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable) => {
            "appearance model reference cannot be aliased"
        }
    };
    AppearanceModelStatus::DeclaredUnavailable {
        source: source.unwrap_or("base_model").to_owned(),
        reason,
    }
}

fn resolve_model<S: asset_core::IndexSpace>(
    edges: &[(u8, AssetEdge<S>)],
    models: &[(u8, String)],
    invalid: &[u8],
    choice: &WeaponCamouflageChoice,
    base: AssetEdge<S>,
    base_name: Option<&str>,
) -> (AssetEdge<S>, AppearanceModelStatus) {
    if invalid.contains(&choice.slot) {
        return (
            AssetEdge::Absent,
            AppearanceModelStatus::DeclaredUnavailable {
                source: choice.name.clone(),
                reason: "invalid appearance model name",
            },
        );
    }
    if let Some((_, edge)) = edges.iter().find(|(slot, _)| *slot == choice.slot) {
        let name = models
            .iter()
            .find(|(slot, _)| *slot == choice.slot)
            .map(|(_, name)| name.as_str());
        (*edge, model_status(*edge, name, false))
    } else if let Some((_, name)) = models.iter().find(|(slot, _)| *slot == choice.slot) {
        let edge = AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
        (edge, model_status(edge, Some(name), false))
    } else {
        (base, model_status(base, base_name, true))
    }
}

#[derive(Clone, Debug)]
enum AppearanceRepresentation {
    Models {
        view: AssetEdge<FpvMeshSpace>,
        world: AssetEdge<WorldWeaponSpace>,
        view_status: AppearanceModelStatus,
        world_status: AppearanceModelStatus,
    },
    Materials(usize),
}

#[derive(Clone, Debug)]
pub(super) struct PreparedWeaponAppearance {
    slot: u8,
    name: String,
    caption: Option<String>,
    preview: Option<String>,
    representation: AppearanceRepresentation,
}

impl PreparedWeaponAppearance {
    pub(super) fn prepare(row: &WeaponRow) -> Arc<[Self]> {
        let mut appearances = vec![Self {
            slot: 0,
            name: "none".to_owned(),
            caption: None,
            preview: None,
            representation: AppearanceRepresentation::Models {
                view: row.gun_xmodel_edge,
                world: row.world_model_edge,
                view_status: model_status(row.gun_xmodel_edge, row.gun_xmodel.as_deref(), false),
                world_status: model_status(row.world_model_edge, row.world_model.as_deref(), false),
            },
        }];
        let optional = |value: &str| (!value.is_empty()).then(|| value.to_owned());
        match crate::weapon_policy(row.namespace)
            .expect("weapon appearance owner required")
            .runtime
            .appearance
        {
            crate::WeaponAppearanceKind::Models => {
                appearances.extend(
                    row.camo_models
                        .choices
                        .iter()
                        .filter(|choice| choice.slot != 0)
                        .map(|choice| {
                            let (view, view_status) = resolve_model(
                                &row.camo_view_edges,
                                &row.camo_models.view,
                                &row.camo_models.invalid_view,
                                choice,
                                row.gun_xmodel_edge,
                                row.gun_xmodel.as_deref(),
                            );
                            let (world, world_status) = resolve_model(
                                &row.camo_world_edges,
                                &row.camo_models.world,
                                &row.camo_models.invalid_world,
                                choice,
                                row.world_model_edge,
                                row.world_model.as_deref(),
                            );
                            Self {
                                slot: choice.slot,
                                name: choice.name.clone(),
                                caption: optional(&choice.caption_key),
                                preview: optional(&choice.preview),
                                representation: AppearanceRepresentation::Models {
                                    view,
                                    world,
                                    view_status,
                                    world_status,
                                },
                            }
                        }),
                );
            }
            crate::WeaponAppearanceKind::Materials => {
                appearances.extend(
                    row.material_camos
                        .iter()
                        .enumerate()
                        .filter(|(_, camo)| camo.slot != 0)
                        .map(|(index, camo)| Self {
                            slot: camo.slot,
                            name: camo.name.clone(),
                            caption: optional(&camo.caption_key),
                            preview: optional(&camo.preview),
                            representation: AppearanceRepresentation::Materials(index),
                        }),
                );
            }
        }
        appearances.into()
    }
}

pub struct SelectedWeaponAppearance<'a> {
    registry: &'a WeaponRegistry,
    weapon: u32,
    appearance: &'a PreparedWeaponAppearance,
}

impl<'a> SelectedWeaponAppearance<'a> {
    pub fn slot(&self) -> u8 {
        self.appearance.slot
    }
    pub fn name(&self) -> &'a str {
        &self.appearance.name
    }
    pub fn caption(&self) -> Option<&'a str> {
        self.appearance.caption.as_deref()
    }
    pub fn preview(&self) -> Option<&'a str> {
        self.appearance.preview.as_deref()
    }
    pub fn material_camouflage(&self) -> Option<&'a crate::WeaponCamouflage> {
        match &self.appearance.representation {
            AppearanceRepresentation::Models { .. } => None,
            AppearanceRepresentation::Materials(index) => self.registry.rows[self.weapon as usize]
                .material_camos
                .get(*index),
        }
    }
    pub fn view_status(&self) -> AppearanceModelStatus {
        match &self.appearance.representation {
            AppearanceRepresentation::Models { view_status, .. } => view_status.clone(),
            AppearanceRepresentation::Materials(_) => {
                let row = &self.registry.rows[self.weapon as usize];
                model_status(row.gun_xmodel_edge, row.gun_xmodel.as_deref(), false)
            }
        }
    }
    pub fn world_status(&self) -> AppearanceModelStatus {
        match &self.appearance.representation {
            AppearanceRepresentation::Models { world_status, .. } => world_status.clone(),
            AppearanceRepresentation::Materials(_) => {
                let row = &self.registry.rows[self.weapon as usize];
                model_status(row.world_model_edge, row.world_model.as_deref(), false)
            }
        }
    }
    pub fn view_model<'b>(
        &self,
        catalog: &'b crate::FpvMeshCatalog,
    ) -> Option<(usize, &'b asset_model::FpvMeshEntry)> {
        if self.registry.fpv_catalog_identity != catalog.identity() {
            return None;
        }
        let view = match &self.appearance.representation {
            AppearanceRepresentation::Models { view, .. } => *view,
            AppearanceRepresentation::Materials(_) => {
                self.registry.rows[self.weapon as usize].gun_xmodel_edge
            }
        };
        let order = view.bound_index()?;
        Some((order, catalog.get_at(order)?))
    }
    pub fn world_model<'b>(
        &self,
        catalog: &'b crate::WorldWeaponCatalog,
    ) -> Option<&'b crate::WorldWeaponEntry> {
        self.world_order(catalog)
            .and_then(|order| catalog.get_at(order))
    }
    pub fn world_order(&self, catalog: &crate::WorldWeaponCatalog) -> Option<usize> {
        if self.registry.world_catalog_identity != catalog.identity() {
            return None;
        }
        let world = match &self.appearance.representation {
            AppearanceRepresentation::Models { world, .. } => *world,
            AppearanceRepresentation::Materials(_) => {
                self.registry.rows[self.weapon as usize].world_model_edge
            }
        };
        world
            .bound_index()
            .filter(|&order| catalog.get_at(order).is_some())
    }
    pub fn weapon(&self) -> crate::WeaponHandle {
        self.registry
            .bind_published_row(self.weapon)
            .expect("selected appearance owner")
            .handle()
    }
}

impl WeaponRegistry {
    pub fn camouflage_caption(&self, weapon: u32, name: &str) -> Option<&str> {
        let slot = self.camouflage_slot(weapon, name)?;
        self.select_appearance(weapon, slot)?.caption()
    }

    pub fn camouflage_preview(&self, weapon: u32, name: &str) -> Option<String> {
        let slot = self.camouflage_slot(weapon, name)?;
        self.select_appearance(weapon, slot)?
            .preview()
            .map(str::to_owned)
    }

    pub fn select_appearance(&self, weapon: u32, slot: u8) -> Option<SelectedWeaponAppearance<'_>> {
        let appearance = self
            .rows
            .get(weapon as usize)?
            .appearances
            .iter()
            .find(|appearance| appearance.slot == slot)?;
        Some(SelectedWeaponAppearance {
            registry: self,
            weapon,
            appearance,
        })
    }

    pub fn appearances_of(&self, weapon: u32) -> Vec<SelectedWeaponAppearance<'_>> {
        self.rows.get(weapon as usize).map_or_else(Vec::new, |row| {
            row.appearances
                .iter()
                .filter(|appearance| appearance.slot != 0)
                .map(|appearance| SelectedWeaponAppearance {
                    registry: self,
                    weapon,
                    appearance,
                })
                .collect()
        })
    }

    pub fn camouflage_choices(&self, weapon: u32) -> Vec<(u8, &str)> {
        self.rows.get(weapon as usize).map_or_else(Vec::new, |row| {
            row.appearances
                .iter()
                .filter(|appearance| appearance.slot != 0)
                .map(|appearance| (appearance.slot, appearance.name.as_str()))
                .collect()
        })
    }
}
