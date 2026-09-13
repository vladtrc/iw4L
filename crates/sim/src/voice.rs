use crate::frame::FrameWorld;
use crate::identities::MatchRng;
use crate::match_state::{ClientLifecycle, EventAudience};
use crate::score::MATCH_TICK_MS;
use crate::world::{ClientId, Tick};

pub(crate) const VOICE_RNG_TAG: u64 = 0xBC_D1_E5_00_0000_0004;

const SPEAKER_HOLD_TICKS: u32 = 2_000 / MATCH_TICK_MS;

const KILLFIRM_DELAY_MS: u32 = 750;
const KILLFIRM_DELAY_TICKS: u32 = KILLFIRM_DELAY_MS / MATCH_TICK_MS;
const _: () = assert!(KILLFIRM_DELAY_MS % MATCH_TICK_MS == 0);

const SPEAKER_RANGE_DIST_SQ: f32 = 1_000.0 * 1_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BattlechatterSpeaker {
    pub client: ClientId,
    pub axis: bool,
    pub expire_tick: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DelayedBattlechatter {
    pub due_tick: u32,
    pub speaker: ClientId,
}

pub(crate) fn voice_rng_from_root(seed: u64) -> MatchRng {
    MatchRng::new(seed ^ VOICE_RNG_TAG)
}

fn random_int_range_1_8(rng: &mut MatchRng) -> u32 {
    let span = (sound_iw4::DEATH_VOICE_MAX_EXCLUSIVE - sound_iw4::DEATH_VOICE_MIN) as usize;
    sound_iw4::DEATH_VOICE_MIN + rng.next_index(span) as u32
}

pub(crate) fn play_death_sound(world: &mut FrameWorld, tick: Tick, victim: ClientId) {
    remove_speaker(world, victim);
    let Some(ps) = world.player(victim).copied() else {
        return;
    };
    let axis = world.pers_team_is_axis(victim);
    let n = random_int_range_1_8(world.voice_rng_mut());
    let alias = death_voice_alias(axis, n);
    push_play_sound(world, tick, EventAudience::All, victim, ps.origin, &alias);
}

fn death_voice_alias(axis: bool, n: u32) -> String {
    format!(
        "generic_death_{}_{n}",
        sound_iw4::death_voice_nationality(axis)
    )
}

pub(crate) fn on_reload_start(world: &mut FrameWorld, tick: Tick, speaker: ClientId) {
    say_local_sound(world, tick, speaker, sound_iw4::STEM_RELOAD);
}

pub(crate) fn on_grenade_fire(world: &mut FrameWorld, tick: Tick, speaker: ClientId, weapon: u32) {
    let Some(stem) = grenade_bc_stem(world.weapon_script_name(weapon)) else {
        return;
    };
    say_local_sound(world, tick, speaker, stem);
}

pub(crate) fn on_begin_firing(world: &mut FrameWorld, tick: Tick, speaker: ClientId, weapon: u32) {
    if world.weapon_script_name(weapon) != "claymore_mp" {
        return;
    }
    say_local_sound(world, tick, speaker, sound_iw4::STEM_CLAYMORE);
}

pub(crate) fn schedule_killfirm(
    world: &mut FrameWorld,
    tick: Tick,
    attacker: ClientId,
    victim: ClientId,
) {
    if !world.bootstrap_ref().kind.is_team() {
        return;
    }
    if attacker == victim {
        return;
    }
    if !world
        .client_meta(attacker)
        .is_some_and(|m| m.lifecycle == ClientLifecycle::Alive)
    {
        return;
    }
    world
        .pending_battlechatter_mut()
        .push(DelayedBattlechatter {
            due_tick: tick.0.saturating_add(KILLFIRM_DELAY_TICKS),
            speaker: attacker,
        });
}

pub(crate) fn tick_delayed(world: &mut FrameWorld, tick: Tick) {
    let due: Vec<ClientId> = {
        let pending = world.pending_battlechatter_mut();
        let mut due = Vec::new();
        pending.retain(|row| {
            if row.due_tick <= tick.0 {
                due.push(row.speaker);
                false
            } else {
                true
            }
        });
        due
    };
    for speaker in due {
        say_local_sound(world, tick, speaker, sound_iw4::STEM_KILLFIRM);
    }
}

fn grenade_bc_stem(weapon_name: &str) -> Option<&'static str> {
    match weapon_name {
        "frag_grenade_mp" => Some(sound_iw4::STEM_FRAG),
        "flash_grenade_mp" => Some(sound_iw4::STEM_FLASH),
        "concussion_grenade_mp" => Some(sound_iw4::STEM_STUN),
        "smoke_grenade_mp" => Some(sound_iw4::STEM_SMOKE),
        "c4_mp" => Some(sound_iw4::STEM_C4),
        _ => None,
    }
}

