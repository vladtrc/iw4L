use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    DoublePoints,
    InstaKill,
    MaxAmmo,
    Nuke,
    Carpenter,
    BonusPoints,
    ZombieBlood,
}

impl Kind {
    fn model(self) -> &'static str {
        match self {
            Self::DoublePoints => "zombie_x2_icon",
            Self::InstaKill => "zombie_skull",
            Self::MaxAmmo => "zombie_ammocan",
            Self::Nuke => "zombie_bomb",
            Self::Carpenter => "zombie_carpenter",
            Self::BonusPoints => "zombie_z_money_icon",
            Self::ZombieBlood => "p6_zm_tm_blood_power_up",
        }
    }
}

#[derive(Clone, Debug)]
struct Drop {
    kind: Kind,
    object: u64,
    origin: [f32; 3],
    born: u32,
    rise: f32,
    rise_ms: u32,
}

#[derive(Clone, Debug)]
struct Blast {
    origin: [f32; 3],
    due: u32,
    targets: Option<VecDeque<u64>>,
}

#[derive(Clone, Debug)]
pub(super) struct Powerups {
    drops: Vec<Drop>,
    bag: Vec<Kind>,
    count: u8,
    earned: u64,
    threshold: f64,
    increment: f64,
    poll: u32,
    pending: bool,
    double_until: u32,
    instant_until: u32,
    blasts: Vec<Blast>,
    repairs: Option<VecDeque<(usize, usize)>>,
    blood: BTreeMap<ClientId, u32>,
}

impl Default for Powerups {
    fn default() -> Self {
        Self {
            drops: Vec::new(),
            bag: Vec::new(),
            count: 0,
            earned: 0,
            threshold: 2000.0,
            increment: 2000.0,
            poll: 0,
            pending: false,
            double_until: 0,
            instant_until: 0,
            blasts: Vec::new(),
            repairs: None,
            blood: BTreeMap::new(),
        }
    }
}

impl Powerups {
    pub(super) fn new_round(&mut self) {
        self.count = 0;
    }

    pub(super) fn reward(&mut self, tick: Tick, amount: i32) -> i32 {
        let points = amount.saturating_mul(if tick.0 < self.double_until { 2 } else { 1 });
        self.earned = self.earned.saturating_add(points.max(0) as u64);
        points
    }

    pub(super) fn instant(&self, tick: Tick) -> bool {
        tick.0 < self.instant_until
    }

    pub(super) fn blood(&self, client: ClientId, tick: Tick) -> bool {
        self.blood.get(&client).is_some_and(|&until| tick.0 < until)
    }

    pub(super) fn drop_count(&self) -> u8 {
        self.count
    }

    fn poll(&mut self, tick: Tick) {
        if tick.0 < self.poll {
            return;
        }
        self.poll = tick.0 + ticks(500);
        if self.earned as f64 > self.threshold {
            self.increment *= 1.14;
            self.threshold = self.earned as f64 + self.increment;
            self.pending = true;
        }
    }

    pub(super) fn killed(&mut self, world: &mut World, at: [f32; 3], destroyed: usize) {
        if self.count >= 4
            || (!self.pending && super::super::natives::math::random(world) % 100 >= 3)
        {
            return;
        }
        let mut origin = at;
        origin[2] += 40.0;
        let frame = FrameWorld::from_world(world);
        if ground(&frame, at).is_none() {
            return;
        }
        drop(frame);
        if self.bag.is_empty() {
            self.bag = vec![
                Kind::DoublePoints,
                Kind::InstaKill,
                Kind::MaxAmmo,
                Kind::Nuke,
                Kind::Carpenter,
            ];
            for end in (1..self.bag.len()).rev() {
                let index = super::super::natives::math::random(world) as usize % (end + 1);
                self.bag.swap(index, end);
            }
        }
        let Some(index) = self.bag.iter().rposition(|kind| {
            !matches!(kind, Kind::Carpenter) || (destroyed >= 5 && self.repairs.is_none())
        }) else {
            self.bag.clear();
            return;
        };
        let kind = self.bag[index];
        if self.spawn(world, kind, origin) {
            self.bag.remove(index);
            self.count += 1;
            self.pending = false;
        }
    }

    pub(super) fn spawn(&mut self, world: &mut World, kind: Kind, origin: [f32; 3]) -> bool {
        self.spawn_with_rise(world, kind, origin, 0.0, 0)
    }

    pub(super) fn spawn_dig(&mut self, world: &mut World, kind: Kind, origin: [f32; 3]) -> bool {
        self.spawn_with_rise(world, kind, origin, 40.0, 600)
    }

