use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct Digs {
    sites: Vec<Site>,
    weapons: Vec<WeaponDrop>,
    staff_parts: Vec<StaffPartDrop>,
    players: BTreeMap<ClientId, Progress>,
    round: u32,
    ended_round: u32,
    refill: Option<u32>,
    rare: BTreeSet<u8>,
    last_rare: bool,
    powerups: u8,
    blood_spawned: bool,
    grenades: Vec<(u32, ClientId, [f32; 3])>,
}

#[derive(Clone, Debug)]
struct Site {
    origin: [f32; 3],
    angles: [f32; 3],
    active: bool,
    born: u32,
    object: Option<u64>,
}

#[derive(Clone, Debug)]
struct WeaponDrop {
    origin: [f32; 3],
    gun: u32,
    object: u64,
    born: u32,
}

#[derive(Clone, Debug)]
struct StaffPartDrop {
    origin: [f32; 3],
    kind: origins_staff::StaffKind,
    object: u64,
    born: u32,
}

#[derive(Clone, Debug, Default)]
struct Progress {
    dug: u32,
    losing: u8,
}

impl Progress {
    fn golden(&self) -> bool {
        self.dug >= 30
    }

    fn good_chance(&mut self) -> u32 {
        let forced = self.dug == 0 || self.losing == 3;
        if forced {
            self.losing = 0;
        }
        if self.golden() {
            70
        } else if forced {
            100
        } else {
            50
        }
    }

    fn complete(&mut self, bad: bool) -> bool {
        let was_golden = self.golden();
        self.dug = self.dug.saturating_add(1).min(30);
        if bad {
            self.losing += 1;
        }
        !was_golden && self.golden()
    }