fn say_local_sound(world: &mut FrameWorld, tick: Tick, speaker: ClientId, stem: &str) {
    prune_speakers(world, tick);
    let Some(meta) = world.client_meta(speaker) else {
        return;
    };
    if meta.lifecycle != ClientLifecycle::Alive {
        return;
    }
    if meta.client_state_team == entity_iw4::TEAM_SPECTATOR {
        return;
    }
    if is_speaker_in_range(world, speaker) {
        return;
    }
    let axis = world.pers_team_is_axis(speaker);
    let Some(prefix) = world.team_voice_prefix(axis).map(str::to_owned) else {
        return;
    };
    let Some(origin) = world.player(speaker).map(|ps| ps.origin) else {
        return;
    };
    let alias = format!("{prefix}{}{stem}", sound_iw4::BATTLECHATTER_INFIX);
    let audience = play_sound_to_team_audience(world, speaker, axis);
    add_speaker(world, tick, speaker, axis);
    push_play_sound(world, tick, audience, speaker, origin, &alias);
}

fn play_sound_to_team_audience(world: &FrameWorld, speaker: ClientId, axis: bool) -> EventAudience {
    let want = if axis {
        entity_iw4::TEAM_AXIS
    } else {
        entity_iw4::TEAM_ALLIES
    };
    let ids: Vec<ClientId> = world
        .client_ids_sorted()
        .into_iter()
        .filter(|&id| id != speaker)
        .filter(|&id| {
            world
                .client_meta(id)
                .is_some_and(|m| m.client_state_team == want)
        })
        .collect();
    EventAudience::Clients(ids)
}

fn is_speaker_in_range(world: &FrameWorld, player: ClientId) -> bool {
    let axis = world.pers_team_is_axis(player);
    let Some(origin) = world.player(player).map(|ps| ps.origin) else {
        return false;
    };
    world.bc_speakers().iter().any(|row| {
        if row.axis != axis {
            return false;
        }
        if row.client == player {
            return true;
        }
        let Some(other) = world.player(row.client) else {
            return false;
        };
        let dx = other.origin[0] - origin[0];
        let dy = other.origin[1] - origin[1];
        let dz = other.origin[2] - origin[2];
        dx * dx + dy * dy + dz * dz < SPEAKER_RANGE_DIST_SQ
    })
}

fn add_speaker(world: &mut FrameWorld, tick: Tick, client: ClientId, axis: bool) {
    world.bc_speakers_mut().push(BattlechatterSpeaker {
        client,
        axis,
        expire_tick: tick.0.saturating_add(SPEAKER_HOLD_TICKS),
    });
}

fn remove_speaker(world: &mut FrameWorld, client: ClientId) {
    world.bc_speakers_mut().retain(|row| row.client != client);
}

fn prune_speakers(world: &mut FrameWorld, tick: Tick) {
    world
        .bc_speakers_mut()
        .retain(|row| row.expire_tick > tick.0);
}

fn push_play_sound(
    world: &mut FrameWorld,
    tick: Tick,
    audience: EventAudience,
    number: ClientId,
    origin: [f32; 3],
    alias: &str,
) {
    let event_parm = i32::from(world.sound_alias_index(alias));
    world.push_entity_event(
        tick,
        audience,
        entity_iw4::EntityEventKind::SOUND_ALIAS,
        crate::EntityEventPayload {
            number: number.0 as i32,
            event_parm,
            origin,
            ..Default::default()
        },
    );
}
