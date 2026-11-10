use super::*;

impl WeaponRegistry {
    pub fn shared_rank_table(&self) -> Result<&crate::CapturedStringTable, &'static str> {
        self.shared_ranks
            .get_or_init(|| self.capture_shared_ranks())
            .as_ref()
            .map_err(|error| *error)
    }

    fn capture_shared_ranks(&self) -> Result<crate::CapturedStringTable, &'static str> {
        let host = self
            .family_tables
            .iter()
            .rev()
            .find(|(ns, table)| {
                *ns == crate::FamilyId::IW4 && table.name.eq_ignore_ascii_case("mp/rankTable.csv")
            })
            .map(|(_, table)| table)
            .ok_or("rank.missing_table")?;
        let maximum = |table: &crate::CapturedStringTable| {
            (0..table.rows as i32)
                .filter_map(|row| table.cell(row, 0).parse::<u32>().ok())
                .max()
        };
        let host_max = maximum(host).ok_or("rank.invalid_table")?;
        let donor = self
            .family_tables
            .iter()
            .filter(|(_, table)| table.name.eq_ignore_ascii_case("mp/rankTable.csv"))
            .filter_map(|(_, table)| maximum(table).map(|rank| (rank, table)))
            .max_by_key(|(rank, _)| *rank)
            .ok_or("rank.missing_table")?;
        let mut shared = host.clone();
        if donor.0 > host_max {
            let last = host
                .lookup_row_in_col(0, &host_max.to_string())
                .ok_or("rank.invalid_table")?;
            let first = donor
                .1
                .lookup_row_in_col(0, &(host_max + 1).to_string())
                .ok_or("rank.invalid_table")?;
            let next_xp = host
                .cell(last, 7)
                .parse::<i32>()
                .map_err(|_| "rank.invalid_threshold")?;
            let donor_xp = donor
                .1
                .cell(first, 2)
                .parse::<i32>()
                .map_err(|_| "rank.invalid_threshold")?;
            let offset = next_xp
                .checked_sub(donor_xp)
                .ok_or("rank.invalid_threshold")?;
            for rank in host_max + 1..=donor.0 {
                let row = donor
                    .1
                    .lookup_row_in_col(0, &rank.to_string())
                    .ok_or("rank.invalid_table")?;
                let mut cells = (0..host.columns as i32)
                    .map(|column| donor.1.cell(row, column).to_owned())
                    .collect::<Vec<_>>();
                for column in [2, 7] {
                    cells[column] = cells[column]
                        .parse::<i32>()
                        .ok()
                        .and_then(|xp| xp.checked_add(offset))
                        .ok_or("rank.invalid_threshold")?
                        .to_string();
                }
                cells[6] = host.cell(last, 6).to_owned();
                shared.cells.extend(cells);
                shared.rows += 1;
            }
            let max_row = shared
                .lookup_row_in_col(0, "maxrank")
                .ok_or("rank.invalid_table")?;
            shared.cells[max_row as usize * shared.columns + 1] = donor.0.to_string();
        }
        gamemode_iw4::progression::RankProgression::capture(&shared)?;
        Ok(shared)
    }

    pub fn shared_rank_icons(&self) -> Result<&crate::CapturedStringTable, &'static str> {
        self.shared_icons
            .get_or_init(|| self.capture_shared_icons())
            .as_ref()
            .map_err(|error| *error)
    }

    fn capture_shared_icons(&self) -> Result<crate::CapturedStringTable, &'static str> {
        let mut icons = self
            .family_tables
            .iter()
            .rev()
            .find(|(ns, table)| {
                *ns == crate::FamilyId::IW4
                    && table.name.eq_ignore_ascii_case("mp/rankIconTable.csv")
            })
            .map(|(_, table)| table.clone())
            .ok_or("rank.missing_icons")?;
        let max = (0..icons.rows as i32)
            .filter_map(|row| icons.cell(row, 0).parse::<u32>().ok())
            .max()
            .ok_or("rank.invalid_icons")?;
        let shared = self.shared_rank_table()?;
        let shared_max = shared
            .lookup_col("maxrank", 1)
            .parse::<u32>()
            .map_err(|_| "rank.invalid_table")?;
        let last = icons
            .lookup_row_in_col(0, &max.to_string())
            .ok_or("rank.invalid_icons")?;
        let last = (0..icons.columns as i32)
            .map(|column| icons.cell(last, column).to_owned())
            .collect::<Vec<_>>();
        for rank in max + 1..=shared_max {
            let mut row = last.clone();
            row[0] = rank.to_string();
            icons.cells.extend(row);
            icons.rows += 1;
        }
        Ok(icons)
    }

    pub fn rank_progression(
        &self,
    ) -> Result<gamemode_iw4::progression::RankProgression, &'static str> {
        gamemode_iw4::progression::RankProgression::capture(self.shared_rank_table()?)
    }

    pub fn item_unlock_requirement(
        &self,
        namespace: crate::FamilyId,
        name: &str,
    ) -> Result<gamemode_iw4::progression::UnlockRequirement, &'static str> {
        let policy = crate::weapon_policy(namespace).ok_or("unlock.unknown_family")?;
        (policy.unlock)(self, namespace, name)
    }

    pub fn weapon_unlock_requirement(
        &self,
        id: u32,
    ) -> Result<gamemode_iw4::progression::UnlockRequirement, &'static str> {
        if id == 0 {
            return Ok(Default::default());
        }
        let selection = self
            .describe_configuration(id)
            .ok_or("unlock.missing_family")?;
        let family = selection.family.as_ref().ok_or("unlock.missing_family")?;
        self.item_unlock_requirement(family.namespace, &family.base)
    }

    pub fn vehicle_compass(&self) -> impl Iterator<Item = (&str, &[String; 2], [i32; 2])> {
        self.vehicle_compass
            .iter()
            .map(|(name, (icons, size))| (name.as_str(), icons, *size))
    }

    pub fn vehicle_accel(&self) -> impl Iterator<Item = (&str, f32)> {
        self.vehicle_accel
            .iter()
            .map(|(name, accel)| (name.as_str(), *accel))
    }

    pub fn vehicle_turrets(&self) -> Vec<(String, String)> {
        self.vehicle_turrets
            .iter()
            .map(|(vehicle, weapon)| (vehicle.clone(), weapon.clone()))
            .collect()
    }

    pub fn world_catalog_identity(&self) -> u64 {
        self.world_catalog_identity
    }

    pub fn native_capture_of(&self, id: u32) -> Option<&crate::WeaponNativeCapture> {
        Some(&self.rows.get(id as usize)?.native_capture)
    }

    pub(super) fn rebuild_name_maps(&mut self) {
        self.by_name.clear();
        self.by_namespaced.clear();
        for (id, row) in self.rows.iter().enumerate().skip(1) {
            if !row.indexed_name {
                continue;
            }
            self.by_namespaced
                .insert((row.namespace, row.name.clone()), id as u32);
            self.by_name.entry(row.name.clone()).or_insert(id as u32);
        }
        for row in &mut self.rows {
            if let Some(name) = row.alternate_weapon.as_deref() {
                row.alternate_index = self
                    .by_namespaced
                    .get(&(row.namespace, normalize_weapon_name(name)))
                    .copied()
                    .unwrap_or(0);
            }
        }
    }

    pub fn reticle_of(&self, index: u32) -> Option<&WeaponReticleAssets> {
        self.rows.get(index as usize).map(|row| &row.reticle)
    }

    pub fn projectile_fx_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in row.projectile_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn sz_xanim_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanim_edges)
    }

    pub fn notetrack_actions_of(
        &self,
        index: u32,
    ) -> impl Iterator<Item = (&str, &LinkedNotetrackAction)> {
        self.rows
            .get(index as usize)
            .into_iter()
            .flat_map(|row| row.notetrack_actions.iter())
            .map(|(note, action)| (note.as_str(), action))
    }

    pub fn notetrack_sound_aliases_of(&self, index: u32) -> impl Iterator<Item = &str> {
        self.rows
            .get(index as usize)
            .into_iter()
            .flat_map(|row| row.notetrack_actions.values())
            .filter_map(|action| action.sound_alias.as_deref())
    }

    pub fn sz_xanim_right_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanim_right_edges)
    }

    pub fn sz_xanim_left_edges_of(
        &self,
        index: u32,
    ) -> Option<&[AssetEdge<XAnimSpace>; WEAPON_ANIM_SLOTS]> {
        self.rows
            .get(index as usize)
            .map(|row| &row.sz_xanim_left_edges)
    }

    pub fn sz_xanim_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in &row.sz_xanim_edges {
                census.push(*edge);
            }
        }
        census
    }

    pub fn bound_weapon_xanim_indices(&self) -> Vec<usize> {
        let mut indices = std::collections::BTreeSet::new();
        for row in self.rows.iter().skip(1) {
            for edge in row
                .sz_xanim_edges
                .iter()
                .chain(&row.sz_xanim_right_edges)
                .chain(&row.sz_xanim_left_edges)
            {
                if let Some(index) = edge.bound_index() {
                    indices.insert(index);
                }
            }
        }
        indices.into_iter().collect()
    }

    pub fn gun_xmodel_edge_of(&self, index: u32) -> Option<AssetEdge<FpvMeshSpace>> {
        self.rows.get(index as usize).map(|row| row.gun_xmodel_edge)
    }

    pub fn camouflage_slot(&self, weapon: u32, name: &str) -> Option<u8> {
        if name.is_empty() || name.eq_ignore_ascii_case("none") {
            return Some(0);
        }
        self.camouflage_choices(weapon)
            .into_iter()
            .find(|(_, own)| own.eq_ignore_ascii_case(name))
            .map(|(slot, _)| slot)
    }

    pub fn script_viewmodel_names(&self) -> impl Iterator<Item = &str> {
        self.script_viewmodels
            .iter()
            .flat_map(|variants| variants.iter())
            .filter(|(_, variant)| variant.hands.is_ok())
            .map(|(name, _)| name.as_str())
    }

    pub fn script_fpv_variants(&self) -> impl Iterator<Item = (u32, u32, &str)> {
        self.script_viewmodels
            .iter()
            .flat_map(|variants| variants.iter())
            .flat_map(|(name, variant)| {
                (1..variant.assemblies.len() as u32)
                    .map(|id| (id, 0, name.as_str()))
                    .chain(
                        variant
                            .alternates
                            .keys()
                            .map(|&(id, parent)| (id, parent, name.as_str())),
                    )
            })
    }

    pub fn uses_script_viewmodels(&self) -> bool {
        self.script_viewmodels.is_some()
    }

    pub fn fpv_hands_for(
        &self,
        index: u32,
        axis: bool,
        viewmodel: Option<&str>,
    ) -> Option<(&asset_model::FpvHands, crate::FpvMeshIndex)> {
        if let Some(variants) = &self.script_viewmodels {
            let hands = variants.get(viewmodel?)?.hands.as_ref().ok()?;
            return Some((hands.hands().choice(), hands.hands().model()));
        }
        self.fpv_hands_of(index, axis)
    }

    pub fn fpv_assemblies_for_viewmodel(
        &self,
        index: u32,
        parent: u32,
        axis: bool,
        viewmodel: Option<&str>,
    ) -> Option<&crate::FpvSideAssemblies> {
        if let Some(variants) = &self.script_viewmodels {
            let variant = variants.get(viewmodel?)?;
            if parent != 0 && self.facts_of(index).is_some_and(|f| f.inventory_type == 3) {
                return variant.alternates.get(&(index, parent))?[usize::from(axis)]
                    .as_ref()?
                    .as_ref()
                    .ok();
            }
            return variant.assemblies.get(index as usize)?[usize::from(axis)]
                .as_ref()?
                .as_ref()
                .ok();
        }
        self.fpv_assemblies_for(index, parent, axis)
    }

    pub fn fpv_tracks_for_viewmodel(
        &self,
        viewmodel: Option<&str>,
    ) -> Option<&crate::FpvClipTracks> {
        if let Some(variants) = &self.script_viewmodels {
            return Some(&variants.get(viewmodel?)?.tracks);
        }
        Some(self.fpv_clip_tracks())
    }

    pub fn fpv_hands_of(
        &self,
        index: u32,
        axis: bool,
    ) -> Option<(&asset_model::FpvHands, crate::FpvMeshIndex)> {
        self.rows.get(index as usize)?.fpv_soldiers[usize::from(axis)]
            .as_ref()
            .and_then(|presentation| presentation.as_ref().ok())
            .map(|presentation| (presentation.hands().choice(), presentation.hands().model()))
    }

    pub fn alternate_fpv_pairs(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.alternate_fpv.keys().copied()
    }

    pub fn fpv_assemblies_for(
        &self,
        index: u32,
        parent: u32,
        axis: bool,
    ) -> Option<&crate::FpvSideAssemblies> {
        if parent != 0 && self.facts_of(index).is_some_and(|f| f.inventory_type == 3) {
            return self.alternate_fpv.get(&(index, parent))?[usize::from(axis)]
                .as_ref()?
                .as_ref()
                .ok();
        }
        self.fpv_assemblies_of(index, axis)
    }

    pub fn fpv_assemblies_of(&self, index: u32, axis: bool) -> Option<&crate::FpvSideAssemblies> {
        self.rows.get(index as usize)?.fpv_assemblies[usize::from(axis)]
            .as_ref()?
            .as_ref()
            .ok()
    }

    pub fn fpv_assembly_gap_of(&self, index: u32, axis: bool) -> String {
        let Some(row) = self.rows.get(index as usize) else {
            return "no such weapon".to_owned();
        };
        match (&row.fpv_mount_plan, &row.fpv_assemblies[usize::from(axis)]) {
            (None, _) => "unlinked selected models".to_owned(),
            (Some(Err(error)), _) => error.to_string(),
            (_, None) => "effective kit / weapon hands".to_owned(),
            (_, Some(Err(error))) => error.clone(),
            (_, Some(Ok(_))) => "linked".to_owned(),
        }
    }

    pub fn fpv_clip_tracks(&self) -> &crate::FpvClipTracks {
        &self.fpv_clip_tracks
    }

    pub fn world_model_edge_of(&self, index: u32) -> Option<AssetEdge<WorldWeaponSpace>> {
        self.rows
            .get(index as usize)
            .map(|row| row.world_model_edge)
    }

    pub fn attachment_world_model_edges_of(&self, index: u32) -> &[AssetEdge<WorldWeaponSpace>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_model_edges.as_slice())
    }

    pub fn attachment_world_mounts_of(&self, index: u32) -> &[Option<String>] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_world_mounts.as_slice())
    }

    pub fn world_model_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.world_model_edge);
        }
        census
    }

    pub fn dependency_gaps(&self) -> Vec<WeaponDependencyGap> {
        (1..self.rows.len() as u32)
            .flat_map(|id| self.dependency_gaps_of(id))
            .collect()
    }

    pub(super) fn native_dependency_gaps_of(&self, id: u32) -> Vec<WeaponDependencyGap> {
        let Some(row) = self.rows.get(id as usize).filter(|_| id != 0) else {
            return Vec::new();
        };
        let named = [
            (
                row.gun_xmodel.is_some() && !row.gun_xmodel_edge.is_bound(),
                "view model",
                &row.gun_xmodel,
            ),
            (
                row.rocket_model.is_some() && !row.rocket_model_edge.is_bound(),
                "FPV rocket model",
                &row.rocket_model,
            ),
            (
                row.world_model_edge.is_unresolved(),
                "world model",
                &row.world_model,
            ),
        ];
        let anims = row
            .sz_xanim_edges
            .iter()
            .zip(&row.sz_xanims)
            .map(|(edge, name)| (edge.is_unresolved(), "anim", name));
        let dual_anims = !row.facts.no_dual_wield
            && row.sz_xanims_right[weap_anim::IDLE]
                .as_deref()
                .is_some_and(|name| !name.is_empty());
        let right_anims = row
            .sz_xanim_right_edges
            .iter()
            .zip(&row.sz_xanims_right)
            .map(|(edge, name)| (dual_anims && edge.is_unresolved(), "right anim", name));
        let left_anims = row
            .sz_xanim_left_edges
            .iter()
            .zip(&row.sz_xanims_left)
            .map(|(edge, name)| (dual_anims && edge.is_unresolved(), "left anim", name));
        let mut gaps: Vec<WeaponDependencyGap> = named
            .into_iter()
            .chain(anims)
            .chain(right_anims)
            .chain(left_anims)
            .filter(|(unresolved, ..)| *unresolved)
            .map(|(_, kind, name)| WeaponDependencyGap {
                id,
                kind,
                name: name.clone().unwrap_or(String::new()),
            })
            .collect();
        for (name, edge) in row
            .attachment_view_models
            .iter()
            .zip(&row.attachment_view_model_edges)
        {
            if !edge.is_bound() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "FPV attachment model",
                    name: name.clone(),
                });
            }
        }
        for (name, edge) in row
            .attachment_world_models
            .iter()
            .zip(&row.attachment_world_model_edges)
        {
            if !edge.is_bound() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "world attachment model",
                    name: name.clone(),
                });
            }
        }
        for ((name, edge), mount) in row
            .attachment_world_models
            .iter()
            .zip(&row.attachment_world_model_edges)
            .zip(&row.attachment_world_mounts)
        {
            if edge.is_bound() && mount.is_none() {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "world attachment mount",
                    name: name.clone(),
                });
            }
        }
        if row.attachment_world_models.len() != row.attachment_world_model_edges.len() {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "world attachment bindings",
                name: format!(
                    "{} names, {} links",
                    row.attachment_world_models.len(),
                    row.attachment_world_model_edges.len()
                ),
            });
        }
        if row.attachment_view_models.len() != row.attachment_view_model_edges.len() {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "FPV attachment bindings",
                name: format!(
                    "{} names, {} links",
                    row.attachment_view_models.len(),
                    row.attachment_view_model_edges.len()
                ),
            });
        }
        gaps
    }

    pub fn dependency_gaps_of(&self, id: u32) -> Vec<WeaponDependencyGap> {
        let mut gaps = self.native_dependency_gaps_of(id);
        let Some(row) = self.rows.get(id as usize).filter(|_| id != 0) else {
            return gaps;
        };
        if let Some(Err(error)) = &row.fpv_mount_plan {
            gaps.push(WeaponDependencyGap {
                id,
                kind: "FPV mount",
                name: error.to_string(),
            });
        }
        if let Some(variants) = &self.script_viewmodels {
            if row.gun_xmodel_edge.is_bound()
                && !variants.values().any(|variant| {
                    variant
                        .assemblies
                        .get(id as usize)
                        .is_some_and(|sides| sides.iter().flatten().any(Result::is_ok))
                })
            {
                let reason = variants
                    .values()
                    .find_map(|variant| {
                        variant.assemblies.get(id as usize).and_then(|sides| {
                            sides
                                .iter()
                                .flatten()
                                .find_map(|value| value.as_ref().err())
                        })
                    })
                    .cloned()
                    .unwrap_or_else(|| "no loaded script viewmodel was prepared".into());
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "script FPV skeleton",
                    name: reason,
                });
            }
            return gaps;
        }
        for (side, assembly) in row.fpv_assemblies.iter().enumerate() {
            if let Some(Err(error)) = assembly {
                gaps.push(WeaponDependencyGap {
                    id,
                    kind: "FPV skeleton",
                    name: format!("{}: {error}", if side == 0 { "allies" } else { "axis" }),
                });
            }
        }
        if row.gun_xmodel_edge.is_bound() {
            for (side, hands) in row.fpv_soldiers.iter().enumerate() {
                if hands.as_ref().is_none_or(|hands| hands.is_err()) {
                    gaps.push(WeaponDependencyGap {
                        id,
                        kind: "FPV hands layout",
                        name: format!(
                            "{}: {}",
                            if side == 0 { "allies" } else { "axis" },
                            hands
                                .as_ref()
                                .and_then(|hands| hands.as_ref().err())
                                .map_or("soldier presentation not prepared", String::as_str)
                        ),
                    });
                }
            }
        }
        gaps
    }

    pub(super) fn native_configuration_admission(
        &self,
        id: u32,
    ) -> Result<(), crate::ConfigurationRefusal> {
        if !self.configuration_supported(id) {
            return Err(crate::ConfigurationRefusal::Unsupported(format!(
                "`{}` needs a runtime mechanism IW4L lacks",
                self.name_of(id)
            )));
        }
        match self.native_dependency_gaps_of(id).first() {
            Some(gap) => Err(crate::ConfigurationRefusal::MissingDependency {
                weapon: self.name_of(id).to_owned(),
                kind: gap.kind,
                name: gap.name.clone(),
            }),
            None => Ok(()),
        }
    }

    pub fn configuration_admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        if !self.configuration_supported(id) {
            return Err(crate::ConfigurationRefusal::Unsupported(format!(
                "`{}` needs a runtime mechanism IW4L lacks",
                self.name_of(id)
            )));
        }
        match self.dependency_gaps_of(id).first() {
            Some(gap) => Err(crate::ConfigurationRefusal::MissingDependency {
                weapon: self.name_of(id).to_owned(),
                kind: gap.kind,
                name: gap.name.clone(),
            }),
            None => Ok(()),
        }
    }

    pub fn world_model_entry<'a>(
        &self,
        index: u32,
        catalog: &'a crate::WorldWeaponCatalog,
    ) -> Option<&'a crate::WorldWeaponEntry> {
        if self.world_catalog_identity != catalog.identity() {
            return None;
        }
        catalog.get_at(self.world_model_edge_of(index)?.bound_index()?)
    }

    pub fn world_model_entry_for<'a>(
        &self,
        index: u32,
        camo: u8,
        catalog: &'a crate::WorldWeaponCatalog,
    ) -> Option<&'a crate::WorldWeaponEntry> {
        self.select_appearance(index, camo)?.world_model(catalog)
    }

    pub fn authored_weapon_sound(&self, index: u32, slot: WeaponSoundSlot) -> Option<&str> {
        self.sounds_of(index)?
            .hint(slot)
            .filter(|name| !name.is_empty())
    }

    pub fn weapon_sound_alias<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        self.weapon_sound_key(index, slot, catalog)
            .map(|(_, alias)| alias)
    }

    pub fn weapon_sound_key<'a>(
        &self,
        index: u32,
        slot: WeaponSoundSlot,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<(crate::FamilyId, &'a str)> {
        let alias = self.authored_weapon_sound(index, slot)?;
        let ns = self.sound_namespace_for(index, alias)?;
        sound_alias_in_bank(Some(alias), ns, catalog)
    }

    pub fn bounce_sound_alias<'a>(
        &self,
        index: u32,
        surf: usize,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<&'a str> {
        let alias = self.bounce_sound_of(index, surf)?;
        let ns = self.sound_namespace_for(index, alias)?;
        sound_alias_in_bank(Some(alias), ns, catalog).map(|(_, alias)| alias)
    }

    pub fn hud_material_edge_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in &self.rows {
            census.push(row.reticle.center_edge);
            census.push(row.reticle.side_edge);
        }
        for row in &self.rows {
            census.push(row.hud_material_edges.overlay);
            census.push(row.hud_material_edges.hud_icon);
            census.push(row.hud_material_edges.pickup_icon);
            census.push(row.hud_material_edges.kill_icon);
        }
        census
    }

    pub fn display_name_key_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.display_name_key.as_deref())
    }

    pub(crate) fn facts_of(&self, index: u32) -> Option<WeaponBodyFacts> {
        self.rows.get(index as usize).map(|row| row.facts)
    }

    pub fn semantic_policy_of(&self, index: u32) -> Option<&crate::WeaponSemanticPolicy> {
        self.rows.get(index as usize)?.semantics.as_ref()
    }

    pub fn melee_weapon_of(&self, index: u32) -> u32 {
        self.semantic_policy_of(index)
            .map_or(index, |policy| policy.melee_weapon.unwrap_or(index))
    }

    pub fn melee_impact_sound_key<'a>(
        &self,
        index: u32,
        knife: bool,
        impact: crate::MeleeImpact,
        catalog: &'a crate::SoundCatalog,
    ) -> Option<(crate::FamilyId, &'a str)> {
        let row = self.rows.get(index as usize)?;
        let semantics = row.semantics.as_ref()?;
        let policy = semantics.melee_cues;
        let namespace = row.component_namespace(WeaponComponent::Sound)?;
        let slot = match impact {
            crate::MeleeImpact::Hit => WeaponSoundSlot::MeleeHit,
            crate::MeleeImpact::Miss => WeaponSoundSlot::MeleeMiss,
        };
        let own = || self.weapon_sound_key(index, slot, catalog);
        let generic =
            |knife| sound_alias_in_bank(Some(policy.generic(knife, impact)), namespace, catalog);
        let knife_alias = || knife.then(|| generic(true)).flatten();
        match policy.precedence {
            crate::MeleeCuePrecedence::AuthoredFirst => own().or_else(knife_alias),
            crate::MeleeCuePrecedence::KnifeFirst => knife_alias().or_else(own),
        }
        .or_else(|| generic(false))
    }

    pub fn alternate_of(&self, index: u32) -> u32 {
        self.rows
            .get(index as usize)
            .map_or(0, |row| row.alternate_index)
    }

    pub fn resolve_index(&self, name: &str) -> Result<Option<u32>, UnknownWeaponName> {
        if name.is_empty() {
            return Ok(None);
        }
        if name.contains(':') {
            match crate::AssetKey::parse(name) {
                Ok(key) => return self.resolve_key(&key),
                Err(_) => {
                    return Err(UnknownWeaponName {
                        name: name.to_owned(),
                    });
                }
            }
        }
        let norm = normalize_weapon_name(name);
        match self.by_name.get(&norm).copied() {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName { name: norm }),
        }
    }

    pub fn resolve_key(&self, key: &crate::AssetKey) -> Result<Option<u32>, UnknownWeaponName> {
        if key.kind != crate::AssetKind::Weapon {
            return Err(UnknownWeaponName {
                name: key.display(),
            });
        }
        let norm = normalize_weapon_name(key.logical_name());
        match self.by_namespaced.get(&(key.namespace, norm)).copied() {
            Some(id) => Ok(Some(id)),
            None => Err(UnknownWeaponName {
                name: key.display(),
            }),
        }
    }

    pub fn identity_namespace_of(&self, index: u32) -> Option<crate::FamilyId> {
        if index == 0 {
            return None;
        }
        self.rows.get(index as usize).map(|row| row.namespace)
    }

    pub fn sound_namespace_for(&self, index: u32, alias: &str) -> Option<crate::FamilyId> {
        self.rows
            .get(index as usize)
            .filter(|_| index != 0)?
            .preparation
            .namespace_for(WeaponComponent::Sound, alias)
    }

    pub fn projectile_model_reference_of(&self, index: u32) -> Option<(crate::FamilyId, &str)> {
        let row = self.rows.get(index as usize)?;
        let name = row.projectile_model.as_deref()?;
        row.projectile_model_edge.bound_index()?;
        Some((
            row.preparation
                .namespace_for(WeaponComponent::Projectile, name)?,
            name,
        ))
    }

    pub fn component_namespace_of(
        &self,
        index: u32,
        component: WeaponComponent,
    ) -> Option<crate::FamilyId> {
        self.rows
            .get(index as usize)
            .filter(|_| index != 0)?
            .component_namespace(component)
    }

    pub fn host_namespace_of(&self, index: u32) -> Option<crate::FamilyId> {
        self.rows
            .get(index as usize)
            .filter(|_| index != 0)
            .map(|row| row.preparation.host_namespace())
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn item_group_count(&self) -> usize {
        self.item_groups.len()
    }

    pub fn namespaced_key_of(&self, index: u32) -> Option<String> {
        self.key_of(index).map(|key| key.to_string())
    }

    pub fn key_of(&self, index: u32) -> Option<crate::AssetKey> {
        let ns = self.identity_namespace_of(index)?;
        let name = self.name_of(index);
        if name.is_empty() {
            return None;
        }
        crate::AssetKey::new(ns, crate::AssetKind::Weapon, gsc_weapon_script_name(name)).ok()
    }

    pub fn namespace_count(&self, ns: crate::FamilyId) -> usize {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.namespace == ns)
            .count()
    }

    #[deprecated(note = "use resolve_index — unknown must not become 0")]
    pub fn index_of(&self, name: &str) -> u32 {
        match self.resolve_index(name) {
            Ok(Some(id)) => id,
            Ok(None) | Err(_) => 0,
        }
    }

    pub fn name_of(&self, index: u32) -> &str {
        self.rows
            .get(index as usize)
            .map(|row| row.name.as_str())
            .unwrap_or("")
    }

    pub fn script_name_of(&self, index: u32) -> String {
        gsc_weapon_script_name(self.name_of(index))
    }

    pub fn native_script_aliases(&self) -> impl Iterator<Item = (String, u32)> + '_ {
        self.rows
            .iter()
            .enumerate()
            .skip(1)
            .filter(|&(_id, row)| {
                normalize_weapon_name(&row.native_script_name) == row.name
                    && row.native_script_name != gsc_weapon_script_name(&row.name)
            })
            .map(|(id, row)| (row.native_script_name.clone(), id as u32))
    }

    pub fn configuration_supported(&self, id: u32) -> bool {
        let Some(row) = self.rows.get(id as usize) else {
            return false;
        };
        let Some(policy) = crate::weapon_policy(row.namespace) else {
            return false;
        };
        (policy.runtime.supports)(crate::WeaponSupportInputs {
            dual_wield: row.facts.dual_wield,
            secondary_model: row.secondary_gun_xmodel.is_some(),
            left_idle: xanims_idle(&row.sz_xanims_left).is_some(),
            gun_model: row.gun_xmodel.as_deref(),
        })
    }

    pub fn weapon_families(&self) -> &crate::WeaponFamilies {
        &self.families
    }

    pub fn resolve_configuration(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<crate::ResolvedConfiguration, crate::ConfigurationRefusal> {
        self.families.resolve(selection, rules, self)
    }

    pub fn describe_configuration(&self, id: u32) -> Option<&crate::WeaponSelection> {
        self.families.describe(id)
    }

    pub fn attachment_caption_keys_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map_or(&[], |row| row.attachment_caption_keys.as_slice())
    }

    pub fn prepared_attachments_of(&self, id: u32) -> &[String] {
        self.rows
            .get(id as usize)
            .map_or(&[], |row| row.prepared_attachments.as_slice())
    }

    pub fn weapon_completion_names(&self) -> &[String] {
        &self.completion_names
    }

    pub fn configuration_label(&self, id: u32) -> String {
        match self
            .describe_configuration(id)
            .and_then(|selection| Some((selection.family.as_ref()?, &selection.attachments)))
        {
            Some((family, attachments)) => {
                let mut label = family.short();
                for name in attachments {
                    label.push_str(" +");
                    label.push_str(name);
                }
                label
            }
            None => self.namespaced_key_of(id).unwrap_or(String::new()),
        }
    }

    pub fn configuration_transition_groups(&self) -> Vec<u32> {
        let mut keys: Vec<_> = self
            .families
            .families()
            .iter()
            .map(|family| family.key.clone())
            .collect();
        keys.sort_by_key(|key| key.asset_key());
        keys.dedup();
        let groups: HashMap<_, _> = keys
            .into_iter()
            .enumerate()
            .map(|(index, key)| (key, index as u32 + 1))
            .collect();
        let mut result = vec![0; self.rows.len()];
        for id in 1..self.rows.len() as u32 {
            let Some(selection) = self.describe_configuration(id) else {
                continue;
            };
            let Some(key) = selection.family.as_ref() else {
                continue;
            };
            if self
                .resolve_configuration(selection, crate::LoadoutRules::default())
                .is_ok_and(|resolved| {
                    self.bind(resolved.handle())
                        .is_ok_and(|weapon| weapon.wire_id() == id)
                })
            {
                result[id as usize] = groups.get(key).copied().unwrap_or(0);
            }
        }
        result
    }

    pub fn list_attachment_choices(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<Vec<crate::AttachmentOption>, crate::ConfigurationRefusal> {
        self.families.attachment_options(selection, rules, self)
    }

    pub fn gun_xmodel_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.gun_xmodel.as_deref())
    }

    pub fn clip_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.clip_model.as_deref())
    }

    pub fn world_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.world_model.as_deref())
    }

    pub fn impact_payload_of(&self, index: u32) -> Option<u32> {
        let row = self.rows.get(index as usize)?;
        let name = normalize_weapon_name(row.impact_payload.as_deref()?);
        self.by_namespaced.get(&(row.namespace, name)).copied()
    }

    pub fn projectile_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.projectile_model.as_deref())
    }

    pub fn rocket_model_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.rocket_model.as_deref())
    }

    pub fn anim_of(&self, index: u32, slot: usize) -> Option<&str> {
        self.rows
            .get(index as usize)?
            .sz_xanims
            .get(slot)?
            .as_deref()
    }

    pub fn idle_anim_of(&self, index: u32) -> Option<&str> {
        self.anim_of(index, weap_anim::IDLE)
    }

    pub fn hide_tags_of(&self, index: u32) -> &[String] {
        self.rows
            .get(index as usize)
            .map(|row| row.hide_tags.as_slice())
            .unwrap_or(&[])
    }

    pub fn sounds_of(&self, index: u32) -> Option<&WeaponSoundAliases> {
        self.rows.get(index as usize).map(|row| &row.sounds)
    }

    pub fn bounce_sound_of(&self, index: u32, surf: usize) -> Option<&str> {
        self.sounds_of(index)?.bounce.get(surf)?.as_deref()
    }

    pub fn combat_fx_of(&self, index: u32) -> Option<&WeaponCombatFx> {
        self.rows.get(index as usize).map(|row| &row.combat_fx)
    }

    pub fn flame_stream_of(&self, index: u32) -> Option<&WeaponFlameStream> {
        self.rows.get(index as usize)?.flame.as_deref()
    }

    pub fn tracer_type_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            census.push(row.combat_fx.tracer);
        }
        census
    }

    pub fn combat_fx_census(&self) -> AssetEdgeCensus {
        let mut census = AssetEdgeCensus::default();
        for row in self.rows.iter().skip(1) {
            for edge in row.combat_fx.edges() {
                census.push(edge);
            }
        }
        census
    }

    pub fn sz_xanims_of(&self, index: u32) -> Option<&[Option<String>; WEAPON_ANIM_SLOTS]> {
        self.rows.get(index as usize).map(|row| &row.sz_xanims)
    }

    pub fn timers_of(&self, index: u32) -> (i32, i32) {
        self.facts_of(index)
            .map(|f| (f.fire_time_ms, f.raise_time_ms))
            .unwrap_or((0, 0))
    }

    pub fn switch_timers_of(&self, index: u32) -> (i32, i32, i32) {
        self.facts_of(index)
            .map(|f| (f.drop_time_ms, f.quick_drop_time_ms, f.quick_raise_time_ms))
            .unwrap_or((0, 0, 0))
    }

    pub fn sprint_timers_of(&self, index: u32) -> (i32, i32, i32) {
        self.facts_of(index)
            .map(|f| {
                (
                    f.sprint_raise_time_ms,
                    f.sprint_loop_time_ms,
                    f.sprint_drop_time_ms,
                )
            })
            .unwrap_or((0, 0, 0))
    }

    pub fn quick_reload_timers_of(&self, index: u32) -> Option<(i32, i32)> {
        let dual_mag = self.facts_of(index)?.dual_mag?;
        Some((dual_mag.reload_ms, dual_mag.reload_empty_ms))
    }

    pub fn reload_timers_of(&self, index: u32) -> (i32, i32, i32, i32) {
        self.facts_of(index)
            .map(|f| {
                (
                    f.reload_time_ms,
                    f.reload_empty_time_ms,
                    f.reload_start_time_ms,
                    f.reload_end_time_ms,
                )
            })
            .unwrap_or((0, 0, 0, 0))
    }

    pub fn len(&self) -> usize {
        self.rows.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn camo_census(&self) -> (usize, usize, usize) {
        let with = self
            .rows
            .iter()
            .filter(|row| !row.camo_models.view.is_empty())
            .count();
        let view_bound = self
            .rows
            .iter()
            .flat_map(|row| &row.camo_view_edges)
            .filter(|(_, edge)| edge.is_bound())
            .count();
        let world_bound = self
            .rows
            .iter()
            .flat_map(|row| &row.camo_world_edges)
            .filter(|(_, edge)| edge.is_bound())
            .count();
        (with, view_bound, world_bound)
    }

    pub fn gun_xmodel_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.gun_xmodel.is_some())
            .count()
    }

    pub fn overlay_material_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.overlay_material.as_deref())
    }

    pub fn overlay_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.overlay_image.as_deref())
    }

    pub fn overlay_is_hud_iris(&self, index: u32) -> bool {
        self.overlay_material_of(index)
            .is_some_and(overlay_name_is_hud_iris)
    }

    pub fn pickup_icon_of(&self, index: u32) -> Option<(&str, i32)> {
        let row = self.rows.get(index as usize)?;
        if row.pickup_icon_authored {
            Some((row.pickup_icon_image.as_deref()?, row.pickup_icon_ratio))
        } else {
            Some((row.hud_icon_image.as_deref()?, row.hud_icon_ratio))
        }
    }

    pub fn hud_icon_namespace_of(&self, index: u32) -> Option<crate::FamilyId> {
        self.component_namespace_of(index, WeaponComponent::HudIconImage)
    }

    pub fn hud_icon_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.hud_icon_image.as_deref())
    }

    pub fn dpad_icon_of(&self, index: u32) -> Option<(&str, i32)> {
        let row = self.rows.get(index as usize)?;
        Some((row.dpad_icon_image.as_deref()?, row.dpad_icon_ratio))
    }

    pub fn dpad_icon_atlas_of(&self, index: u32) -> Option<[u8; 2]> {
        self.rows.get(index as usize)?.dpad_icon_atlas
    }

    pub fn kill_icon_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.kill_icon.as_deref())
    }

    pub fn kill_icon_image_of(&self, index: u32) -> Option<&str> {
        self.rows
            .get(index as usize)
            .and_then(|row| row.kill_icon_image.as_deref())
    }

    pub fn proj_trail_of(&self, index: u32) -> Option<crate::FxName<'_>> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.trail.is_bound().then_some(())?;
        Some(crate::FxName::new(
            row.component_namespace(WeaponComponent::Effect)?,
            row.proj_trail.as_deref()?,
        ))
    }

    pub fn proj_beacon_of(&self, index: u32) -> Option<crate::FxName<'_>> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.beacon.is_bound().then_some(())?;
        Some(crate::FxName::new(
            row.component_namespace(WeaponComponent::Effect)?,
            row.proj_beacon.as_deref()?,
        ))
    }

    pub fn proj_ignition_of(&self, index: u32) -> Option<crate::FxName<'_>> {
        let row = self.rows.get(index as usize)?;
        row.projectile_fx.ignition.is_bound().then_some(())?;
        Some(crate::FxName::new(
            row.component_namespace(WeaponComponent::Effect)?,
            row.proj_ignition.as_deref()?,
        ))
    }

    pub fn world_model_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.world_model.is_some())
            .count()
    }

    pub fn projectile_model_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model.is_some())
            .count()
    }

    pub fn projectile_model_bound_n(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model_edge.is_bound())
            .count()
    }

    pub fn projectile_model_name_hint_n(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.projectile_model.is_some() && !row.projectile_model_edge.is_bound())
            .count()
    }

    pub fn idle_anim_count(&self) -> usize {
        (1..=self.len() as u32)
            .filter(|&id| self.idle_anim_of(id).is_some())
            .count()
    }

    pub fn sz_xanims_count(&self) -> usize {
        self.rows
            .iter()
            .skip(1)
            .filter(|row| row.sz_xanims.iter().any(|n| n.is_some()))
            .count()
    }
}

impl crate::WeaponUnlockContent for WeaponRegistry {
    fn tables(&self) -> &[(crate::FamilyId, crate::CapturedStringTable)] {
        &self.family_tables
    }
    fn contains_family(&self, namespace: crate::FamilyId, name: &str) -> bool {
        self.families
            .find(&crate::FamilyKey::new(namespace, name))
            .is_some()
    }
}
