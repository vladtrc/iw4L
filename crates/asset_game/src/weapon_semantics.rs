use asset_core::FamilyId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeImpact {
    Hit,
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeleeCuePrecedence {
    AuthoredFirst,
    KnifeFirst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeleeCuePolicy {
    pub precedence: MeleeCuePrecedence,
    pub knife_hit: &'static str,
    pub knife_miss: &'static str,
    pub generic_hit: &'static str,
    pub generic_miss: &'static str,
}
impl MeleeCuePolicy {
    pub fn generic(self, knife: bool, impact: MeleeImpact) -> &'static str {
        match (knife, impact) {
            (true, MeleeImpact::Hit) => self.knife_hit,
            (true, MeleeImpact::Miss) => self.knife_miss,
            (false, MeleeImpact::Hit) => self.generic_hit,
            (false, MeleeImpact::Miss) => self.generic_miss,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BreathCuePolicy {
    pub inhale: &'static str,
    pub exhale: &'static str,
    pub gasp: &'static str,
    pub heartbeat: &'static str,
}
impl BreathCuePolicy {
    pub fn aliases(self) -> [&'static str; 4] {
        [self.inhale, self.exhale, self.gasp, self.heartbeat]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponSemanticPolicy {
    pub thermal_scope: bool,
    pub cue_namespace: FamilyId,
    pub melee_cues: MeleeCuePolicy,
    /// None means the current weapon; Some selects another published row.
    pub melee_weapon: Option<u32>,
    pub breath_cues: BreathCuePolicy,
}

pub struct WeaponSemanticInputs<'a> {
    pub namespace: FamilyId,
    pub native_thermal: bool,
    pub overlay: Option<&'a str>,
    pub use_as_melee: bool,
    /// Resolves an authored bare name in this source family, preserving first-row precedence.
    pub find_weapon: &'a dyn Fn(&str) -> Option<u32>,
}
pub type WeaponSemanticCompiler = for<'a> fn(WeaponSemanticInputs<'a>) -> WeaponSemanticPolicy;

pub fn reference_weapon_semantics(input: WeaponSemanticInputs<'_>) -> WeaponSemanticPolicy {
    WeaponSemanticPolicy {
        thermal_scope: input.native_thermal,
        cue_namespace: input.namespace,
        melee_weapon: None,
        breath_cues: BreathCuePolicy {
            inhale: "weap_sniper_breathin",
            exhale: "weap_sniper_breathout",
            gasp: "weap_sniper_breathgasp",
            heartbeat: "weap_sniper_heartbeat",
        },
        melee_cues: MeleeCuePolicy {
            precedence: MeleeCuePrecedence::KnifeFirst,
            knife_hit: "melee_knife_hit_body",
            knife_miss: "melee_knife_hit_other",
            generic_hit: "melee_hit",
            generic_miss: "melee_hit_other",
        },
    }
}
