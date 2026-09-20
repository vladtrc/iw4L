use std::sync::Arc;

use asset_iw4::size::{WEAPON_ANIM_COUNT, weap_anim};

use crate::weapon_catalog::WeaponRegistry;
use asset_anim::XAnimCatalog;
use asset_anim::xanim_clip::AnimClip;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdsOverlayConvention {
    #[default]
    WeightIsFrac,

    PlayAdsAnim,
}

impl AdsOverlayConvention {
    pub const fn dump_token(self) -> &'static str {
        match self {
            Self::WeightIsFrac => "weight_is_frac",
            Self::PlayAdsAnim => "play_ads_anim",
        }
    }

    pub const fn from_namespace(ns: crate::AssetNamespace) -> Self {
        match ns {
            crate::AssetNamespace::T5 => Self::PlayAdsAnim,
            crate::AssetNamespace::Iw4 | crate::AssetNamespace::Iw5 => Self::WeightIsFrac,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WeaponAnimSlot {
    Idle = weap_anim::IDLE as u8,
    EmptyIdle = weap_anim::EMPTY_IDLE as u8,
    Fire = weap_anim::FIRE as u8,
    HoldFire = weap_anim::HOLD_FIRE as u8,
    LastShot = weap_anim::LASTSHOT as u8,
    Rechamber = weap_anim::RECHAMBER as u8,
    Melee = weap_anim::MELEE as u8,
    MeleeCharge = weap_anim::MELEE_CHARGE as u8,
    Reload = weap_anim::RELOAD as u8,
    ReloadEmpty = weap_anim::RELOAD_EMPTY as u8,
    ReloadStart = weap_anim::RELOAD_START as u8,
    ReloadEnd = weap_anim::RELOAD_END as u8,
    Raise = weap_anim::RAISE as u8,
    FirstRaise = weap_anim::FIRST_RAISE as u8,
    Drop = weap_anim::DROP as u8,
    AltRaise = weap_anim::ALT_RAISE as u8,
    AltDrop = weap_anim::ALT_DROP as u8,
    QuickRaise = weap_anim::QUICK_RAISE as u8,
    QuickDrop = weap_anim::QUICK_DROP as u8,
    EmptyRaise = weap_anim::EMPTY_RAISE as u8,
    EmptyDrop = weap_anim::EMPTY_DROP as u8,
    SprintIn = weap_anim::SPRINT_IN as u8,
    SprintLoop = weap_anim::SPRINT_LOOP as u8,
    SprintOut = weap_anim::SPRINT_OUT as u8,
    AdsFire = weap_anim::ADS_FIRE as u8,
    AdsLastShot = weap_anim::ADS_LASTSHOT as u8,
    AdsRechamber = weap_anim::ADS_RECHAMBER as u8,
    AdsUp = weap_anim::ADS_UP as u8,
    AdsDown = weap_anim::ADS_DOWN as u8,
}

impl WeaponAnimSlot {
    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Some(match index {
            weap_anim::IDLE => Self::Idle,
            weap_anim::EMPTY_IDLE => Self::EmptyIdle,
            weap_anim::FIRE => Self::Fire,
            weap_anim::HOLD_FIRE => Self::HoldFire,
            weap_anim::LASTSHOT => Self::LastShot,
            weap_anim::RECHAMBER => Self::Rechamber,
            weap_anim::MELEE => Self::Melee,
            weap_anim::MELEE_CHARGE => Self::MeleeCharge,
            weap_anim::RELOAD => Self::Reload,
            weap_anim::RELOAD_EMPTY => Self::ReloadEmpty,
            weap_anim::RELOAD_START => Self::ReloadStart,
            weap_anim::RELOAD_END => Self::ReloadEnd,
            weap_anim::RAISE => Self::Raise,
            weap_anim::FIRST_RAISE => Self::FirstRaise,
            weap_anim::DROP => Self::Drop,
            weap_anim::ALT_RAISE => Self::AltRaise,
            weap_anim::ALT_DROP => Self::AltDrop,
            weap_anim::QUICK_RAISE => Self::QuickRaise,
            weap_anim::QUICK_DROP => Self::QuickDrop,
            weap_anim::EMPTY_RAISE => Self::EmptyRaise,
            weap_anim::EMPTY_DROP => Self::EmptyDrop,
            weap_anim::SPRINT_IN => Self::SprintIn,
            weap_anim::SPRINT_LOOP => Self::SprintLoop,
            weap_anim::SPRINT_OUT => Self::SprintOut,
            weap_anim::ADS_FIRE => Self::AdsFire,
            weap_anim::ADS_LASTSHOT => Self::AdsLastShot,
            weap_anim::ADS_RECHAMBER => Self::AdsRechamber,
            weap_anim::ADS_UP => Self::AdsUp,
            weap_anim::ADS_DOWN => Self::AdsDown,
            _ => return None,
        })
    }
}

#[derive(Clone)]
pub struct WeaponAnimations {
    pub name: String,

    pub fire_time_ms: i32,

    pub melee_time_ms: i32,

    pub melee_charge_time_ms: i32,

    pub raise_time_ms: i32,

    pub drop_time_ms: i32,

    pub quick_drop_time_ms: i32,

    pub quick_raise_time_ms: i32,

    pub sprint_raise_time_ms: i32,

    pub sprint_loop_time_ms: i32,

    pub sprint_drop_time_ms: i32,

    pub reload_time_ms: i32,

    pub reload_empty_time_ms: i32,

    pub reload_start_time_ms: i32,

    pub reload_end_time_ms: i32,

    pub ads_overlay: AdsOverlayConvention,

    pub inherits_perks: bool,
    clips: [Option<Arc<AnimClip>>; WEAPON_ANIM_COUNT],
}

impl std::fmt::Debug for WeaponAnimations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeaponAnimations")
            .field("name", &self.name)
            .field("fire_time_ms", &self.fire_time_ms)
            .field("raise_time_ms", &self.raise_time_ms)
            .field("sprint_loop_time_ms", &self.sprint_loop_time_ms)
            .field("ads_overlay", &self.ads_overlay)
            .field("resolved_clips", &self.clips.iter().flatten().count())
            .finish()
    }
}