    fn spawn_with_rise(
        &mut self,
        world: &mut World,
        kind: Kind,
        origin: [f32; 3],
        rise: f32,
        rise_ms: u32,
    ) -> bool {
        if FrameWorld::from_world(world)
            .model_capability(kind.model())
            .flatten()
            .is_none()
            || world.resource::<Runtime>().entities.len()
                >= super::super::entities::MAX_SCRIPT_ENTITIES
        {
            return false;
        }
        let object = {
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "zombie_powerup") else {
                return false;
            };
            object
        };
        let Ok(presence) = super::super::presence::spawn_presence(world, origin) else {
            world.resource_mut::<Runtime>().delete_entity(object);
            return false;
        };
        let tick = world.resource::<crate::step::StepRequest>().tick;
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.set_object_field(object, "origin", Value::Vector(origin));
        runtime.set_object_field(object, "angles", Value::Vector([0.0; 3]));
        runtime.set_object_field(object, "model", Value::string(kind.model()));
        let entity = runtime.entities.get_mut(&object).unwrap();
        entity.presence = Some(presence);
        entity.solid = false;
        entity.contents = 0;
        self.drops.push(Drop {
            kind,
            object,
            origin: [origin[0], origin[1], origin[2] + rise],
            born: tick.0,
            rise,
            rise_ms,
        });
        true
    }
}

fn reward_all(
    world: &mut World,
    state: &mut Survival,
    powers: &mut Powerups,
    tick: Tick,
    amount: i32,
) {
    for client in FrameWorld::from_world(world).client_ids_sorted() {
        if state.survivors.contains_key(&client.0) {
            score(world, client, powers.reward(tick, amount));
        }
    }
}

