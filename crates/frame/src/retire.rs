use bevy::prelude::Resource;

#[derive(Resource, Default)]
pub struct Retiring {
    handed: Vec<Box<dyn std::any::Any + Send + Sync>>,
    teardown_requested: bool,
    jobs: u64,
    values: u64,
    in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    teardown_in_flight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Retiring {
    pub fn hand_over<T: Send + Sync + 'static>(&mut self, value: T) {
        self.handed.push(Box::new(value));
        self.values = self.values.saturating_add(1);
    }

    pub fn mark_teardown(&mut self) {
        self.teardown_requested = true;
    }

    pub fn take_batch(&mut self) -> (Vec<Box<dyn std::any::Any + Send + Sync>>, bool) {
        let batch = std::mem::take(&mut self.handed);
        let teardown = std::mem::take(&mut self.teardown_requested);
        if !batch.is_empty() {
            self.jobs = self.jobs.saturating_add(1);
        }
        (batch, teardown)
    }

    pub fn ship(&self, teardown: bool) -> RetiringInFlight {
        self.in_flight
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if teardown {
            self.teardown_in_flight
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
        RetiringInFlight {
            all: self.in_flight.clone(),
            teardown: teardown.then(|| self.teardown_in_flight.clone()),
        }
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight.load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn busy(&self) -> bool {
        self.teardown_requested || !self.handed.is_empty() || self.in_flight() != 0
    }

    pub fn counts(&self) -> (u64, u64, usize) {
        (self.jobs, self.values, self.handed.len())
    }
}

pub struct RetiringInFlight {
    all: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    teardown: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl RetiringInFlight {
    pub fn settle(self) -> Option<usize> {
        self.all.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        self.teardown
            .map(|counter| counter.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) - 1)
    }
}

pub fn retire_resources(world: &mut bevy::prelude::World, take: impl FnOnce(&mut RetireBatch<'_>)) {
    world.init_resource::<Retiring>();
    world.resource_scope::<Retiring, ()>(|world, mut retiring| {
        let mut batch = RetireBatch {
            world,
            retiring: &mut retiring,
        };
        take(&mut batch);
    });
}

pub struct RetireBatch<'w> {
    world: &'w mut bevy::prelude::World,
    retiring: &'w mut Retiring,
}

impl RetireBatch<'_> {
    pub fn resource<T: bevy::prelude::Resource>(&mut self) -> &mut Self {
        if let Some(value) = self.world.remove_resource::<T>() {
            self.retiring.hand_over(value);
        }
        self
    }

    pub fn reset<
        T: bevy::prelude::Resource<Mutability = bevy::ecs::component::Mutable> + Default,
    >(
        &mut self,
    ) -> &mut Self {
        if let Some(mut value) = self.world.get_resource_mut::<T>() {
            self.retiring.hand_over(std::mem::take(&mut *value));
        }
        self
    }
}
