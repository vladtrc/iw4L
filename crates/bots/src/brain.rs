use movement_iw4::ANGLE2SHORT;
use playerstate_iw4::{UserCmd, buttons};
use sim::{ClientId, ClientLifecycle, Snapshot};

pub type Millis = i32;

#[derive(Clone, Debug)]
pub struct BotSenses {
    pub self_id: ClientId,
    pub lifecycle: ClientLifecycle,
    pub origin: [f32; 3],
    pub viewangles: [f32; 3],
    pub weapon: u16,

    pub objective: Option<([f32; 3], bool, bool)>,

    pub others: Vec<(ClientId, [f32; 3])>,
}

impl BotSenses {
    pub fn from_snapshot(snapshot: &Snapshot, self_id: ClientId) -> Self {
        let meta = snapshot.meta.for_client(self_id);
        let lifecycle = meta
            .map(|m| m.lifecycle)
            .unwrap_or(ClientLifecycle::Connecting);
        let ps = snapshot
            .players
            .iter()
            .find(|(id, _)| *id == self_id)
            .map(|(_, ps)| ps);
        let origin = ps.map(|ps| ps.origin).unwrap_or([0.0; 3]);
        let viewangles = ps.map(|ps| ps.viewangles).unwrap_or([0.0; 3]);
        let weapon = ps.map(|ps| ps.weapon as u16).unwrap_or(0);
        let team = meta.map(|m| m.client_state_team).unwrap_or(0);
        let others = snapshot
            .players
            .iter()
            .filter(|(id, _)| *id != self_id)
            .filter(|(id, _)| {
                snapshot.meta.for_client(*id).is_some_and(|m| {
                    m.lifecycle == ClientLifecycle::Alive
                        && (!snapshot.meta.kind.is_team() || m.client_state_team != team)
                })
            })
            .map(|(id, ps)| (*id, ps.origin))
            .collect();
        let objective = snapshot
            .meta
            .objectives
            .flags
            .iter()
            .filter(|f| f.owner as i32 != team)
            .map(|f| (f.origin, f.users.contains(&self_id), false))
            .chain(
                snapshot
                    .meta
                    .objectives
                    .bombs
                    .iter()
                    .filter(|b| {
                        !b.destroyed
                            && (b.planted_at_ms.is_some()
                                != (snapshot.meta.objectives.attackers as i32 == team))
                    })
                    .map(|b| (b.view.origin, b.view.users.contains(&self_id), true)),
            )
            .min_by(|a, b| dist2(origin, a.0).total_cmp(&dist2(origin, b.0)));
        Self {
            self_id,
            lifecycle,
            origin,
            viewangles,
            weapon,
            objective,
            others,
        }
    }
}

pub trait Brain {
    type Senses;
    fn think(&mut self, senses: &Self::Senses, dt: Millis) -> UserCmd;
}

#[derive(Clone, Debug)]
pub struct DumbBrain {
    seed: u64,
    state: u64,
    yaw_deg: f32,
    retarget_ms: i32,
    shoot_ms: i32,
}

impl DumbBrain {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
            yaw_deg: 0.0,
            retarget_ms: 0,
            shoot_ms: 0,
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
}

impl Brain for DumbBrain {
    type Senses = BotSenses;

    fn think(&mut self, senses: &Self::Senses, dt: Millis) -> UserCmd {
        let dt = dt.max(1);
        if senses.lifecycle != ClientLifecycle::Alive {
            return UserCmd {
                server_time: 0,
                weapon: senses.weapon,
                weapon_mapped: senses.weapon,
                ..UserCmd::default()
            };
        }

        if let Some((target, touching, use_button)) = senses.objective {
            let yaw = yaw_toward(senses.origin, target);
            return UserCmd {
                angles: [0, (yaw * ANGLE2SHORT) as i32, 0],
                forwardmove: if touching { 0 } else { 127 },
                buttons: if touching && use_button {
                    buttons::USE
                } else {
                    0
                },
                weapon: senses.weapon,
                weapon_mapped: senses.weapon,
                ..UserCmd::default()
            };
        }
        self.retarget_ms -= dt;
        self.shoot_ms = (self.shoot_ms - dt).max(0);

        let nearest = senses.others.iter().min_by(|(_, a), (_, b)| {
            let da = dist2(senses.origin, *a);
            let db = dist2(senses.origin, *b);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut attack = false;
        if let Some((_, target)) = nearest {
            let desire = yaw_toward(senses.origin, *target);

            if dist2(senses.origin, *target) < 90_000.0 * 90_000.0 {
                self.yaw_deg = desire;
                if self.shoot_ms == 0 && self.next_f32() < 0.35 {
                    attack = true;
                    self.shoot_ms = 200 + (self.next_u32() % 400) as i32;
                }
            } else if self.retarget_ms <= 0 {
                self.yaw_deg = desire + (self.next_f32() - 0.5) * 40.0;
                self.retarget_ms = 400 + (self.next_u32() % 900) as i32;
            }
        } else if self.retarget_ms <= 0 {
            self.yaw_deg = self.next_f32() * 360.0;
            self.retarget_ms = 500 + (self.next_u32() % 1200) as i32;
        }

        let pitch = senses.viewangles[0].clamp(-70.0, 70.0);
        let mut cmd = UserCmd {
            server_time: 0,
            forwardmove: 127,
            rightmove: 0,
            angles: [
                (pitch * ANGLE2SHORT) as i32,
                (self.yaw_deg * ANGLE2SHORT) as i32,
                0,
            ],
            weapon: senses.weapon,
            weapon_mapped: senses.weapon,
            ..UserCmd::default()
        };
        if attack {
            cmd.buttons |= buttons::ATTACK;
        }

        let _ = self.seed;
        cmd
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn yaw_toward(from: [f32; 3], to: [f32; 3]) -> f32 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    dy.atan2(dx).to_degrees()
}
