use sim::{ClientId, PlayerCommand, SimWorld, Tick, TickInput};

pub struct BotInputs {
    actors: Vec<(ClientId, bots::HostController)>,
    pub trace_budget: u32,
    pub route_budget: u32,
    pub navigation: Option<std::sync::Arc<bots::NavGraph>>,
}
impl BotInputs {
    pub fn new(
        actors: impl IntoIterator<Item = (ClientId, u64)>,
        trace_budget: u32,
        route_budget: u32,
    ) -> Result<Self, String> {
        let mut actors: Vec<_> = actors
            .into_iter()
            .map(|(client, seed)| (client, bots::HostController::new(seed)))
            .collect();
        actors.sort_by_key(|(client, _)| *client);
        if actors.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate bot input owner".into());
        }
        if trace_budget == 0 || route_budget == 0 {
            return Err("bot query budgets must be positive".into());
        }
        Ok(Self {
            actors,
            trace_budget,
            route_budget,
            navigation: None,
        })
    }
    pub(crate) fn input(&mut self, tick: Tick, world: &mut SimWorld) -> TickInput {
        let snapshot = world.snapshot(Tick(tick.0.saturating_sub(1)));
        let mut traces = bots::TraceBudget::new(self.trace_budget);
        let mut routes = self.route_budget;
        let mut input = TickInput::default();
        let count = self.actors.len();
        let start = (tick.0 as usize / 2) % count.max(1);
        for index in 0..count {
            let (id, brain) = &mut self.actors[(start + index) % count];
            if world.input_program_owns(*id) {
                continue;
            }
            if snapshot
                .meta
                .for_client(*id)
                .is_none_or(|meta| meta.lifecycle != sim::ClientLifecycle::Alive)
            {
                continue;
            }
            let mut query = bots::Budgeted::new(world, &mut traces);
            let Some(observation) =
                bots::observe_focused(&snapshot, *id, &mut query, brain.focus_target())
            else {
                continue;
            };
            let mut command = brain.drive_nav(
                &observation,
                &mut query,
                self.navigation.as_deref(),
                &mut routes,
                50,
            );
            command.server_time = (tick.0 * 50) as i32;
            input.cmds.push(PlayerCommand::unsequenced(*id, command));
        }
        input.canonicalize();
        input
    }
}
