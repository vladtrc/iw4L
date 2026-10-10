use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct PowerGrid {
    generators: Vec<Generator>,
}

#[derive(Clone, Debug)]
struct Generator {
    number: u8,
    origin: [f32; 3],
    progress: f32,
    owner: Option<(ClientId, i32)>,
    active: bool,
}

impl PowerGrid {
    pub(super) fn initialize(&mut self, authored: &[Vec<(String, String)>]) {
        self.generators = authored
            .iter()
            .filter_map(|row| {
                if field(row, "targetname") != "s_generator" {
                    return None;
                }
                let number = field(row, "script_int").parse().ok()?;
                if !(1..=6).contains(&number) {
                    return None;
                }
                Some(Generator {
                    number,
                    origin: point(field(row, "origin"))?,
                    progress: 0.0,
                    owner: None,
                    active: false,
                })
            })
            .collect();
        self.generators.sort_by_key(|generator| generator.number);
        self.generators.dedup_by_key(|generator| generator.number);
    }

    pub(super) fn present(&self) -> bool {
        !self.generators.is_empty()
    }

    pub(super) fn all_active(&self) -> bool {
        self.generators.len() == 6 && self.generators.iter().all(|generator| generator.active)
    }

    pub(super) fn active(&self, number: u8) -> bool {
        self.generators
            .iter()
            .any(|generator| generator.number == number && generator.active)
    }

    pub(super) fn powered(&self, row: &Purchase) -> bool {
        match &row.kind {
            PurchaseKind::Generator(_) => true,
            PurchaseKind::Upgrade => self.all_active(),
            PurchaseKind::Perk(name) => match name.as_str() {
                "revive" => self.active(1),
                "sleight" => self.active(3),
                "juggernog" => self.active(4),
                "staminup" => self.active(5),
                "mulekick" => true,
                _ => false,
            },
            PurchaseKind::Box => self
                .generators
                .iter()
                .min_by(|a, b| {
                    Vec3::from_array(a.origin)
                        .distance_squared(Vec3::from_array(row.origin))
                        .total_cmp(
                            &Vec3::from_array(b.origin)
                                .distance_squared(Vec3::from_array(row.origin)),
                        )
                })
                .is_some_and(|generator| generator.active),
            _ => true,
        }
    }

    pub(super) fn available(&self, number: u8) -> bool {
        self.generators
            .iter()
            .any(|generator| generator.number == number && !generator.active)
    }

    pub(super) fn start(&mut self, world: &mut World, number: u8, client: ClientId) {
        if self
            .generators
            .iter()
            .any(|generator| generator.owner.is_some())
        {
            return;
        }
        let frame = FrameWorld::from_world(world);
        let price = 200 * frame.client_ids_sorted().len() as i32;
        if frame
            .client_meta(client)
            .is_none_or(|meta| meta.score < price)
        {
            return;
        }
        drop(frame);
        let Some(generator) = self
            .generators
            .iter_mut()
            .find(|generator| generator.number == number && !generator.active)
        else {
            return;
        };
        generator.owner = Some((client, price));
        generator.progress = 0.001;
        score(world, client, -price);
        diag::info!(
            Sim,
            "origins generator started number={number} client={} cost={price}",
            client.0
        );
    }

    pub(super) fn advance(&mut self, world: &mut World, players: &[(ClientId, [f32; 3])]) {
        let count = FrameWorld::from_world(world)
            .client_ids_sorted()
            .len()
            .max(1);
        let dt = crate::MATCH_TICK_MS as f32 / 1000.0;
        for generator in &mut self.generators {
            let Some((owner, price)) = generator.owner else {
                continue;
            };
            let inside = |at: [f32; 3]| {
                let delta = Vec3::from_array(at) - Vec3::from_array(generator.origin);
                delta.x * delta.x + delta.y * delta.y < 220.0 * 220.0 && delta.z > -20.0
            };
            let captured = players.iter().filter(|(_, at)| inside(*at)).count();
            let refund = players
                .iter()
                .any(|(client, at)| *client == owner && inside(*at));
            generator.progress = (generator.progress
                + if captured == 0 {
                    -dt / 20.0
                } else if count == 1 {
                    dt / 12.0
                } else {
                    dt / 10.0 * captured as f32 / count as f32
                })
            .clamp(0.0, 1.0);
            if generator.progress >= 1.0 {
                generator.active = true;
                generator.owner = None;
                if refund {
                    score(world, owner, price);
                }
                diag::info!(
                    Sim,
                    "origins generator captured number={} refund={}",
                    generator.number,
                    if refund { price } else { 0 }
                );
            } else if generator.progress == 0.0 {
                generator.owner = None;
                diag::info!(
                    Sim,
                    "origins generator abandoned number={}",
                    generator.number
                );
            }
        }
    }

    pub(super) fn status(&self) -> String {
        let mut text = "GENERATORS:".to_owned();
        for generator in &self.generators {
            text.push_str(&format!(
                " {}{}",
                generator.number,
                if generator.active { "+" } else { "-" }
            ));
            if generator.owner.is_some() {
                text.push_str(&format!(" ({}%)", (generator.progress * 10.0) as u32 * 10));
            }
        }
        text
    }

    pub(super) fn prompt(&self, number: u8, count: usize) -> String {
        let Some(generator) = self
            .generators
            .iter()
            .find(|generator| generator.number == number)
        else {
            return String::new();
        };
        if generator.owner.is_some() {
            format!(
                "Generator {number}: {}% - Stay inside the capture area",
                (generator.progress * 10.0) as u32 * 10
            )
        } else if self
            .generators
            .iter()
            .any(|generator| generator.owner.is_some())
        {
            "Another generator is being captured".to_owned()
        } else {
            format!("USE: Start generator {number} [{} points]", 200 * count)
        }
    }
}
