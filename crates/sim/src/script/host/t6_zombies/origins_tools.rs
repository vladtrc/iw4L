use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct Tools {
    shovels: Vec<Shovel>,
}

#[derive(Clone, Debug)]
struct Shovel {
    origin: [f32; 3],
    angles: [f32; 3],
    object: Option<u64>,
    owner: Option<ClientId>,
    hud: Option<u64>,
}

impl Tools {
    pub(super) fn owned(&self, client: ClientId) -> bool {
        self.shovels
            .iter()
            .any(|shovel| shovel.owner == Some(client))
    }
    pub(super) fn initialize(&mut self, world: &mut World, authored: &[Vec<(String, String)>]) {
        let mut zones = BTreeMap::<String, Vec<([f32; 3], [f32; 3])>>::new();
        for row in authored {
            if field(row, "targetname") != "shovel_location" {
                continue;
            }
            let Some(origin) = point(field(row, "origin")) else {
                continue;
            };
            zones
                .entry(field(row, "script_noteworthy").to_owned())
                .or_default()
                .push((origin, point(field(row, "angles")).unwrap_or([0.0; 3])));
        }
        self.shovels = zones
            .into_values()
            .map(|placements| {
                let index = super::super::natives::math::random(world) as usize % placements.len();
                let (origin, angles) = placements[index];
                Shovel {
                    origin,
                    angles,
                    object: None,
                    owner: None,
                    hud: None,
                }
            })
            .collect();
    }

    pub(super) fn advance(
        &mut self,
        world: &mut World,
        players: &[(ClientId, [f32; 3])],
        digs: &origins_dig::Digs,
    ) {
        let connected = FrameWorld::from_world(world).client_ids_sorted();
        for shovel in &mut self.shovels {
            if shovel
                .owner
                .is_some_and(|owner| !connected.contains(&owner))
            {
                shovel.owner = None;
                if let Some(object) = shovel.hud.take() {
                    super::super::hud::destroy(world, object);
                }
            }
            if let Some(owner) = shovel.owner {
                if shovel.hud.is_none() {
                    shovel.hud = make_hud(world, owner, 400.0, 1.0);
                }
                if let Some(object) = shovel.hud {
                    let slot = world.resource::<Runtime>().hud_slots.get(&object).copied();
                    let mut frame = FrameWorld::from_world(world);
                    let material = frame.hud_material_index(if digs.golden(owner) {
                        "zom_hud_shovel_gold"
                    } else {
                        "zom_hud_craftable_tank_shovel"
                    });
                    if let Some(slot) =
                        slot.and_then(|slot| frame.hud_elem_slots_mut().get_mut(slot))
                    {
                        slot.elem.elem_type = hud_iw4::HE_TYPE_MATERIAL;
                        slot.elem.material_index = i32::from(material);
                        slot.elem.width = 30;
                        slot.elem.height = 30;
                    }
                }
            }
            if shovel.owner.is_some()
                || shovel.object.is_some()
                || !players.iter().any(|(_, at)| {
                    Vec3::from_array(*at).distance_squared(Vec3::from_array(shovel.origin))
                        < 1200.0 * 1200.0
                })
            {
                continue;
            }
            if FrameWorld::from_world(world)
                .model_capability("p6_zm_tm_shovel")
                .flatten()
                .is_none()
                || world.resource::<Runtime>().entities.len()
                    >= super::super::entities::MAX_SCRIPT_ENTITIES
            {
                continue;
            }
            let Ok(presence) = super::super::presence::spawn_presence(world, shovel.origin) else {
                continue;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "origins_shovel") else {
                continue;
            };
            runtime.set_object_field(object, "origin", Value::Vector(shovel.origin));
            runtime.set_object_field(object, "angles", Value::Vector(shovel.angles));
            runtime.set_object_field(object, "model", Value::string("p6_zm_tm_shovel"));
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.presence = Some(presence);
            entity.solid = false;
            entity.contents = 0;
            shovel.object = Some(object);
            diag::info!(
                Sim,
                "origins shovel presented object={object} origin={:?}",
                shovel.origin
            );
        }
    }

    pub(super) fn selected(
        &self,
        world: &mut World,
        client: ClientId,
        origin: [f32; 3],
    ) -> Option<usize> {
        if self
            .shovels
            .iter()
            .any(|shovel| shovel.owner == Some(client))
        {
            return None;
        }
        let frame = FrameWorld::from_world(world);
        let forward = Vec3::from_array(math_iw4::angle_vectors(frame.player(client)?.viewangles).0);
        let eye = Vec3::new(origin[0], origin[1], origin[2] + 50.0);
        self.shovels
            .iter()
            .enumerate()
            .filter(|(_, shovel)| {
                shovel.owner.is_none()
                    && shovel.object.is_some()
                    && Vec3::from_array(origin).distance_squared(Vec3::from_array(shovel.origin))
                        <= 64.0 * 64.0
                    && (Vec3::from_array(shovel.origin) + Vec3::Z * 8.0 - eye)
                        .normalize_or_zero()
                        .dot(forward)
                        >= 0.5
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            [shovel.origin[0], shovel.origin[1], shovel.origin[2] + 8.0],
                            [0.0; 3],
                            [0.0; 3],
                            0x11,
                        )
                        .fraction
                        >= 0.95
            })
            .min_by(|(_, a), (_, b)| {
                Vec3::from_array(a.origin)
                    .distance_squared(Vec3::from_array(origin))
                    .total_cmp(
                        &Vec3::from_array(b.origin).distance_squared(Vec3::from_array(origin)),
                    )
            })
            .map(|(index, _)| index)
    }

    pub(super) fn take(&mut self, world: &mut World, client: ClientId, index: usize) {
        if self
            .shovels
            .iter()
            .any(|shovel| shovel.owner == Some(client))
        {
            return;
        }
        let Some(shovel) = self
            .shovels
            .get_mut(index)
            .filter(|shovel| shovel.owner.is_none())
        else {
            return;
        };
        let Some(object) = shovel.object.take() else {
            return;
        };
        shovel.owner = Some(client);
        world.resource_mut::<Runtime>().delete_entity(object);
        diag::info!(
            Sim,
            "origins shovel acquired client={} location={index}",
            client.0
        );
    }
}