    fn rare_weapons(&self) -> &'static [&'static str] {
        if self.golden() {
            &[
                "dsr50_zm",
                "srm1216_zm",
                "claymore_zm",
                "ak74u_zm",
                "ksg_zm",
                "mp40_zm",
                "mp44_zm",
            ]
        } else {
            &["dsr50_zm", "srm1216_zm"]
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Selection {
    Site(usize),
    Weapon(usize),
    StaffPart(usize),
}

fn roll(world: &mut World, count: u32) -> u32 {
    super::super::natives::math::random(world) % count
}

fn model(world: &mut World, name: &str, origin: [f32; 3], angles: [f32; 3]) -> Option<u64> {
    if FrameWorld::from_world(world)
        .model_capability(name)
        .flatten()
        .is_none()
        || world.resource::<Runtime>().entities.len() >= super::super::entities::MAX_SCRIPT_ENTITIES
    {
        return None;
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let object = runtime
        .create_entity(EntityKind::Spawned, "origins_dig")
        .ok()?;
    drop(runtime);
    let Ok(presence) = super::super::presence::spawn_presence(world, origin) else {
        world.resource_mut::<Runtime>().delete_entity(object);
        return None;
    };
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.set_object_field(object, "origin", Value::Vector(origin));
    runtime.set_object_field(object, "angles", Value::Vector(angles));
    runtime.set_object_field(object, "model", Value::string(name));
    let entity = runtime.entities.get_mut(&object).unwrap();
    entity.presence = Some(presence);
    entity.solid = false;
    entity.contents = 0;
    Some(object)
}

impl Digs {
    pub(super) fn golden(&self, client: ClientId) -> bool {
        self.players.get(&client).is_some_and(Progress::golden)
    }

    pub(super) fn initialize(&mut self, authored: &[Vec<(String, String)>]) {
        self.sites = authored
            .iter()
            .filter(|row| field(row, "targetname") == "dig_spot")
            .filter_map(|row| {
                Some(Site {
                    origin: point(field(row, "origin"))?,
                    angles: point(field(row, "angles")).unwrap_or([0.0; 3]),
                    active: field(row, "script_noteworthy") == "initial_spot",
                    born: 0,
                    object: None,
                })
            })
            .collect();
    }

    pub(super) fn advance(
        &mut self,
        world: &mut World,
        state: &mut Survival,
        tick: Tick,
        players: &[(ClientId, [f32; 3])],
    ) {
        if self.sites.is_empty() {
            return;
        }
        if self.round != state.round {
            if self.round == 0 {
                for site in &mut self.sites {
                    site.born = tick.0;
                }
            }
            self.round = state.round;
        }
        if state.remaining == 0 && state.actors.is_empty() && self.ended_round != state.round {
            self.ended_round = state.round;
            self.refill = Some(tick.0 + ticks(2000));
            self.rare.clear();
            self.powerups = 0;
            self.blood_spawned = false;
        }
        if self.refill.is_some_and(|due| tick.0 >= due) {
            self.refill = None;
            let mut indices: Vec<_> = (0..self.sites.len()).collect();
            for end in (1..indices.len()).rev() {
                let at = roll(world, (end + 1) as u32) as usize;
                indices.swap(end, at);
            }
            let limit = match state.weather.current() {
                origins_weather::Precipitation::Snow(_) => 0,
                origins_weather::Precipitation::Rain(_) => {
                    5 + roll(world, players.len().max(1) as u32)
                }
                origins_weather::Precipitation::Clear => {
                    3 + roll(world, players.len().max(1) as u32)
                }
            };
            let mut respawned = 0;
            let mut active = self.sites.iter().filter(|site| site.active).count();
            for index in indices {
                let site = &mut self.sites[index];
                if !site.active && respawned < limit && active <= 15 {
                    site.active = true;
                    site.born = tick.0;
                    respawned += 1;
                    active += 1;
                }
            }
            diag::info!(
                Sim,
                "origins dig sites replenished count={respawned} active={active}"
            );
        }
        for (index, site) in self
            .sites
            .iter_mut()
            .enumerate()
            .filter(|(_, site)| site.active)
        {
            if site.object.is_none()
                && players.iter().any(|(_, at)| {
                    Vec3::from_array(*at).distance_squared(Vec3::from_array(site.origin))
                        < 1200.0 * 1200.0
                })
            {
                site.object = model(world, "p6_zm_tm_dig_mound", site.origin, site.angles);
                if let Some(object) = site.object {
                    diag::info!(
                        Sim,
                        "origins dig mound presented index={index} object={object} origin={:?}",
                        site.origin
                    );
                }
            }
            if let Some(object) = site.object {
                let seconds =
                    tick.0.saturating_sub(site.born) as f32 * crate::MATCH_TICK_MS as f32 / 1000.0;
                if seconds <= 3.0 {
                    let rise = if seconds <= 2.0 {
                        16.0 * seconds
                    } else {
                        let tail = seconds - 2.0;
                        32.0 + 16.0 * tail - 8.0 * tail * tail
                    };
                    let mut at = site.origin;
                    at[2] += rise - 40.0;
                    world.resource_mut::<Runtime>().set_object_field(
                        object,
                        "origin",
                        Value::Vector(at),
                    );
                }
            }
        }
        self.weapons.retain(|drop| {
            let elapsed =
                tick.0.saturating_sub(drop.born) as f32 * crate::MATCH_TICK_MS as f32 / 1000.0;
            if elapsed >= 12.0 {
                world.resource_mut::<Runtime>().delete_entity(drop.object);
                return false;
            }
            let sink = if elapsed < 6.0 {
                40.0 * elapsed * elapsed / 108.0
            } else {
                40.0 * (elapsed - 3.0) / 9.0
            };
            let mut at = drop.origin;
            at[2] -= sink;
            world.resource_mut::<Runtime>().set_object_field(
                drop.object,
                "origin",
                Value::Vector(at),
            );
            true
        });
        self.staff_parts.retain(|drop| {
            let elapsed =
                tick.0.saturating_sub(drop.born) as f32 * crate::MATCH_TICK_MS as f32 / 1000.0;
            if elapsed >= 12.0 {
                world.resource_mut::<Runtime>().delete_entity(drop.object);
                return false;
            }
            let sink = if elapsed < 6.0 {
                40.0 * elapsed * elapsed / 108.0
            } else {
                40.0 * (elapsed - 3.0) / 9.0
            };
            let mut at = drop.origin;
            at[2] -= sink;
            world.resource_mut::<Runtime>().set_object_field(
                drop.object,
                "origin",
                Value::Vector(at),
            );
            true
        });
        for (due, client, at) in std::mem::take(&mut self.grenades) {
            if tick.0 < due {
                self.grenades.push((due, client, at));
                continue;
            }
            let mut frame = FrameWorld::from_world(world);
            if frame.client_meta(client).is_some()
                && let Some(gun) = weapon_id(&frame, "frag_grenade_zm")
            {
                crate::equipment::spawn_script_grenade(
                    &mut frame,
                    client,
                    gun,
                    tick,
                    at,
                    [50.0, 50.0, 300.0],
                    3000,
                );
            }
        }
        let connected = FrameWorld::from_world(world).client_ids_sorted();
        self.players.retain(|client, _| connected.contains(client));
    }

    pub(super) fn selected(
        &self,
        world: &mut World,
        origin: [f32; 3],
        client: ClientId,
        tick: Tick,
    ) -> Option<Selection> {
        let frame = FrameWorld::from_world(world);
        let forward = Vec3::from_array(math_iw4::angle_vectors(frame.player(client)?.viewangles).0);
        let eye = Vec3::from_array(origin) + Vec3::Z * 50.0;
        let visible = |at: [f32; 3]| {
            Vec3::from_array(origin).distance_squared(Vec3::from_array(at)) <= 100.0 * 100.0
                && (Vec3::from_array(at) - eye)
                    .normalize_or_zero()
                    .dot(forward)
                    >= 0.5
                && frame
                    .trace_world(eye.to_array(), at, [0.0; 3], [0.0; 3], 0x11)
                    .fraction
                    >= 0.95
        };
        if let Some(index) = self
            .staff_parts
            .iter()
            .position(|drop| visible(drop.origin))
        {
            return Some(Selection::StaffPart(index));
        }
        if let Some(index) = self.weapons.iter().position(|drop| visible(drop.origin)) {
            return Some(Selection::Weapon(index));
        }
        self.sites
            .iter()
            .enumerate()
            .find(|(_, site)| {
                site.active
                    && site.object.is_some()
                    && tick.0.saturating_sub(site.born) >= ticks(3000)
                    && visible([site.origin[0], site.origin[1], site.origin[2] + 20.0])
            })
            .map(|(index, _)| Selection::Site(index))
    }

    pub(super) fn prompt(&self, selection: Selection, shovel: bool) -> &'static str {
        match selection {
            Selection::Weapon(_) => "USE: Take dug-up weapon",
            Selection::StaffPart(_) => "USE: Take staff part",
            Selection::Site(_) if shovel => "USE: Dig",
            Selection::Site(_) => "Shovel required",
        }
    }

    pub(super) fn dig(
        &mut self,
        world: &mut World,
        state: &mut Survival,
        survivor: &mut Survivor,
        client: ClientId,
        selection: Selection,
        tick: Tick,
    ) {
        if let Selection::StaffPart(index) = selection {
            let Some(drop) = self.staff_parts.get(index) else {
                return;
            };
            let kind = drop.kind;
            let drop = self.staff_parts.remove(index);
            world.resource_mut::<Runtime>().delete_entity(drop.object);
            state.staffs.add_part(client, kind);
            diag::info!(
                Sim,
                "origins staff part acquired client={} kind={}",
                client.0,
                kind.name()
            );
            return;
        }
        if let Selection::Weapon(index) = selection {
            let Some(drop) = self.weapons.get(index) else {
                return;
            };
            let mut frame = FrameWorld::from_world(world);
            let claymore = frame.weapon_script_name(drop.gun) == "claymore_zm";
            let acquired = if claymore {
                let had = frame
                    .player(client)
                    .is_some_and(|ps| ps.weapons.contains(&(drop.gun as i32)));
                if crate::script_player::give_weapon(&mut frame, client, drop.gun, false).is_ok() {
                    if had {
                        crate::script_player::give_max_ammo(&mut frame, client, drop.gun);
                    } else {
                        crate::script_player::set_ammo_stock(&mut frame, client, drop.gun, 2);
                    }
                    if let Some(ps) = frame.player_mut(client) {
                        ps.action_slot_type[3] = 1;
                        ps.action_slot_param[3] = drop.gun as i32;
                    }
                    true
                } else {
                    false
                }
            } else {
                std::mem::drop(frame);
                give_gun(world, survivor, client, drop.gun, None)
            };
            if acquired {
                let drop = self.weapons.remove(index);
                world.resource_mut::<Runtime>().delete_entity(drop.object);
                diag::info!(
                    Sim,
                    "origins dug weapon acquired client={} gun={}",
                    client.0,
                    drop.gun
                );
            }
            return;
        }
        if !state.tools.owned(client) {
            return;
        }
        let Selection::Site(index) = selection else {
            return;
        };
        let Some(site) = self
            .sites
            .get(index)
            .filter(|site| site.active && site.object.is_some())
        else {
            return;
        };
        let mut origin = site.origin;
        origin[2] += 20.0;
        let progress = self.players.entry(client).or_default();
        let golden = progress.golden();
        let good_chance = progress.good_chance();
        let rare_weapons = progress.rare_weapons();
        let bad = roll(world, 100) > good_chance;
        let staff_part_chance = if state.round >= 5 { 15 } else { 0 };
        let staff_crystal_chance = if state.round >= 10 { 20 } else { 0 };
        let success = if bad {
            if roll(world, 2) == 0 {
                let mut frame = FrameWorld::from_world(world);
                let spawned = weapon_id(&frame, "frag_grenade_zm").is_some_and(|gun| {
                    crate::equipment::spawn_script_grenade(
                        &mut frame,
                        client,
                        gun,
                        tick,
                        origin,
                        [0.0, 0.0, 300.0],
                        3000,
                    )
                });
                drop(frame);
                if spawned && roll(world, 4) != 0 && roll(world, 2) == 0 {
                    self.grenades.push((tick.0 + ticks(300), client, origin));
                }
                spawned
            } else {
                spawn_actor_site(
                    world,
                    state,
                    tick,
                    SpawnSite {
                        origin: site.origin,
                        barrier: None,
                        riser: true,
                    },
                )
            }
        } else if roll(world, 100) < staff_part_chance {
            let kinds = [
                origins_staff::StaffKind::Fire,
                origins_staff::StaffKind::Ice,
                origins_staff::StaffKind::Lightning,
                origins_staff::StaffKind::Gas,
            ];
            let kind = kinds[roll(world, kinds.len() as u32) as usize];
            let part_model = match kind {
                origins_staff::StaffKind::Fire => "p6_zm_staff_part_fire",
                origins_staff::StaffKind::Ice => "p6_zm_staff_part_ice",
                origins_staff::StaffKind::Lightning => "p6_zm_staff_part_lightning",
                origins_staff::StaffKind::Gas => "p6_zm_staff_part_gas",
            };
            origin[2] += 40.0;
            let frame = FrameWorld::from_world(world);
            let can_spawn = frame.model_capability(part_model).flatten().is_some();
            drop(frame);
            if can_spawn {
                if let Some(object) = model(world, part_model, origin, [0.0, 0.0, 0.0]) {
                    self.staff_parts.push(StaffPartDrop {
                        origin,
                        kind,
                        object,
                        born: tick.0,
                    });
                    diag::info!(
                        Sim,
                        "origins staff part spawned client={} kind={} object={}",
                        client.0,
                        kind.name(),
                        object
                    );
                    true
                } else {
                    false
                }
            } else {
                state.staffs.add_part(client, kind);
                diag::info!(
                    Sim,
                    "origins staff part dug (no model) client={} kind={}",
                    client.0,
                    kind.name()
                );
                true
            }
        } else if roll(world, 100) < staff_crystal_chance {
            let kinds = [
                origins_staff::StaffKind::Fire,
                origins_staff::StaffKind::Ice,
                origins_staff::StaffKind::Lightning,
                origins_staff::StaffKind::Gas,
            ];
            let kind = kinds[roll(world, kinds.len() as u32) as usize];
            state.staffs.add_crystal(client, kind);
            diag::info!(
                Sim,
                "origins staff crystal dug client={} kind={}",
                client.0,
                kind.name()
            );
            true
        } else if roll(world, 2) == 0 {
            let choices: Vec<_> = [
                (0, powerups::Kind::Nuke),
                (1, powerups::Kind::DoublePoints),
                (2, powerups::Kind::InstaKill),
                (3, powerups::Kind::MaxAmmo),
            ]
            .into_iter()
            .filter(|(key, _)| (*key < 2 || golden) && !self.rare.contains(key))
            .collect();
            let rare = self.powerups.saturating_add(state.powerups.drop_count()) <= 4
                && !self.last_rare
                && !choices.is_empty()
                && roll(world, 100) >= 80;
            let (kind, key) = if rare {
                let (key, kind) = choices[roll(world, choices.len() as u32) as usize];
                (kind, Some(key))
            } else {
                if !self.blood_spawned && roll(world, 100) > 70 {
                    (powerups::Kind::ZombieBlood, None)
                } else {
                    (powerups::Kind::BonusPoints, None)
                }
            };
            let spawned = state.powerups.spawn_dig(world, kind, origin);
            if spawned {
                self.last_rare = key.is_some();
                if let Some(key) = key {
                    self.rare.insert(key);
                    self.powerups += 1;
                }
                if matches!(kind, powerups::Kind::ZombieBlood) {
                    self.blood_spawned = true;
                    self.powerups += 1;
                }
            }
            spawned
        } else {
            let names = if roll(world, 100) < 90 {
                &["ballista_zm", "c96_zm", "870mcs_zm"][..]
            } else {
                rare_weapons
            };
            let name = names[roll(world, names.len() as u32) as usize];
            let frame = FrameWorld::from_world(world);
            let gun = weapon_id(&frame, name);
            let mesh = if name == "claymore_zm" {
                Some("t6_wpn_claymore_world".to_owned())
            } else {
                gun.and_then(|gun| {
                    frame
                        .weapon_world_model(gun)
                        .map(|(mesh, _)| mesh.to_owned())
                })
            };
            let angles = [
                0.0,
                frame.player(client).map_or(0.0, |ps| ps.viewangles[1])
                    + if name == "claymore_zm" { 180.0 } else { 90.0 },
                0.0,
            ];
            if gun.is_none()
                || mesh
                    .as_deref()
                    .is_none_or(|mesh| frame.model_capability(mesh).flatten().is_none())
            {
                diag::warn!(
                    Sim,
                    "origins dug weapon unavailable name={name} weapon={gun:?} model={mesh:?}"
                );
            }
            drop(frame);
            origin[2] += 40.0;
            match gun.zip(mesh).and_then(|(gun, mesh)| {
                model(world, &mesh, origin, angles).map(|object| (gun, object))
            }) {
                Some((gun, object)) => {
                    self.weapons.push(WeaponDrop {
                        origin,
                        gun,
                        object,
                        born: tick.0,
                    });
                    true
                }
                None => false,
            }
        };
        if !success {
            diag::warn!(
                Sim,
                "origins dig reward unavailable client={} site={index}",
                client.0
            );
            return;
        }
        let site = &mut self.sites[index];
        site.active = false;
        world
            .resource_mut::<Runtime>()
            .delete_entity(site.object.take().unwrap());
        let progress = self.players.entry(client).or_default();
        if progress.complete(bad) {
            diag::info!(Sim, "origins golden shovel acquired client={}", client.0);
            let mut frame = FrameWorld::from_world(world);
            let alias_index = frame.sound_alias_index("zmb_squest_golden_anything");
            frame.push_local_sound(crate::PendingLocalSound {
                recipient: client,
                stop: false,
                alias_index,
            });
        }
        powerups::effect(
            world,
            tick,
            "maps/zombie_tomb/fx_tomb_shovel_dig",
            [site.origin[0], site.origin[1], site.origin[2] + 20.0],
        );
        diag::info!(
            Sim,
            "origins site dug client={} site={index} count={} bad={bad}",
            client.0,
            progress.dug
        );
    }
}
