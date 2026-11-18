use crate::frame::FrameWorld;
use crate::{AuthorityModelOwner, ClientId, ScriptModelId, Tick};
use asset_core::FamilyId;
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub struct RadiusDamage {
    pub origin: [f32; 3],
    pub radius: f32,
    pub inner: f32,
    pub outer: f32,
    pub attacker: Option<ClientId>,
    pub exclude: Option<ScriptModelId>,
    pub cone: Option<([f32; 3], f32)>,
}
pub trait ModelBehavior: Send + Sync + 'static {
    /// Initialize family-owned state from the authored model metadata.
    /// The registered family owns the metadata interpretation.
    fn initialize(&self, dobj: &mut crate::AuthorityDObjState, metadata: &dyn std::any::Any);
    fn apply_hit(
        &self,
        world: &mut FrameWorld,
        tick: Tick,
        owner: AuthorityModelOwner,
        bone: u16,
        amount: u32,
        attacker: Option<ClientId>,
    ) -> bool;
    fn apply_piece_hit(
        &self,
        world: &mut FrameWorld,
        tick: Tick,
        owner: AuthorityModelOwner,
        piece: i32,
        amount: i32,
        attacker: Option<ClientId>,
    ) -> bool;
    fn apply_radius(&self, world: &mut FrameWorld, tick: Tick, damage: &RadiusDamage);
    fn stage_matches(
        &self,
        world: &FrameWorld,
        owner: AuthorityModelOwner,
        piece: usize,
        stage: usize,
    ) -> bool;
    fn restart(&self, world: &mut FrameWorld);
}
#[derive(Default)]
pub struct ModelBehaviors(BTreeMap<FamilyId, &'static dyn ModelBehavior>);
impl ModelBehaviors {
    pub fn register(&mut self, family: FamilyId, behavior: &'static dyn ModelBehavior) {
        assert!(!self.0.contains_key(&family), "duplicate model behavior");
        self.0.insert(family, behavior);
    }
}
static BEHAVIORS: OnceLock<ModelBehaviors> = OnceLock::new();
pub fn install_model_behaviors(behaviors: ModelBehaviors) -> Result<(), &'static str> {
    BEHAVIORS
        .set(behaviors)
        .map_err(|_| "model behaviors already installed")
}
fn registered() -> impl Iterator<Item = &'static dyn ModelBehavior> {
    BEHAVIORS
        .get()
        .into_iter()
        .flat_map(|registry| registry.0.values().copied())
}
pub fn initialize(
    family: FamilyId,
    dobj: &mut crate::AuthorityDObjState,
    metadata: &dyn std::any::Any,
) {
    if let Some(behavior) = BEHAVIORS.get().and_then(|registry| registry.0.get(&family)) {
        behavior.initialize(dobj, metadata);
    }
}
pub fn apply_hit(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    bone: u16,
    amount: u32,
    attacker: Option<ClientId>,
) -> bool {
    registered().any(|b| b.apply_hit(world, tick, owner, bone, amount, attacker))
}
pub fn apply_piece_hit(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    piece: i32,
    amount: i32,
    attacker: Option<ClientId>,
) -> bool {
    registered().any(|b| b.apply_piece_hit(world, tick, owner, piece, amount, attacker))
}
pub fn apply_radius(world: &mut FrameWorld, tick: Tick, damage: &RadiusDamage) {
    for behavior in registered() {
        behavior.apply_radius(world, tick, damage);
    }
}
pub fn stage_matches(
    world: &FrameWorld,
    owner: AuthorityModelOwner,
    piece: usize,
    stage: usize,
) -> bool {
    registered().any(|b| b.stage_matches(world, owner, piece, stage))
}
pub fn restart(world: &mut FrameWorld) {
    for behavior in registered() {
        behavior.restart(world);
    }
}