pub(super) fn advance(world: &mut World, state: &mut Survival, tick: Tick) {
    let mut powers = std::mem::take(&mut state.powerups);
    powers.poll(tick);
    let frame = FrameWorld::from_world(world);
    let players: Vec<_> = frame
        .client_ids_sorted()
        .into_iter()
        .filter(|&id| {
            frame
                .client_meta(id)
                .is_some_and(|meta| meta.lifecycle == ClientLifecycle::Alive)
        })
        .filter_map(|id| frame.player(id).map(|ps| (id, ps.origin)))
        .collect();
    drop(frame);
    for drop in std::mem::take(&mut powers.drops) {
        let elapsed = tick
            .0
            .saturating_sub(drop.born)
            .saturating_mul(crate::MATCH_TICK_MS);
        let age = elapsed.saturating_sub(drop.rise_ms);
        if drop.rise_ms > 0 && elapsed <= drop.rise_ms {
            let mut at = drop.origin;
            at[2] -= drop.rise * (1.0 - elapsed as f32 / drop.rise_ms as f32);
            world.resource_mut::<Runtime>().set_object_field(
                drop.object,
                "origin",
                Value::Vector(at),
            );
        }
        let picker = players
            .iter()
            .find(|(_, at)| {
                elapsed >= drop.rise_ms
                    && Vec3::from_array(*at).distance_squared(Vec3::from_array(drop.origin))
                        <= 4096.0
            })
            .map(|(client, _)| *client);
        let picked = picker.is_some();
        if age >= 26500 || picked {
            world.resource_mut::<Runtime>().delete_entity(drop.object);
            if age >= 26500 || !picked {
                effect(world, tick, "misc/fx_zombie_powerup_off", drop.origin);
                continue;
            }
            effect(world, tick, "misc/fx_zombie_powerup_grab", drop.origin);
            effect(world, tick, "misc/fx_zombie_powerup_wave", drop.origin);
            match drop.kind {
                Kind::DoublePoints => powers.double_until = tick.0 + ticks(30000),
                Kind::InstaKill => powers.instant_until = tick.0 + ticks(30000),
                Kind::MaxAmmo => {
                    let mut frame = FrameWorld::from_world(world);
                    for &(client, _) in &players {
                        if state
                            .survivors
                            .get(&client.0)
                            .is_none_or(|survivor| survivor.downed_since.is_some())
                        {
                            continue;
                        }
                        for weapon in crate::script_player::weapons(
                            &frame,
                            client,
                            crate::script_player::WeaponList::All,
                        ) {
                            crate::script_player::give_max_ammo(&mut frame, client, weapon);
                        }
                    }
                }
                Kind::Nuke => {
                    effect(world, tick, "misc/fx_zombie_mini_nuke_hotness", drop.origin);
                    powers.blasts.push(Blast {
                        origin: drop.origin,
                        due: tick.0 + ticks(500),
                        targets: None,
                    });
                }
                Kind::Carpenter => {
                    powers.repairs = Some(
                        state
                            .barriers
                            .iter()
                            .enumerate()
                            .flat_map(|(index, barrier)| {
                                (barrier.boards as usize..barrier.models.len())
                                    .map(move |board| (index, board))
                            })
                            .collect(),
                    );
                }
                Kind::BonusPoints => {
                    let amount = (1 + super::super::natives::math::random(world) % 5) as i32 * 50;
                    let amount = powers.reward(tick, amount);
                    score(world, picker.unwrap(), amount);
                }
                Kind::ZombieBlood => {
                    powers.blood.insert(picker.unwrap(), tick.0 + ticks(30000));
                }
            }
        } else {
            let hidden = if age < 15000 {
                false
            } else if age < 22500 {
                (age - 15000) / 500 % 2 != 0
            } else if age < 25000 {
                (15 + (age - 22500) / 250) % 2 != 0
            } else {
                (25 + (age - 25000) / 100) % 2 != 0
            };
            let mut runtime = world.resource_mut::<Runtime>();
            if let Some(entity) = runtime.entities.get_mut(&drop.object) {
                entity.hidden = hidden;
            }
            runtime.set_object_field(
                drop.object,
                "angles",
                Value::Vector([0.0, (age as f32 * 0.09) % 360.0, 0.0]),
            );
            powers.drops.push(drop);
        }
    }
    for mut blast in std::mem::take(&mut powers.blasts) {
        if tick.0 < blast.due {
            powers.blasts.push(blast);
            continue;
        }
        if blast.targets.is_none() {
            let mut targets: Vec<_> = state
                .actors
                .iter()
                .map(|(&id, actor)| (id, actor.origin))
                .collect();
            targets.sort_by(|a, b| {
                Vec3::from_array(a.1)
                    .distance_squared(Vec3::from_array(blast.origin))
                    .total_cmp(
                        &Vec3::from_array(b.1).distance_squared(Vec3::from_array(blast.origin)),
                    )
            });
            blast.targets = Some(targets.into_iter().map(|(id, _)| id).collect());
            blast.due = tick.0 + ticks(100 + super::super::natives::math::random(world) % 600);
            powers.blasts.push(blast);
            continue;
        }
        let targets = blast.targets.as_mut().unwrap();
        while targets
            .front()
            .is_some_and(|id| !state.actors.contains_key(id))
        {
            targets.pop_front();
        }
        if let Some(id) = targets.pop_front() {
            state.actors.remove(&id);
            world.resource_mut::<Runtime>().delete_entity(id);
            if targets.is_empty() {
                reward_all(world, state, &mut powers, tick, 400);
            } else {
                blast.due = tick.0 + ticks(100 + super::super::natives::math::random(world) % 600);
                powers.blasts.push(blast);
            }
        } else {
            reward_all(world, state, &mut powers, tick, 400);
        }
    }
    if let Some(mut repairs) = powers.repairs.take() {
        if let Some((index, _)) = repairs.pop_front() {
            if let Some(barrier) = state.barriers.get_mut(index) {
                let board = barrier.boards as usize;
                if board < barrier.models.len() {
                    board_visibility(world, barrier, board, true);
                    barrier.boards = (board + 1) as u8;
                }
            }
            powers.repairs = Some(repairs);
        } else {
            reward_all(world, state, &mut powers, tick, 200);
        }
    }
    let label = match (tick.0 < powers.double_until, tick.0 < powers.instant_until) {
        (true, true) => "DOUBLE POINTS   INSTA-KILL",
        (true, false) => "DOUBLE POINTS",
        (false, true) => "INSTA-KILL",
        _ => "",
    };
    for (&client, survivor) in &mut state.survivors {
        let label = if powers.blood(ClientId(client), tick) {
            format!("{label}   ZOMBIE BLOOD")
        } else {
            label.to_owned()
        };
        if survivor.powerup_label.is_none() {
            survivor.powerup_label = make_hud(world, ClientId(client), 350.0, 1.0);
        }
        if survivor.last_powerups != label {
            if let Some(object) = survivor.powerup_label {
                hud_text(world, object, &label);
            }
            survivor.last_powerups = label.to_owned();
        }
    }
    state.powerups = powers;
}

pub(super) fn effect(world: &mut World, tick: Tick, name: &str, origin: [f32; 3]) {
    let mut frame = FrameWorld::from_world(world);
    let index = frame.effect_name_index(name);
    frame.push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::PLAY_FX,
        crate::EntityEventPayload {
            number: i32::from(trace_iw4::ENTITYNUM_WORLD),
            event_parm: i32::from(index),
            origin,
            direction: [0.0, 0.0, 1.0],
            ..Default::default()
        },
    );
}