impl WeaponAnimations {
    pub fn resolve(
        name: impl Into<String>,
        sz_xanims: &[Option<String>; WEAPON_ANIM_COUNT],
        fire_time_ms: i32,
        raise_time_ms: i32,
        mut resolve_clip: impl FnMut(&str) -> Option<Arc<AnimClip>>,
    ) -> Self {
        let clips = std::array::from_fn(|index| {
            sz_xanims
                .get(index)
                .and_then(|slot| slot.as_deref())
                .filter(|name| !name.is_empty())
                .and_then(|name| resolve_clip(name))
        });
        Self {
            name: name.into(),
            fire_time_ms,
            melee_time_ms: 0,
            melee_charge_time_ms: 0,
            raise_time_ms,
            drop_time_ms: 0,
            quick_drop_time_ms: 0,
            quick_raise_time_ms: 0,
            sprint_raise_time_ms: 0,
            sprint_loop_time_ms: 0,
            sprint_drop_time_ms: 0,
            reload_time_ms: 0,
            reload_empty_time_ms: 0,
            reload_start_time_ms: 0,
            reload_end_time_ms: 0,
            ads_overlay: AdsOverlayConvention::WeightIsFrac,
            inherits_perks: false,
            clips,
        }
    }

    pub fn with_switch_timers(
        mut self,
        drop_time_ms: i32,
        quick_drop_time_ms: i32,
        quick_raise_time_ms: i32,
    ) -> Self {
        self.drop_time_ms = drop_time_ms;
        self.quick_drop_time_ms = quick_drop_time_ms;
        self.quick_raise_time_ms = quick_raise_time_ms;
        self
    }

    pub fn with_sprint_timers(
        mut self,
        sprint_raise_time_ms: i32,
        sprint_loop_time_ms: i32,
        sprint_drop_time_ms: i32,
    ) -> Self {
        self.sprint_raise_time_ms = sprint_raise_time_ms;
        self.sprint_loop_time_ms = sprint_loop_time_ms;
        self.sprint_drop_time_ms = sprint_drop_time_ms;
        self
    }

    pub fn with_reload_timers(
        mut self,
        reload_time_ms: i32,
        reload_empty_time_ms: i32,
        reload_start_time_ms: i32,
        reload_end_time_ms: i32,
    ) -> Self {
        self.reload_time_ms = reload_time_ms;
        self.reload_empty_time_ms = reload_empty_time_ms;
        self.reload_start_time_ms = reload_start_time_ms;
        self.reload_end_time_ms = reload_end_time_ms;
        self
    }

    pub fn with_ads_overlay(mut self, ads_overlay: AdsOverlayConvention) -> Self {
        self.ads_overlay = ads_overlay;
        self
    }

