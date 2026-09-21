use crate::frame::FrameWorld;
use crate::{ClientId, ClientLifecycle, MatchPhase, Tick};
use gamemode_iw4::{GameModeKind, Team, UseHoldLoopInput, UseHoldLoopState, UseHoldLoopTick, dd};
use playerstate_iw4::{PM_TYPE_NORMAL_LINKED, buttons};

/// Round-based gametypes run the clock too; the remaining time is the current
/// round's, not the match's.
fn evaluate_round_clock(world: &mut FrameWorld, tick: Tick, remaining_ms: u32) {
    let Some(emit) = gamemode_iw4::match_clock::clock_tick(gamemode_iw4::ClockTick {
        time_remaining_ms: remaining_ms as i32,
        time_limit_minutes: dd::TIME_LIMIT_MS as f32 / 60_000.0,
        half_time: false,
        timer_stopped: false,
    }) else {
        return;
    };
    if emit.countdown_tick {
        crate::score::push_countdown_tick_event(world, tick);
    }
    world.set_pending_match_clock(Some(emit));
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectiveView {
    pub id: u32,
    pub model_source: u32,
    pub label: String,
    pub origin: [f32; 3],
    pub owner: Team,
    pub progress: f32,
    pub capturing: Team,
    pub contested: bool,
    pub users: Vec<ClientId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveHull {
    pub mid: [f32; 3],
    pub half: [f32; 3],
    pub slabs: Vec<([f32; 3], f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BombSite {
    pub view: ObjectiveView,
    pub intact_sources: Vec<u32>,
    pub destroyed_sources: Vec<u32>,
    pub hulls: Vec<ObjectiveHull>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub planted_at_ms: Option<u32>,
    pub planter: Option<ClientId>,
    pub bomb_origin: [f32; 3],
    pub bomb_angles: [f32; 3],
    pub destroyed: bool,
    pub user: Option<ClientId>,
    pub return_weapon: Option<u32>,
    pub hold: UseHoldLoopState,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveMatch {
    pub flags: Vec<ObjectiveView>,
    pub flag_models: [String; 3],
    pub use_weapons: [u32; 2],
    pub restoring: Vec<(ClientId, u32, u32)>,
    pub bombs: Vec<BombSite>,
    pub scores: [i32; 3],
    pub attackers: Team,
    pub round: u32,
    pub round_remaining_ms: u32,
    pub round_end_at_ms: Option<u32>,
    pub winner: Option<Team>,
    pub match_over: bool,
}
impl Default for ObjectiveMatch {
    fn default() -> Self {
        Self {
            flags: Vec::new(),
            flag_models: Default::default(),
            use_weapons: [0; 2],
            restoring: Vec::new(),
            bombs: Vec::new(),
            scores: [0; 3],
            attackers: Team::Allies,
            round: 1,
            round_remaining_ms: dd::TIME_LIMIT_MS,
            round_end_at_ms: None,
            winner: None,
            match_over: false,
        }
    }
}
impl ObjectiveMatch {
    pub(crate) fn constrain_cmds(&self, cmds: &mut [(ClientId, playerstate_iw4::UserCmd)]) {
        for (id, cmd) in cmds {
            // Active use (plant/defuse): the player is linked to the site, so
            // translation is dropped while viewangles, stance and the melee or
            // grenade cancels keep flowing.
            if let Some(site) = self.bombs.iter().find(|b| b.user == Some(*id)) {
                let weapon = self.use_weapons[usize::from(site.planted_at_ms.is_some())];
                cmd.weapon = weapon as u16;
                cmd.weapon_mapped = weapon as u16;
                cmd.forwardmove = 0;
                cmd.rightmove = 0;
                continue;
            }
            // The briefcase is taken back after the unlink, and the freed player
            // keeps full movement: steer the weapon switch, never the movement.
            if let Some((_, weapon, _)) = self.restoring.iter().find(|(c, _, _)| c == id) {
                cmd.weapon = *weapon as u16;
                cmd.weapon_mapped = *weapon as u16;
            }
        }
    }
    pub fn model_visible(&self, source: u32) -> Option<bool> {
        self.bombs.iter().find_map(|site| {
            if site.intact_sources.contains(&source) {
                Some(!site.destroyed)
            } else if site.destroyed_sources.contains(&source) {
                Some(site.destroyed)
            } else {
                None
            }
        })
    }
    pub fn collision_active(&self, owner: crate::AuthorityModelOwner) -> bool {
        owner
            .script_model()
            .and_then(|id| self.model_visible(id.to_wire()))
            .unwrap_or(true)
    }
    pub fn defenders(&self) -> Team {
        if self.attackers == Team::Allies {
            Team::Axis
        } else {
            Team::Allies
        }
    }
}

pub(crate) fn sync_dom(world: &mut FrameWorld) {
    if world.bootstrap_ref().kind != GameModeKind::Domination {
        return;
    }
    let flags = world
        .use_objects()
        .iter()
        .filter(|o| o.callback_kind == gamemode_iw4::UseCallbackKind::DomFlag)
        .map(|o| ObjectiveView {
            id: o.id,
            model_source: world
                .objectives
                .flags
                .iter()
                .find(|f| f.id == o.id)
                .map(|f| f.model_source)
                .expect("installed DOM model binding"),
            label: o
                .script_label
                .as_str()
                .trim_start_matches('_')
                .to_uppercase(),
            origin: o.script_origin,
            owner: match o.owner_team {
                gamemode_iw4::GameObjectTeam::Axis => Team::Axis,
                gamemode_iw4::GameObjectTeam::Allies => Team::Allies,
                _ => Team::Free,
            },
            progress: (o.cur_progress as f32 / o.use_time_ms.max(1) as f32).clamp(0.0, 1.0),
            capturing: o.claim.as_team().unwrap_or(Team::Free),
            contested: o
                .touching
                .iter()
                .any(|(_, t, _)| *t == gamemode_iw4::ProxClaimTeam::Axis)
                && o.touching
                    .iter()
                    .any(|(_, t, _)| *t == gamemode_iw4::ProxClaimTeam::Allies),
            users: o.touching.iter().map(|(id, _, _)| *id).collect(),
        })
        .collect();
    world.objectives.flags = flags;
    let scores = world.team_scores();
    world.objectives.scores = [0, scores.axis, scores.allies];
}

pub(crate) fn touching(origin: [f32; 3], site: &BombSite) -> bool {
    let mid: [f32; 3] = std::array::from_fn(|i| {
        origin[i]
            + (crate::bullet_collision::PLAYER_MAXS[i] + crate::bullet_collision::PLAYER_MINS[i])
                * 0.5
    });
    let half: [f32; 3] = std::array::from_fn(|i| {
        (crate::bullet_collision::PLAYER_MAXS[i] - crate::bullet_collision::PLAYER_MINS[i]) * 0.5
    });
    site.hulls.iter().any(|h| {
        gamemode_iw4::use_bind::capsule_trigger_hull_contact(mid, half, h.mid, h.half, &h.slabs)
    })
}

fn use_weapon_ammo(world: &mut FrameWorld, id: ClientId, weapon: u32, count: i32) {
    let facts = world
        .combat_facts_for(weapon)
        .expect("installed DD weapon facts");
    let ammo = weapon_iw4::bg_ammo_table_key(facts.ammo_index, weapon);
    let clip = weapon_iw4::bg_clip_table_key(facts.clip_index, weapon);
    if let Some(ps) = world.player_mut(id) {
        weapon_iw4::bg_set_ammo_not_in_clip(&mut ps.ammo, ammo, count);
        weapon_iw4::bg_set_clip_for_hand(&mut ps.ammoclip, clip, 0, count);
    }
}

/// The user is linked to the trigger on the USE press and unlinked in the frame
/// the hold completes or is released. The briefcase take-back afterwards runs on
/// a freed player, so `restoring` must never re-freeze movement.
fn link_user(world: &mut FrameWorld, id: ClientId) {
    if let Some(p) = world.player_mut(id).filter(|p| p.pm_type == 0) {
        p.pm_type = PM_TYPE_NORMAL_LINKED;
    }
}

fn unlink_user(world: &mut FrameWorld, id: ClientId) {
    if let Some(p) = world
        .player_mut(id)
        .filter(|p| p.pm_type == PM_TYPE_NORMAL_LINKED)
    {
        p.pm_type = 0;
    }
}

fn sound(world: &mut FrameWorld, tick: Tick, origin: [f32; 3], name: &str) {
    let index = world.sound_alias_index(name);
    world.push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        crate::EntityEventPayload {
            number: 2046,
            event_parm: i32::from(index),
            origin,
            ..Default::default()
        },
    );
}

pub(crate) fn advance(world: &mut FrameWorld, tick: Tick, cmds: &[(u32, u32)]) {
    if !world.publishes_snapshot() {
        return;
    }
    sync_dom(world);
    if world.bootstrap_ref().kind != GameModeKind::Demolition {
        return;
    }
    let now = tick.0.saturating_mul(crate::MATCH_TICK_MS);
    let mut state = std::mem::take(&mut world.objectives);
    state.restoring.retain(|(id, restore, temporary)| {
        let done = world
            .player(*id)
            .is_none_or(|p| p.weapon == *restore || p.health <= 0);
        if done && let Some(p) = world.player_mut(*id) {
            for slot in 0..p.weapons.len() {
                if p.weapons[slot] == *temporary as i32 {
                    p.weapons[slot] = 0;
                    p.weapon_data[slot * 5..slot * 5 + 5].fill(0);
                }
            }
        }
        !done
    });
    if state.match_over {
        world.objectives = state;
        return;
    }
    if let Some(end) = state.round_end_at_ms {
        if now >= end {
            state.round += 1;
            state.attackers = state.defenders();
            state.round_end_at_ms = None;
            state.winner = None;
            state.round_remaining_ms = dd::TIME_LIMIT_MS;
            state.restoring.clear();
            let defenders = state.defenders();
            for site in &mut state.bombs {
                if let Some(id) = site.user.take() {
                    unlink_user(world, id);
                }
                site.planted_at_ms = None;
                site.planter = None;
                site.destroyed = false;
                site.user = None;
                site.return_weapon = None;
                site.hold = UseHoldLoopState::begin();
                site.view.owner = defenders;
                site.view.progress = 0.0;
                site.view.users.clear();
                site.view.capturing = Team::Free;
            }
            world.set_use_start_spawns(true);
            for id in world.client_ids_sorted() {
                if world.client_meta(id).is_some_and(|m| m.loadout.is_some()) {
                    world.client_meta_mut(id).lifecycle = ClientLifecycle::SpawnPending;
                }
            }
            world.set_phase(MatchPhase::Playing);
        }
        world.objectives = state;
        return;
    }
    if world.phase() != MatchPhase::Playing {
        world.objectives = state;
        return;
    }
    world.set_use_start_spawns(false);
    let paused = state.bombs.iter().any(|s| s.planted_at_ms.is_some());
    let defenders = state.defenders();
    let mut busy = Vec::new();
    for site in &mut state.bombs {
        site.view.users.clear();
        if site.destroyed {
            continue;
        }

        if site
            .planted_at_ms
            .is_some_and(|at| now.saturating_sub(at) >= dd::BOMB_FUSE_MS)
        {
            if let (Some(id), Some(restore)) = (site.user, site.return_weapon.take()) {
                state.restoring.push((id, restore, state.use_weapons[1]));
            }
            if let Some(id) = site.user.take() {
                unlink_user(world, id);
            }
            site.planted_at_ms = None;
            site.destroyed = true;
            site.user = None;
            site.view.progress = 0.0;
            sound(world, tick, site.bomb_origin, dd::EXPLODE_SOUND_ALIAS);
            if let Some(attacker) = site.planter {
                let attacker_life = world
                    .client_meta(attacker)
                    .map(|m| m.life_sequence)
                    .unwrap_or_default();
                let owner =
                    crate::ScriptModelId::from_authored_source_ordinal(site.view.model_source);
                let explosion = crate::world_objects::DestructibleExplodeEvent {
                    owner,
                    origin: site.bomb_origin,
                    attacker,
                    attacker_life,
                    source: crate::DamageSource::Radius(owner),
                    explode_range_mp: dd::EXPLODE_RADIUS as u32,
                    explode_damage: (dd::EXPLODE_OUTER as u32, dd::EXPLODE_INNER as u32),
                };
                crate::damage::apply_explosion_blast(
                    world,
                    tick,
                    &crate::damage::ExplosionBlast::from_destructible(&explosion),
                );
                crate::damage::apply_explode_glass_blast(world, tick, &explosion);
            }
            let index = world
                .effect_name_index(dd::PLANTED_BOMB_EXPLODE_FX_PATH.expect("pinned DD effect"));
            let mut origin = site.bomb_origin;
            origin[2] += dd::EXPLODE_FX_Z;
            world.push_entity_event(
                tick,
                crate::EventAudience::All,
                entity_iw4::EntityEventKind::PLAY_FX,
                crate::EntityEventPayload {
                    number: 2046,
                    event_parm: i32::from(index),
                    origin,
                    direction: [0.0, 0.0, 1.0],
                    ..Default::default()
                },
            );
            state.round_remaining_ms = state.round_remaining_ms.saturating_add(dd::ADD_TIME_MS);
            continue;
        }
        if let Some(at) = site.planted_at_ms {
            let elapsed = now.saturating_sub(at);
            let remaining = dd::BOMB_FUSE_MS.saturating_sub(elapsed);
            let interval = if remaining > 10_000 {
                1000
            } else if remaining > 5_000 {
                500
            } else {
                250
            };
            if elapsed % interval == 0 {
                sound(world, tick, site.bomb_origin, dd::SUITCASE_TIMER_ALIAS);
            }
        }
        let planted = site.planted_at_ms.is_some();
        let allowed = if planted { defenders } else { state.attackers };
        let eligible: Vec<ClientId> = world
            .client_ids_sorted()
            .into_iter()
            .filter(|id| {
                world.client_meta(*id).is_some_and(|m| {
                    m.lifecycle == ClientLifecycle::Alive && m.client_state_team == allowed as i32
                }) && world
                    .player(*id)
                    .is_some_and(|p| p.health > 0 && touching(p.origin, site))
            })
            .collect();
        site.view.users = eligible.clone();
        let pressed = |id: ClientId| {
            cmds.iter()
                .find(|(c, _)| *c == id.0)
                .map(|(_, b)| *b)
                .unwrap_or(0)
        };
        if site
            .user
            .is_some_and(|id| !eligible.contains(&id) || pressed(id) & buttons::USE == 0)
        {
            if let (Some(id), Some(restore)) = (site.user, site.return_weapon.take()) {
                state
                    .restoring
                    .push((id, restore, state.use_weapons[usize::from(planted)]));
            }
            if let Some(id) = site.user.take() {
                unlink_user(world, id);
            }
            site.user = None;
            site.hold = UseHoldLoopState::begin();
        }
        if site.user.is_none() {
            site.user = eligible.iter().copied().find(|id| {
                pressed(*id) & buttons::USE != 0
                    && !busy.contains(id)
                    && !state.restoring.iter().any(|(c, _, _)| c == id)
            });
            if let Some(id) = site.user {
                let temporary = state.use_weapons[usize::from(planted)];
                let p = world.player_mut(id).expect("eligible user");
                let restore = p.weapon;
                crate::world::inventory_add_weapon(p, temporary, false);
                if p.weapons.contains(&(temporary as i32)) {
                    site.return_weapon = Some(restore);
                    let origin = p.origin;
                    use_weapon_ammo(world, id, temporary, 0);
                    if planted {
                        sound(world, tick, origin, "mp_bomb_defuse");
                    }
                } else {
                    site.user = None;
                }
            }
        }
        if let Some(id) = site.user {
            link_user(world, id);
            busy.push(id);
            let input = UseHoldLoopInput {
                alive: true,
                touching: true,
                use_pressed: true,
                throwing_grenade: world.use_throwing_grenade(id),
                melee_pressed: pressed(id) & buttons::MELEE_CHARGE != 0,
                weapon_ready: world
                    .player(id)
                    .is_some_and(|p| p.weapon == state.use_weapons[usize::from(planted)]),
                use_time: if planted { dd::DEFUSE_MS } else { dd::PLANT_MS } as i32,
                objective_scaler: world.use_objective_scaler(id),
            };
            match gamemode_iw4::use_hold_loop_tick(site.hold, &input) {
                UseHoldLoopTick::Continue(hold) => site.hold = hold,
                UseHoldLoopTick::Cancelled => {
                    if let Some(restore) = site.return_weapon.take() {
                        state.restoring.push((
                            id,
                            restore,
                            state.use_weapons[usize::from(planted)],
                        ));
                    }
                    unlink_user(world, id);
                    site.user = None;
                    site.hold = UseHoldLoopState::begin();
                }
                UseHoldLoopTick::Completed(_) => {
                    use_weapon_ammo(world, id, state.use_weapons[usize::from(planted)], 1);
                    if planted {
                        site.planted_at_ms = None;
                    } else {
                        site.planted_at_ms = Some(now);
                        site.planter = Some(id);
                        let p = world.player(id).expect("eligible player").origin;
                        let mut start = p;
                        start[2] += 20.0;
                        let mut end = p;
                        end[2] -= 2000.0;
                        let trace = world.trace_world(start, end, [0.0; 3], [0.0; 3], 0x11);
                        site.bomb_origin = std::array::from_fn(|i| {
                            start[i] + (end[i] - start[i]) * trace.fraction
                        });

                        let yaw = world.combat_rng_mut().next_u32() as f64
                            / (u32::MAX as f64 + 1.0)
                            * std::f64::consts::TAU;
                        let forward = [yaw.cos() as f32, yaw.sin() as f32, 0.0];
                        let dot: f32 = (0..3).map(|i| forward[i] * trace.normal[i]).sum();
                        let tangent: [f32; 3] =
                            std::array::from_fn(|i| forward[i] - trace.normal[i] * dot);
                        site.bomb_angles = [
                            -tangent[2].atan2(tangent[0].hypot(tangent[1])).to_degrees(),
                            tangent[1].atan2(tangent[0]).to_degrees(),
                            0.0,
                        ];
                        sound(world, tick, p, "mp_bomb_plant");
                    }
                    world.give_player_objective_score(id, 100);
                    if let Some(restore) = site.return_weapon.take() {
                        state.restoring.push((
                            id,
                            restore,
                            state.use_weapons[usize::from(planted)],
                        ));
                    }
                    unlink_user(world, id);
                    site.user = None;
                    site.hold = UseHoldLoopState::begin();
                }
            }
        }
        site.view.progress = site.hold.cur_progress as f32
            / if planted { dd::DEFUSE_MS } else { dd::PLANT_MS } as f32;
        site.view.capturing = if site.user.is_some() {
            allowed
        } else {
            Team::Free
        };
    }
    if !paused {
        let before = state.round_remaining_ms;
        state.round_remaining_ms = before.saturating_sub(crate::MATCH_TICK_MS);
        if before / 1000 != state.round_remaining_ms / 1000 {
            evaluate_round_clock(world, tick, state.round_remaining_ms);
        }
    }
    let winner = if !state.bombs.is_empty() && state.bombs.iter().all(|s| s.destroyed) {
        Some(state.attackers)
    } else if state.round_remaining_ms == 0
        && !state.bombs.iter().any(|s| s.planted_at_ms.is_some())
    {
        Some(defenders)
    } else {
        None
    };
    if let Some(winner) = winner {
        for site in &mut state.bombs {
            let user = site.user.take();
            let restore = site.return_weapon.take();
            if let Some(id) = user {
                unlink_user(world, id);
                if let Some(restore) = restore {
                    state.restoring.push((
                        id,
                        restore,
                        state.use_weapons[usize::from(site.planted_at_ms.is_some())],
                    ));
                }
            } else if let Some(restore) = restore {
                site.return_weapon = Some(restore);
            }
            site.user = None;
            site.view.users.clear();
            site.view.progress = 0.0;
            site.view.capturing = Team::Free;
        }
        state.scores[winner as usize] += 1;
        state.winner = Some(winner);
        state.match_over =
            state.scores[winner as usize] >= dd::WIN_LIMIT as i32 || state.round >= dd::ROUND_LIMIT;
        state.round_end_at_ms = Some(now.saturating_add(dd::ROUND_END_MS + dd::SWITCH_SIDES_MS));
        world.set_phase(MatchPhase::Intermission);
        if !state.match_over {
            world.set_pending_round_win(Some(winner));
            world.set_pending_round_switch(gamemode_iw4::round_switch_is_halftime(
                state.round,
                dd::ROUND_LIMIT,
                dd::WIN_LIMIT,
            ));
        } else {
            let allies = state.scores[Team::Allies as usize];
            let axis = state.scores[Team::Axis as usize];
            world.set_pending_team_game_win(match allies.cmp(&axis) {
                core::cmp::Ordering::Greater => Some(Team::Allies),
                core::cmp::Ordering::Less => Some(Team::Axis),
                core::cmp::Ordering::Equal => None,
            });
            world.push_event(
                tick,
                crate::EventAudience::All,
                crate::SimEvent::MatchEnded {
                    reason: if state.round_remaining_ms == 0 {
                        crate::MatchEndReason::TimeLimit
                    } else {
                        crate::MatchEndReason::ScoreLimit
                    },
                },
            );
        }
    }
    world.objectives = state;
}
