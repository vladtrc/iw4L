use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Spin {
    settings: Option<weapon_iw4::BarrelSpinSettings>,
    manual: bool,
    running: bool,
    rate: f32,
    angle: f32,
    updated_ms: Option<i64>,
}

impl Spin {
    pub(super) fn new(settings: Option<weapon_iw4::BarrelSpinSettings>) -> Self {
        Self {
            settings,
            manual: false,
            running: false,
            rate: 0.,
            angle: 0.,
            updated_ms: None,
        }
    }

    pub(super) fn advance(&mut self, now: i64, firing: bool) {
        // Integrate the previous demand up to this clock before switching it.
        let elapsed = self
            .updated_ms
            .map_or(0., |old| (now - old).max(0) as f32 * 0.001);
        self.updated_ms = Some(now);
        if let Some(settings) = self.settings
            && elapsed > 0.
        {
            let target = f32::from(self.running);
            let duration = if self.running {
                settings.up_seconds
            } else {
                settings.down_seconds
            };
            let (rate, turns) = if duration <= 0. {
                (target, target * elapsed)
            } else {
                let moving = ((target - self.rate).abs() * duration).min(elapsed);
                let rate =
                    (self.rate + (target - self.rate).signum() * moving / duration).clamp(0., 1.);
                let area = (self.rate + rate) * 0.5 * moving + target * (elapsed - moving);
                (rate, area)
            };
            self.rate = rate;
            self.angle = (self.angle + settings.turns_per_second * 360. * turns).rem_euclid(360.);
        }
        self.running = self.manual || firing;
    }

    pub(super) fn at_speed(&self) -> bool {
        self.settings.is_none() || self.rate >= 0.9999
    }

    fn manual(&mut self, now: i64, requested: bool, firing: bool) {
        self.advance(now, firing);
        self.manual = requested;
        self.running = requested || firing;
    }

    pub(super) fn rotation(&self) -> Option<xmodel_runtime::TagRotation> {
        self.settings.map(|_| xmodel_runtime::TagRotation {
            tag: "tag_barrel".into(),
            angles: [0., 0., self.angle],
        })
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Method, "startbarrelspin", start);
    registry.register(Method, "stopbarrelspin", stop);
    registry.register(Method, "getbarrelspinrate", |world, receiver, _| {
        let object = turret_of(world, receiver)?;
        Ok(Value::Float(
            world.resource::<RoundScript>().engine.turrets[&object]
                .spin
                .rate,
        ))
    });
}

fn start(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    command(world, receiver, true)
}
fn stop(world: &mut World, receiver: &Value, _: &[Value]) -> Result<Value, String> {
    command(world, receiver, false)
}
fn command(world: &mut World, receiver: &Value, requested: bool) -> Result<Value, String> {
    let object = turret_of(world, receiver)?;
    let now = super::super::players::now_ms(world);
    let mut runtime = world.resource_mut::<RoundScript>();
    let turret = runtime.engine.turrets.get_mut(&object).unwrap();
    turret
        .spin
        .manual(now, requested, turret.firing || turret.manual_firing);
    Ok(Value::Undefined)
}
