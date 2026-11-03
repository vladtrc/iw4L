use asset_core::{AssetEdge, FxSpace, TracerSpace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponCombatFx {
    pub view_flash: AssetEdge<FxSpace>,
    pub view_flash_hint: Option<String>,
    pub world_flash: AssetEdge<FxSpace>,
    pub world_flash_hint: Option<String>,
    pub view_shell_eject: AssetEdge<FxSpace>,
    pub view_shell_eject_hint: Option<String>,
    pub world_shell_eject: AssetEdge<FxSpace>,
    pub world_shell_eject_hint: Option<String>,
    pub view_last_shot_eject: AssetEdge<FxSpace>,
    pub view_last_shot_eject_hint: Option<String>,
    pub world_last_shot_eject: AssetEdge<FxSpace>,
    pub world_last_shot_eject_hint: Option<String>,
    pub explosion: AssetEdge<FxSpace>,
    pub explosion_hint: Option<String>,

    pub tracer: AssetEdge<TracerSpace>,

    pub tracer_hint: Option<String>,

    pub namespace: crate::FamilyId,

    pub last_shot_eject_pair_authored: bool,
}

impl WeaponCombatFx {
    pub fn empty(namespace: crate::FamilyId) -> Self {
        Self {
            namespace,
            view_flash: AssetEdge::Absent,
            view_flash_hint: None,
            world_flash: AssetEdge::Absent,
            world_flash_hint: None,
            view_shell_eject: AssetEdge::Absent,
            view_shell_eject_hint: None,
            world_shell_eject: AssetEdge::Absent,
            world_shell_eject_hint: None,
            view_last_shot_eject: AssetEdge::Absent,
            view_last_shot_eject_hint: None,
            world_last_shot_eject: AssetEdge::Absent,
            world_last_shot_eject_hint: None,
            explosion: AssetEdge::Absent,
            explosion_hint: None,
            tracer: AssetEdge::Absent,
            tracer_hint: None,
            last_shot_eject_pair_authored: false,
        }
    }
    fn present_bound<'a>(
        &self,
        edge: AssetEdge<FxSpace>,
        hint: &'a Option<String>,
    ) -> Option<crate::FxName<'a>> {
        edge.is_bound()
            .then_some(hint.as_deref())
            .flatten()
            .filter(|name| !name.is_empty())
            .map(|name| crate::FxName::new(self.namespace, name))
    }

    pub fn view_flash_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.view_flash, &self.view_flash_hint)
    }

    pub fn world_flash_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.world_flash, &self.world_flash_hint)
    }

    pub fn flash_present(&self, player_view: bool) -> Option<crate::FxName<'_>> {
        if player_view {
            self.view_flash_present()
        } else {
            self.world_flash_present()
        }
    }

    pub fn flash_edge(&self, player_view: bool) -> AssetEdge<FxSpace> {
        if player_view {
            self.view_flash
        } else {
            self.world_flash
        }
    }

    pub fn view_shell_eject_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.view_shell_eject, &self.view_shell_eject_hint)
    }

    pub fn world_shell_eject_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.world_shell_eject, &self.world_shell_eject_hint)
    }

    pub fn brass_present(&self, player_view: bool) -> Option<crate::FxName<'_>> {
        if player_view {
            self.view_shell_eject_present()
        } else {
            self.world_shell_eject_present()
        }
    }

    pub fn last_shot_eject_pair_authored(&self) -> bool {
        self.last_shot_eject_pair_authored
    }

    pub fn last_shot_eject_present(&self, player_view: bool) -> Option<crate::FxName<'_>> {
        if player_view {
            self.present_bound(self.view_last_shot_eject, &self.view_last_shot_eject_hint)
        } else {
            self.present_bound(self.world_last_shot_eject, &self.world_last_shot_eject_hint)
        }
    }

    pub fn brass_present_for_event(
        &self,
        player_view: bool,
        last_shot: bool,
    ) -> Option<crate::FxName<'_>> {
        if last_shot && self.last_shot_eject_pair_authored() {
            self.last_shot_eject_present(player_view)
        } else {
            self.brass_present(player_view)
        }
    }

    pub fn brass_edge(&self, player_view: bool) -> AssetEdge<FxSpace> {
        if player_view {
            self.view_shell_eject
        } else {
            self.world_shell_eject
        }
    }

    pub fn brass_edge_for_event(&self, player_view: bool, last_shot: bool) -> AssetEdge<FxSpace> {
        if last_shot && self.last_shot_eject_pair_authored() {
            if player_view {
                self.view_last_shot_eject
            } else {
                self.world_last_shot_eject
            }
        } else {
            self.brass_edge(player_view)
        }
    }

    pub fn explosion_present(&self) -> Option<crate::FxName<'_>> {
        self.present_bound(self.explosion, &self.explosion_hint)
    }

    pub fn edges(&self) -> [AssetEdge<FxSpace>; 7] {
        [
            self.view_flash,
            self.world_flash,
            self.view_shell_eject,
            self.world_shell_eject,
            self.view_last_shot_eject,
            self.world_last_shot_eject,
            self.explosion,
        ]
    }
}