    pub fn with_inherits_perks(mut self, inherits_perks: bool) -> Self {
        self.inherits_perks = inherits_perks;
        self
    }

    pub fn from_registry(registry: &WeaponRegistry, index: u32, xanims: &XAnimCatalog) -> Self {
        let name = registry.name_of(index).to_owned();
        let clips = std::array::from_fn(|slot| {
            registry
                .sz_xanim_edges_of(index)
                .and_then(|row| row.get(slot).copied())
                .and_then(|edge| edge.bound_index())
                .and_then(|order| xanims.clip_at(order))
        });
        Self {
            name,
            fire_time_ms: 0,
            melee_time_ms: 0,
            melee_charge_time_ms: 0,
            raise_time_ms: 0,
            drop_time_ms: 0,
            quick_drop_time_ms: 0,
            quick_raise_time_ms: 0,
            sprint_raise_time_ms: 0,
            sprint_loop_time_ms: 0,
            sprint_drop_time_ms: 0,
            reload_time_ms: 0,
            reload_empty_time_ms: 0,
            reload_start_time_ms: 0,
            reload_end_time_ms: 0,
            ads_overlay: AdsOverlayConvention::WeightIsFrac,
            inherits_perks: false,
            clips,
        }
        .with_registry_facts(registry, index)
    }

    pub fn from_registry_table(
        registry: &WeaponRegistry,
        index: u32,
        sz_xanims: Option<&[Option<String>; WEAPON_ANIM_COUNT]>,
        resolve_clip: impl FnMut(&str) -> Option<Arc<AnimClip>>,
    ) -> Self {
        let name = registry.name_of(index).to_owned();
        let empty = [const { None }; WEAPON_ANIM_COUNT];
        let sz_xanims = sz_xanims.unwrap_or(&empty);
        Self::resolve(name, sz_xanims, 0, 0, resolve_clip).with_registry_facts(registry, index)
    }

    fn with_registry_facts(mut self, registry: &WeaponRegistry, index: u32) -> Self {
        if let Some(facts) = registry.facts_of(index) {
            self.melee_time_ms = facts.melee_time_ms;
            self.melee_charge_time_ms = facts.melee_charge_time_ms;
        }
        let (fire_time_ms, raise_time_ms) = registry.timers_of(index);
        let (drop_time_ms, quick_drop_time_ms, quick_raise_time_ms) =
            registry.switch_timers_of(index);
        let (sprint_raise_time_ms, sprint_loop_time_ms, sprint_drop_time_ms) =
            registry.sprint_timers_of(index);
        let (reload_time_ms, reload_empty_time_ms, reload_start_time_ms, reload_end_time_ms) =
            registry.reload_timers_of(index);
        self.with_fire_raise(fire_time_ms, raise_time_ms)
            .with_switch_timers(drop_time_ms, quick_drop_time_ms, quick_raise_time_ms)
            .with_sprint_timers(
                sprint_raise_time_ms,
                sprint_loop_time_ms,
                sprint_drop_time_ms,
            )
            .with_reload_timers(
                reload_time_ms,
                reload_empty_time_ms,
                reload_start_time_ms,
                reload_end_time_ms,
            )
            .with_ads_overlay(AdsOverlayConvention::from_namespace(
                registry
                    .namespace_of(index)
                    .unwrap_or(crate::AssetNamespace::Iw4),
            ))
            .with_inherits_perks(
                registry
                    .facts_of(index)
                    .map(|f| f.inherits_perks)
                    .unwrap_or(false),
            )
    }

    fn with_fire_raise(mut self, fire_time_ms: i32, raise_time_ms: i32) -> Self {
        self.fire_time_ms = fire_time_ms;
        self.raise_time_ms = raise_time_ms;
        self
    }

    pub fn clip(&self, slot: WeaponAnimSlot) -> Option<&Arc<AnimClip>> {
        self.clips[slot.index()].as_ref()
    }

    pub fn clip_at(&self, index: usize) -> Option<&Arc<AnimClip>> {
        self.clips.get(index).and_then(|c| c.as_ref())
    }

    pub fn install_clips(
        &self,
        scheduler: &mut crate::ClipScheduler,
    ) -> Result<(), crate::ClipSchedulerError> {
        for (node, clip) in self.clips.iter().enumerate() {
            if let Some(clip) = clip {
                scheduler.set_clip(node, Arc::clone(clip))?;
            }
        }
        Ok(())
    }

    pub fn resolved_count(&self) -> usize {
        self.clips.iter().flatten().count()
    }
}
