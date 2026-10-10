use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Precipitation {
    #[default]
    Clear,
    Rain(u8),
    Snow(u8),
}

impl Precipitation {
    fn period_ms(self) -> u32 {
        match self {
            Self::Clear => 0,
            Self::Rain(level) => (350 / u32::from(level).max(1)).max(150),
            Self::Snow(level) => 500 / u32::from(level).max(1),
        }
    }

    fn effect(self) -> Option<&'static str> {
        match self {
            Self::Clear => None,
            Self::Rain(_) => Some("maps/zombie_tomb/fx_tomb_player_weather_rain"),
            Self::Snow(_) => Some("maps/zombie_tomb/fx_tomb_player_weather_snow"),
        }
    }
}

fn select(
    round: u32,
    snow_round: u32,
    dry_rounds: u8,
    last_snow: u32,
    last_rain: u32,
    random: impl FnOnce() -> u32,
) -> Precipitation {
    if round == snow_round || round == 10 {
        return Precipitation::Snow(1);
    }
    if (5..=9).contains(&round) {
        return if dry_rounds & (1 << (round - 5)) != 0 {
            Precipitation::Clear
        } else {
            Precipitation::Rain(1)
        };
    }
    let roll = random() % 100;
    if roll < 40 || round.saturating_sub(last_snow) > 3 {
        Precipitation::Snow(1)
    } else if roll < 80 || round.saturating_sub(last_rain) > 4 {
        Precipitation::Rain(1)
    } else {
        Precipitation::Clear
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Weather {
    enabled: bool,
    snow_round: u32,
    dry_rounds: u8,
    last_snow: u32,
    last_rain: u32,
    current: Precipitation,
    emitting: Precipitation,
    after: Option<u32>,
    next_emit: BTreeMap<ClientId, u32>,
}

impl Weather {
    pub(super) fn initialize(&mut self, world: &mut World, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            return;
        }
        self.snow_round = 3 + super::super::natives::math::random(world) % 2;
        for bit in 0..5 {
            if super::super::natives::math::random(world) % 2 != 0 {
                self.dry_rounds |= 1 << bit;
            }
        }
    }

    pub(super) fn current(&self) -> Precipitation {
        self.current
    }

    pub(super) fn end_round(&mut self, world: &mut World, round: u32, tick: Tick) {
        if !self.enabled {
            return;
        }
        let selected = select(
            round,
            self.snow_round,
            self.dry_rounds,
            self.last_snow,
            self.last_rain,
            || super::super::natives::math::random(world),
        );
        self.current = match selected {
            Precipitation::Clear => Precipitation::Clear,
            Precipitation::Rain(_) => {
                self.last_rain = round;
                Precipitation::Rain(1 + (super::super::natives::math::random(world) % 4) as u8)
            }
            Precipitation::Snow(_) => {
                self.last_snow = round;
                Precipitation::Snow(1 + (super::super::natives::math::random(world) % 4) as u8)
            }
        };
        self.after = Some(tick.0.saturating_add(ticks(2000)));
        diag::info!(
            Sim,
            "origins weather round={round} precipitation={:?}",
            self.current
        );
    }

    pub(super) fn advance(
        &mut self,
        world: &mut World,
        tick: Tick,
        players: &[(ClientId, [f32; 3])],
    ) {
        self.next_emit
            .retain(|client, _| players.iter().any(|(id, _)| id == client));
        if self.after.is_some_and(|after| tick.0 >= after) {
            self.emitting = self.current;
            self.after = None;
            self.next_emit.clear();
        }
        let Some(effect) = self.emitting.effect() else {
            return;
        };
        let mut frame = FrameWorld::from_world(world);
        let index = frame.effect_name_index(effect);
        for &(client, origin) in players {
            let next = self.next_emit.entry(client).or_insert(tick.0);
            if tick.0 < *next {
                continue;
            }
            *next = tick.0.saturating_add(ticks(self.emitting.period_ms()));
            frame.push_entity_event(
                tick,
                crate::EventAudience::Client(client),
                entity_iw4::EntityEventKind::PLAY_FX,
                crate::EntityEventPayload {
                    number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                    event_parm: i32::from(index),
                    origin,
                    direction: [0.0; 3],
                    ..Default::default()
                },
            );
        }
    }
}
