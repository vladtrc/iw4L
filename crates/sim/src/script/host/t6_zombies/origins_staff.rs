use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum StaffKind {
    Fire,
    Ice,
    Lightning,
    Gas,
}

impl StaffKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Fire => "fire",
            Self::Ice => "ice",
            Self::Lightning => "lightning",
            Self::Gas => "gas",
        }
    }

    pub(super) fn model(self) -> &'static str {
        match self {
            Self::Fire => "p6_zm_staff_fire",
            Self::Ice => "p6_zm_staff_ice",
            Self::Lightning => "p6_zm_staff_lightning",
            Self::Gas => "p6_zm_staff_gas",
        }
    }

    pub(super) fn part_name(self) -> &'static str {
        match self {
            Self::Fire => "fire_staff_part",
            Self::Ice => "ice_staff_part",
            Self::Lightning => "lightning_staff_part",
            Self::Gas => "gas_staff_part",
        }
    }

    pub(super) fn from_part_name(name: &str) -> Option<Self> {
        match name {
            "fire_staff_part" => Some(Self::Fire),
            "ice_staff_part" => Some(Self::Ice),
            "lightning_staff_part" => Some(Self::Lightning),
            "gas_staff_part" => Some(Self::Gas),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Staffs {
    owned: BTreeMap<ClientId, BTreeSet<StaffKind>>,
    upgraded: BTreeMap<ClientId, BTreeSet<StaffKind>>,
    parts: BTreeMap<ClientId, BTreeMap<StaffKind, u8>>,
    crystals: BTreeMap<ClientId, BTreeMap<StaffKind, u8>>,
    stations: Vec<StaffStation>,
    pedestals: Vec<StaffPedestal>,
    placed_staffs: BTreeMap<StaffKind, Option<ClientId>>,
    quest_complete: bool,
    robot_spawned: bool,
    robot_health: i32,
    robot_object: Option<u64>,
    tank_keys_dropped: Vec<TankKey>,
}

#[derive(Clone, Debug)]
struct StaffPedestal {
    kind: StaffKind,
    origin: [f32; 3],
    angles: [f32; 3],
    object: Option<u64>,
    staff_placed: bool,
}

#[derive(Clone, Debug)]
struct TankKey {
    origin: [f32; 3],
    object: u64,
    born: u32,
}

#[derive(Clone, Debug)]
struct StaffStation {
    kind: StaffKind,
    origin: [f32; 3],
    angles: [f32; 3],
    object: Option<u64>,
}

impl Staffs {
    pub(super) fn initialize(&mut self, world: &mut World, authored: &[Vec<(String, String)>]) {
        self.stations = authored
            .iter()
            .filter_map(|row| {
                let targetname = field(row, "targetname");
                if !targetname.starts_with("staff_craft_") {
                    return None;
                }
                let kind = match targetname.strip_prefix("staff_craft_") {
                    Some("fire") => StaffKind::Fire,
                    Some("ice") => StaffKind::Ice,
                    Some("lightning") => StaffKind::Lightning,
                    Some("gas") => StaffKind::Gas,
                    _ => return None,
                };
                let origin = point(field(row, "origin"))?;
                let angles = point(field(row, "angles")).unwrap_or([0.0; 3]);
                Some(StaffStation {
                    kind,
                    origin,
                    angles,
                    object: None,
                })
            })
            .collect();

        self.pedestals = authored
            .iter()
            .filter_map(|row| {
                let targetname = field(row, "targetname");
                if !targetname.starts_with("staff_pedestal_") {
                    return None;
                }
                let kind = match targetname.strip_prefix("staff_pedestal_") {
                    Some("fire") => StaffKind::Fire,
                    Some("ice") => StaffKind::Ice,
                    Some("lightning") => StaffKind::Lightning,
                    Some("gas") => StaffKind::Gas,
                    _ => return None,
                };
                let origin = point(field(row, "origin"))?;
                let angles = point(field(row, "angles")).unwrap_or([0.0; 3]);
                Some(StaffPedestal {
                    kind,
                    origin,
                    angles,
                    object: None,
                    staff_placed: false,
                })
            })
            .collect();

        // Initialize placed_staffs tracking
        for kind in [StaffKind::Fire, StaffKind::Ice, StaffKind::Lightning, StaffKind::Gas] {
            self.placed_staffs.insert(kind, None);
        }
    }

    pub(super) fn advance(&mut self, world: &mut World, players: &[(ClientId, [f32; 3])]) {
        for station in &mut self.stations {
            if station.object.is_some()
                || !players.iter().any(|(_, at)| {
                    Vec3::from_array(*at).distance_squared(Vec3::from_array(station.origin))
                        < 1200.0 * 1200.0
                })
            {
                continue;
            }
            if FrameWorld::from_world(world)
                .model_capability(station.kind.model())
                .flatten()
                .is_none()
                || world.resource::<Runtime>().entities.len()
                    >= super::super::entities::MAX_SCRIPT_ENTITIES
            {
                continue;
            }
            let Ok(presence) = super::super::presence::spawn_presence(world, station.origin) else {
                continue;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "origins_staff_station")
            else {
                continue;
            };
            runtime.set_object_field(object, "origin", Value::Vector(station.origin));
            runtime.set_object_field(object, "angles", Value::Vector(station.angles));
            runtime.set_object_field(object, "model", Value::string(station.kind.model()));
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.presence = Some(presence);
            entity.solid = false;
            entity.contents = 0;
            station.object = Some(object);
            diag::info!(
                Sim,
                "origins staff station presented object={object} kind={} origin={:?}",
                station.kind.name(),
                station.origin
            );
        }
    }

    pub(super) fn selected(
        &self,
        world: &mut World,
        client: ClientId,
        origin: [f32; 3],
    ) -> Option<usize> {
        let frame = FrameWorld::from_world(world);
        let forward = Vec3::from_array(math_iw4::angle_vectors(frame.player(client)?.viewangles).0);
        let eye = Vec3::new(origin[0], origin[1], origin[2] + 50.0);
        self.stations
            .iter()
            .enumerate()
            .filter(|(_, station)| {
                station.object.is_some()
                    && Vec3::from_array(origin).distance_squared(Vec3::from_array(station.origin))
                        <= 96.0 * 96.0
                    && (Vec3::from_array(station.origin) + Vec3::Z * 8.0 - eye)
                        .normalize_or_zero()
                        .dot(forward)
                        >= 0.5
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            [
                                station.origin[0],
                                station.origin[1],
                                station.origin[2] + 8.0,
                            ],
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

    pub(super) fn prompt(&self, index: usize, client: ClientId) -> String {
        let Some(station) = self.stations.get(index) else {
            return String::new();
        };
        let has_staff = self.has(client, station.kind);
        let is_upgraded = self.is_upgraded(client, station.kind);
        let part_count = self.part_count(client, station.kind);
        let crystal_count = self.crystal_count(client, station.kind);
        if is_upgraded {
            format!("{} staff (upgraded)", station.kind.name())
        } else if has_staff {
            if crystal_count >= 3 {
                format!(
                    "USE: Upgrade {} staff ({} crystals)",
                    station.kind.name(),
                    crystal_count
                )
            } else {
                format!(
                    "{} staff (need {} more crystals)",
                    station.kind.name(),
                    3 - crystal_count
                )
            }
        } else if part_count > 0 {
            format!(
                "USE: Craft {} staff ({} part{})",
                station.kind.name(),
                part_count,
                if part_count > 1 { "s" } else { "" }
            )
        } else {
            format!("{} crafting station (need parts)", station.kind.name())
        }
    }

    pub(super) fn craft(&mut self, world: &mut World, client: ClientId, index: usize) -> bool {
        let Some(station) = self.stations.get(index) else {
            return false;
        };
        let kind = station.kind;
        
        // If player already has the staff, try to upgrade it
        if self.has(client, kind) {
            return self.upgrade(client, kind);
        }
        
        // Otherwise, craft a new staff
        if !self.can_craft(client, kind) {
            return false;
        }
        let parts = self
            .parts
            .entry(client)
            .or_default()
            .entry(kind)
            .or_insert(0);
        *parts = parts.saturating_sub(1);
        self.owned.entry(client).or_default().insert(kind);
        diag::info!(
            Sim,
            "origins staff crafted client={} kind={}",
            client.0,
            kind.name()
        );
        true
    }

    pub(super) fn has(&self, client: ClientId, kind: StaffKind) -> bool {
        self.owned
            .get(&client)
            .is_some_and(|staffs| staffs.contains(&kind))
    }

    pub(super) fn part_count(&self, client: ClientId, kind: StaffKind) -> u8 {
        self.parts
            .get(&client)
            .and_then(|parts| parts.get(&kind))
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn add_part(&mut self, client: ClientId, kind: StaffKind) {
        let count = self
            .parts
            .entry(client)
            .or_default()
            .entry(kind)
            .or_insert(0);
        *count = count.saturating_add(1);
        diag::info!(
            Sim,
            "origins staff part added client={} kind={} count={}",
            client.0,
            kind.name(),
            *count
        );
    }

    pub(super) fn add_crystal(&mut self, client: ClientId, kind: StaffKind) {
        let count = self.crystals.entry(client).or_default().entry(kind).or_insert(0);
        *count = count.saturating_add(1);
        diag::info!(
            Sim,
            "origins staff crystal added client={} kind={} count={}",
            client.0,
            kind.name(),
            *count
        );
    }

    pub(super) fn crystal_count(&self, client: ClientId, kind: StaffKind) -> u8 {
        self.crystals
            .get(&client)
            .and_then(|crystals| crystals.get(&kind))
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn can_upgrade(&self, client: ClientId, kind: StaffKind) -> bool {
        self.has(client, kind) && !self.is_upgraded(client, kind) && self.crystal_count(client, kind) >= 3
    }

    pub(super) fn is_upgraded(&self, client: ClientId, kind: StaffKind) -> bool {
        self.upgraded
            .get(&client)
            .is_some_and(|upgraded| upgraded.contains(&kind))
    }

    pub(super) fn upgrade(&mut self, client: ClientId, kind: StaffKind) -> bool {
        if !self.can_upgrade(client, kind) {
            return false;
        }
        let crystals = self.crystals.entry(client).or_default().entry(kind).or_insert(0);
        *crystals = crystals.saturating_sub(3);
        self.upgraded.entry(client).or_default().insert(kind);
        diag::info!(
            Sim,
            "origins staff upgraded client={} kind={}",
            client.0,
            kind.name()
        );
        true
    }

    pub(super) fn can_craft(&self, client: ClientId, kind: StaffKind) -> bool {
        !self.has(client, kind) && self.part_count(client, kind) >= 1
    }

    pub(super) fn give(&mut self, client: ClientId, kind: StaffKind) {
        self.owned.entry(client).or_default().insert(kind);
        diag::info!(
            Sim,
            "origins staff given client={} kind={}",
            client.0,
            kind.name()
        );
    }

    pub(super) fn take(&mut self, client: ClientId, kind: StaffKind) {
        if let Some(staffs) = self.owned.get_mut(&client) {
            staffs.remove(&kind);
        }
        diag::info!(
            Sim,
            "origins staff taken client={} kind={}",
            client.0,
            kind.name()
        );
    }

    pub(super) fn status(&self, client: ClientId) -> String {
        let staffs = self.owned.get(&client).cloned().unwrap_or_default();
        let upgraded = self.upgraded.get(&client).cloned().unwrap_or_default();
        let parts = self.parts.get(&client).cloned().unwrap_or_default();
        let crystals = self.crystals.get(&client).cloned().unwrap_or_default();
        let mut text = "STAFFS:".to_owned();
        for kind in [
            StaffKind::Fire,
            StaffKind::Ice,
            StaffKind::Lightning,
            StaffKind::Gas,
        ] {
            let owned = staffs.contains(&kind);
            let is_upgraded = upgraded.contains(&kind);
            let part_count = parts.get(&kind).copied().unwrap_or(0);
            let crystal_count = crystals.get(&kind).copied().unwrap_or(0);
            text.push_str(&format!(
                " {}{}{}{}",
                kind.name(),
                if is_upgraded {
                    "++"
                } else if owned {
                    "+"
                } else {
                    "-"
                },
                if part_count > 0 {
                    format!("(p{})", part_count)
                } else {
                    String::new()
                },
                if crystal_count > 0 {
                    format!("(c{})", crystal_count)
                } else {
                    String::new()
                }
            ));
        }
        text
    }

    pub(super) fn fire_ability(
        &self,
        world: &mut World,
        client: ClientId,
        kind: StaffKind,
        origin: [f32; 3],
        direction: [f32; 3],
        tick: Tick,
    ) -> bool {
        if !self.has(client, kind) {
            return false;
        }
        let is_upgraded = self.is_upgraded(client, kind);
        let effect_name = match kind {
            StaffKind::Fire => {
                if is_upgraded {
                    "maps/zombie_tomb/fx_tomb_staff_fire_upgraded"
                } else {
                    "maps/zombie_tomb/fx_tomb_staff_fire"
                }
            }
            StaffKind::Ice => {
                if is_upgraded {
                    "maps/zombie_tomb/fx_tomb_staff_ice_upgraded"
                } else {
                    "maps/zombie_tomb/fx_tomb_staff_ice"
                }
            }
            StaffKind::Lightning => {
                if is_upgraded {
                    "maps/zombie_tomb/fx_tomb_staff_lightning_upgraded"
                } else {
                    "maps/zombie_tomb/fx_tomb_staff_lightning"
                }
            }
            StaffKind::Gas => {
                if is_upgraded {
                    "maps/zombie_tomb/fx_tomb_staff_gas_upgraded"
                } else {
                    "maps/zombie_tomb/fx_tomb_staff_gas"
                }
            }
        };
        diag::info!(
            Sim,
            "origins staff ability fired client={} kind={} upgraded={} effect={}",
            client.0,
            kind.name(),
            is_upgraded,
            effect_name
        );
        powerups::effect(world, tick, effect_name, origin);
        true
    }

    pub(super) fn advance_pedestals(&mut self, world: &mut World, players: &[(ClientId, [f32; 3])]) {
        // Present pedestals
        for pedestal in &mut self.pedestals {
            if pedestal.object.is_some()
                || !players.iter().any(|(_, at)| {
                    Vec3::from_array(*at).distance_squared(Vec3::from_array(pedestal.origin))
                        < 1500.0 * 1500.0
                })
            {
                continue;
            }
            if FrameWorld::from_world(world)
                .model_capability("p6_zm_staff_pedestal")
                .flatten()
                .is_none()
                || world.resource::<Runtime>().entities.len()
                    >= super::super::entities::MAX_SCRIPT_ENTITIES
            {
                continue;
            }
            let Ok(presence) = super::super::presence::spawn_presence(world, pedestal.origin) else {
                continue;
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let Ok(object) = runtime.create_entity(EntityKind::Spawned, "origins_staff_pedestal")
            else {
                continue;
            };
            runtime.set_object_field(object, "origin", Value::Vector(pedestal.origin));
            runtime.set_object_field(object, "angles", Value::Vector(pedestal.angles));
            runtime.set_object_field(object, "model", Value::string("p6_zm_staff_pedestal"));
            let entity = runtime.entities.get_mut(&object).unwrap();
            entity.presence = Some(presence);
            entity.solid = false;
            entity.contents = 0;
            pedestal.object = Some(object);
            diag::info!(
                Sim,
                "origins staff pedestal presented object={object} kind={} origin={:?}",
                pedestal.kind.name(),
                pedestal.origin
            );
        }
    }

    pub(super) fn selected_pedestal(
        &self,
        world: &mut World,
        client: ClientId,
        origin: [f32; 3],
    ) -> Option<usize> {
        let frame = FrameWorld::from_world(world);
        let forward = Vec3::from_array(math_iw4::angle_vectors(frame.player(client)?.viewangles).0);
        let eye = Vec3::new(origin[0], origin[1], origin[2] + 50.0);
        self.pedestals
            .iter()
            .enumerate()
            .filter(|(_, pedestal)| {
                pedestal.object.is_some()
                    && !pedestal.staff_placed
                    && Vec3::from_array(origin).distance_squared(Vec3::from_array(pedestal.origin))
                        <= 96.0 * 96.0
                    && (Vec3::from_array(pedestal.origin) + Vec3::Z * 8.0 - eye)
                        .normalize_or_zero()
                        .dot(forward)
                        >= 0.5
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            [
                                pedestal.origin[0],
                                pedestal.origin[1],
                                pedestal.origin[2] + 8.0,
                            ],
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

    pub(super) fn pedestal_prompt(&self, index: usize, client: ClientId) -> String {
        let Some(pedestal) = self.pedestals.get(index) else {
            return String::new();
        };
        if pedestal.staff_placed {
            return format!("{} staff placed", pedestal.kind.name());
        }
        let has_upgraded_staff = self.is_upgraded(client, pedestal.kind);
        if has_upgraded_staff {
            format!("USE: Place {} staff", pedestal.kind.name())
        } else {
            format!("{} pedestal (need upgraded {} staff)", pedestal.kind.name(), pedestal.kind.name())
        }
    }

    pub(super) fn place_staff(
        &mut self,
        world: &mut World,
        client: ClientId,
        pedestal_index: usize,
        tick: Tick,
    ) -> bool {
        let Some(pedestal) = self.pedestals.get(pedestal_index) else {
            return false;
        };
        let kind = pedestal.kind;
        let staff_placed = pedestal.staff_placed;
        let origin = pedestal.origin;
        
        // Check if player has upgraded staff and pedestal is empty
        if !self.is_upgraded(client, kind) || staff_placed {
            return false;
        }
        
        // Place the staff
        let Some(pedestal) = self.pedestals.get_mut(pedestal_index) else {
            return false;
        };
        pedestal.staff_placed = true;
        self.placed_staffs.insert(kind, Some(client));
        
        // Remove staff from player inventory
        if let Some(staffs) = self.owned.get_mut(&client) {
            staffs.remove(&kind);
        }
        if let Some(upgraded) = self.upgraded.get_mut(&client) {
            upgraded.remove(&kind);
        }
        
        // Play placement effect
        let effect_name = match kind {
            StaffKind::Fire => "maps/zombie_tomb/fx_tomb_pedestal_fire",
            StaffKind::Ice => "maps/zombie_tomb/fx_tomb_pedestal_ice",
            StaffKind::Lightning => "maps/zombie_tomb/fx_tomb_pedestal_lightning",
            StaffKind::Gas => "maps/zombie_tomb/fx_tomb_pedestal_gas",
        };
        powerups::effect(world, tick, effect_name, origin);
        
        diag::info!(
            Sim,
            "origins staff placed client={} kind={} pedestal={}",
            client.0,
            kind.name(),
            pedestal_index
        );
        
        // Check if all staffs are placed
        self.check_quest_completion(world, tick);
        
        true
    }

    fn check_quest_completion(&mut self, world: &mut World, tick: Tick) {
        let all_placed = self.pedestals.iter().all(|p| p.staff_placed);
        if all_placed && !self.quest_complete {
            self.quest_complete = true;
            diag::info!(Sim, "origins quest: all staffs placed, spawning robot");
            self.spawn_robot(world, tick);
        }
    }

    fn spawn_robot(&mut self, world: &mut World, tick: Tick) {
        if self.robot_spawned {
            return;
        }
        
        // Robot spawn location (center of pedestal area)
        let center = if !self.pedestals.is_empty() {
            let sum: [f32; 3] = self.pedestals.iter().fold([0.0; 3], |acc, p| {
                [acc[0] + p.origin[0], acc[1] + p.origin[1], acc[2] + p.origin[2]]
            });
            let count = self.pedestals.len() as f32;
            [sum[0] / count, sum[1] / count, sum[2] / count]
        } else {
            [0.0, 0.0, 0.0]
        };
        
        // Spawn robot entity
        if FrameWorld::from_world(world)
            .model_capability("p6_zm_giant_robot")
            .flatten()
            .is_none()
            || world.resource::<Runtime>().entities.len()
                >= super::super::entities::MAX_SCRIPT_ENTITIES
        {
            diag::warn!(Sim, "origins robot: model not available");
            return;
        }
        
        let Ok(presence) = super::super::presence::spawn_presence(world, center) else {
            return;
        };
        
        let mut runtime = world.resource_mut::<Runtime>();
        let Ok(object) = runtime.create_entity(EntityKind::Spawned, "origins_giant_robot") else {
            return;
        };
        
        runtime.set_object_field(object, "origin", Value::Vector(center));
        runtime.set_object_field(object, "angles", Value::Vector([0.0, 0.0, 0.0]));
        runtime.set_object_field(object, "model", Value::string("p6_zm_giant_robot"));
        runtime.set_object_field(object, "health", Value::Int(5000));
        
        let entity = runtime.entities.get_mut(&object).unwrap();
        entity.presence = Some(presence);
        entity.solid = true;
        entity.contents = crate::bullet_collision::CONTENTS_BODY as i32;
        entity.can_damage = true;
        entity.can_radius_damage = true;
        
        self.robot_object = Some(object);
        self.robot_health = 5000;
        self.robot_spawned = true;
        
        // Play spawn effect
        powerups::effect(world, tick, "maps/zombie_tomb/fx_tomb_robot_spawn", center);
        
        diag::info!(Sim, "origins giant robot spawned object={object} health=5000");
    }

    pub(super) fn damage_robot(&mut self, world: &mut World, damage: i32, tick: Tick) -> bool {
        if !self.robot_spawned || self.robot_health <= 0 {
            return false;
        }
        
        self.robot_health -= damage;
        
        if let Some(object) = self.robot_object {
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.set_object_field(object, "health", Value::Int(self.robot_health.max(0)));
        }
        
        if self.robot_health <= 0 {
            diag::info!(Sim, "origins giant robot defeated, dropping tank keys");
            self.drop_tank_keys(world, tick);
        }
        
        true
    }

    fn drop_tank_keys(&mut self, world: &mut World, tick: Tick) {
        // Drop 4 tank keys around the robot
        if let Some(robot_object) = self.robot_object {
            let mut runtime = world.resource_mut::<Runtime>();
            let origin = match runtime.object_field(robot_object, "origin") {
                Value::Vector(v) => v,
                _ => return,
            };
            drop(runtime);
            
            // Spawn 4 tank keys in cardinal directions
            for i in 0..4 {
                let angle = (i as f32) * std::f32::consts::PI * 0.5;
                let key_origin = [
                    origin[0] + 100.0 * angle.cos(),
                    origin[1] + 100.0 * angle.sin(),
                    origin[2] + 20.0,
                ];
                
                if FrameWorld::from_world(world)
                    .model_capability("p6_zm_tank_key")
                    .flatten()
                    .is_none()
                    || world.resource::<Runtime>().entities.len()
                        >= super::super::entities::MAX_SCRIPT_ENTITIES
                {
                    continue;
                }
                
                let Ok(presence) = super::super::presence::spawn_presence(world, key_origin) else {
                    continue;
                };
                
                let mut runtime = world.resource_mut::<Runtime>();
                let Ok(object) = runtime.create_entity(EntityKind::Spawned, "origins_tank_key") else {
                    continue;
                };
                
                runtime.set_object_field(object, "origin", Value::Vector(key_origin));
                runtime.set_object_field(object, "angles", Value::Vector([0.0, 0.0, 0.0]));
                runtime.set_object_field(object, "model", Value::string("p6_zm_tank_key"));
                
                let entity = runtime.entities.get_mut(&object).unwrap();
                entity.presence = Some(presence);
                entity.solid = false;
                entity.contents = 0;
                
                self.tank_keys_dropped.push(TankKey {
                    origin: key_origin,
                    object,
                    born: tick.0,
                });
                
                // Play drop effect
                powerups::effect(world, tick, "maps/zombie_tomb/fx_tomb_tank_key_drop", key_origin);
            }
            
            diag::info!(Sim, "origins tank keys dropped: {}", self.tank_keys_dropped.len());
        }
    }

    pub(super) fn selected_tank_key(
        &self,
        world: &mut World,
        client: ClientId,
        origin: [f32; 3],
    ) -> Option<usize> {
        let frame = FrameWorld::from_world(world);
        let forward = Vec3::from_array(math_iw4::angle_vectors(frame.player(client)?.viewangles).0);
        let eye = Vec3::new(origin[0], origin[1], origin[2] + 50.0);
        
        self.tank_keys_dropped
            .iter()
            .enumerate()
            .filter(|(_, key)| {
                Vec3::from_array(origin).distance_squared(Vec3::from_array(key.origin))
                    <= 64.0 * 64.0
                    && (Vec3::from_array(key.origin) + Vec3::Z * 8.0 - eye)
                        .normalize_or_zero()
                        .dot(forward)
                        >= 0.5
                    && frame
                        .trace_world(
                            [origin[0], origin[1], origin[2] + 50.0],
                            [key.origin[0], key.origin[1], key.origin[2] + 8.0],
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

    pub(super) fn take_tank_key(&mut self, world: &mut World, client: ClientId, index: usize, tick: Tick) -> bool {
        let Some(key) = self.tank_keys_dropped.get(index) else {
            return false;
        };
        
        // Give player the tank key weapon
        let mut frame = FrameWorld::from_world(world);
        let tank_key_weapon = frame.weapon_script_names().iter().position(|n| n == "tank_key_zm");
        
        if let Some(weapon_id) = tank_key_weapon {
            let weapon_id = weapon_id as u32;
            if crate::script_player::give_weapon(&mut frame, client, weapon_id, false).is_ok() {
                diag::info!(Sim, "origins tank key given to client={}", client.0);
                
                // Remove the key entity
                let key = self.tank_keys_dropped.remove(index);
                world.resource_mut::<Runtime>().delete_entity(key.object);
                
                // Play pickup effect
                powerups::effect(world, tick, "maps/zombie_tomb/fx_tomb_tank_key_pickup", key.origin);
                
                return true;
            }
        }
        
        false
    }

    pub(super) fn tank_key_prompt(&self) -> &'static str {
        "USE: Take tank key"
    }

    pub(super) fn is_robot(&self, object: u64) -> bool {
        self.robot_object.is_some_and(|robot_obj| robot_obj == object)
    }
}
