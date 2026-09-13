use std::collections::HashSet;

use bevy::prelude::*;
use frame::{
    AuthoritySet, GameEnded, GameWin, GlassDestroyed, MatchEndingReason as BusReason,
    MatchEndingSoon, MatchEndingVerySoon, PrematchDone, SpawnedPlayerNotify,
};
use killcam_iw4::log::Log;
use killcam_iw4::task::Millis;
use net::{AuthorityWorld, authority_should_tick};
use sound_iw4::{
    Alias, ClientId, Fleet, INIT, Level, MatchEndingReason as SoundReason, MusicController,
    MusicStep, Notify, Output, PersTeam, Player, PlayerDialog, Recipe, SuspenseMusic, SuspenseStep,
    Team, TeamScores, VOICE_INFIX, Winner, advance_all, group, leader_dialog_on_one,
    leader_dialog_on_one_grouped, play_ffa_game_win, play_spawn_music, suspense_len,
};

#[derive(Resource)]
pub struct ScriptMusicHost {
    objectives: crate::objectives::ObjectiveAudio,
    pub controller: MusicController,
    pub now_ms: Millis,
    suspense: Option<SuspenseMusic>,

    rng_state: u64,
    rng_seeded: bool,
    players: Vec<Player>,
    queues: Vec<PlayerDialog>,
    losing: Vec<ClientId>,
    allies_prefix: Option<String>,
    axis_prefix: Option<String>,
    faction_allies: String,
    faction_axis: String,
    prefixes_bound: bool,

    bound_zone: Option<String>,

    prematch_done_flag: bool,

    pub unhandled_glass_destroyed: u32,
}

impl Default for ScriptMusicHost {
    fn default() -> Self {
        Self {
            objectives: Default::default(),
            controller: MusicController::new(),
            now_ms: 0,
            suspense: None,
            rng_state: 0,
            rng_seeded: false,
            players: Vec::new(),
            queues: Vec::new(),
            losing: Vec::new(),
            allies_prefix: None,
            axis_prefix: None,
            faction_allies: gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned(),
            faction_axis: gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned(),
            prefixes_bound: false,
            bound_zone: None,
            prematch_done_flag: false,
            unhandled_glass_destroyed: 0,
        }
    }
}

pub(crate) fn register_script_music(app: &mut App) {
    frame::register_script_notify(app);
    app.init_resource::<ScriptMusicHost>();

    app.add_systems(
        FixedUpdate,
        (bind_faction_prefixes, drain_level_notifies_to_music)
            .chain()
            .in_set(AuthoritySet::Bookkeeping)
            .after(frame::AuthorityBookkeeping)
            .run_if(authority_should_tick),
    );
}

fn bind_faction_prefixes(
    catalog: Option<Res<assets::MenuCatalog>>,
    bank: Option<Res<crate::SoundBank>>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut host: ResMut<ScriptMusicHost>,
) {
    let Some(identity) = identity.as_ref() else {
        return;
    };
    if identity.zone.is_empty() {
        return;
    }
    if host.prefixes_bound && host.bound_zone.as_deref() == Some(identity.zone.as_str()) {
        return;
    }
    let Some(catalog) = catalog.as_ref() else {
        return;
    };
    let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE) else {
        if catalog.string_table("mp/splashTable.csv").is_none() {
            return;
        }
        apply_arena_charsets(&mut host, catalog, identity, bank.as_deref());
        host.prefixes_bound = true;
        host.bound_zone = Some(identity.zone.clone());
        host.allies_prefix = None;
        host.axis_prefix = None;
        diag::warn!(
            Audio,
            "audio: mp/factionTable.csv is unavailable (typed gap)"
        );
        return;
    };
    apply_arena_charsets(&mut host, catalog, identity, bank.as_deref());
    host.prefixes_bound = true;
    host.bound_zone = Some(identity.zone.clone());
    host.allies_prefix = None;
    host.axis_prefix = None;
    let allies = table.lookup_col(&host.faction_allies, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    let axis = table.lookup_col(&host.faction_axis, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    if !allies.is_empty() {
        host.allies_prefix = Some(allies.to_owned());
    }
    if !axis.is_empty() {
        host.axis_prefix = Some(axis.to_owned());
    }
}

fn apply_arena_charsets(
    host: &mut ScriptMusicHost,
    catalog: &assets::MenuCatalog,
    identity: &frame::LaunchIdentity,
    bank: Option<&crate::SoundBank>,
) {
    let (allies, axis, found) = arena_faction_charsets(catalog, bank, identity);
    host.faction_allies = allies;
    host.faction_axis = axis;
    if !found {
        diag::warn!(
            Audio,
            "audio: map faction charsets are unavailable; retail defaults are in use (typed gap)"
        );
    }
}

fn ffa_level<'a>(losing: &'a [ClientId], highest: Option<ClientId>) -> Level<'a> {
    Level {
        splitscreen: false,
        team_based: false,

        hardcore_mode: false,
        roundlimit: gamemode_iw4::ffa::ROUND_LIMIT,
        rounds_played: 0,
        team_scores: TeamScores::default(),
        highest_scoring_player: highest,
        losing_players: losing,
    }
}

fn drain_level_notifies_to_music(
    mut soon: MessageReader<MatchEndingSoon>,
    mut very_soon: MessageReader<MatchEndingVerySoon>,
    mut ended: MessageReader<GameEnded>,
    mut prematch: MessageReader<PrematchDone>,
    mut game_win: MessageReader<GameWin>,
    mut spawned: MessageReader<SpawnedPlayerNotify>,
    mut glass_destroyed: MessageReader<GlassDestroyed>,
    mut host: ResMut<ScriptMusicHost>,
    mut pending_svc: ResMut<net::PendingSvcSounds>,
    authority: Res<AuthorityWorld>,
    clock: Res<net::AuthorityClock>,
) {
    let now = clock.time_ms as Millis;
    if now < host.now_ms {
        host.queues.clear();
        host.objectives = Default::default();
        host.controller = MusicController::new();
        host.suspense = None;
        host.prematch_done_flag = false;
        host.rng_seeded = false;
    }
    sync_roster_from_world(&mut host, &authority);
    host.now_ms = now;
    seed_host_rng(&mut host, authority.0.root_seed());

    let soon_msgs: Vec<MatchEndingSoon> = soon.read().copied().collect();
    let very_msgs: Vec<MatchEndingVerySoon> = very_soon.read().copied().collect();
    let ended_n = ended.read().count();
    let prematch_n = prematch.read().count();
    let win_msgs: Vec<GameWin> = game_win.read().copied().collect();
    let spawned_msgs: Vec<SpawnedPlayerNotify> = spawned.read().copied().collect();
    host.unhandled_glass_destroyed = host
        .unhandled_glass_destroyed
        .saturating_add(glass_destroyed.read().count() as u32);
    let highest = host.players.first().map(|p| p.client);
    let losing = host.losing.clone();
    let level = ffa_level(&losing, highest);
    let mut out: Log<Output, 32> = Log::new();

    let gametype_line = if !authority.0.objectives.bombs.is_empty() {
        "demolition"
    } else if !authority.0.objectives.flags.is_empty() {
        gamemode_iw4::dom::GAMETYPE_DIALOG_LINE
    } else {
        gamemode_iw4::GAMETYPE_DIALOG_LINE
    };
    let objective_cues = host.objectives.collect(&authority.0.objectives, now as u32);
    let before = host.controller.step();
    let mut controller = host.controller;
    let ScriptMusicHost {
        players,
        queues,
        suspense,
        rng_state,
        prematch_done_flag,
        ..
    } = &mut *host;
    {
        let mut fleet = Fleet {
            players: players.as_slice(),
            queues: queues.as_mut_slice(),
        };
        for _ in 0..ended_n {
            controller.on_notify(now, Notify::GameEnded, &level, &mut fleet, &mut out);
            if let Some(s) = suspense.as_mut() {
                s.on_notify(Notify::GameEnded);
            }
        }
        for _ in 0..prematch_n {
            *prematch_done_flag = true;
            controller.on_notify(now, Notify::PrematchDone, &level, &mut fleet, &mut out);
            fire_intro_for_fleet(now, &level, &mut fleet, &authority.0.objectives, &mut out);
        }
        for msg in spawned_msgs {
            let Some(player) = fleet
                .players
                .iter()
                .find(|p| u32::from(p.client.0) == msg.client)
                .copied()
            else {
                continue;
            };
            let team = match player.pers_team {
                Some(PersTeam::Allies) => Team::Allies,
                Some(PersTeam::Axis) => Team::Axis,
                _ => {
                    diag::warn!(Audio, "audio: spawned player has no pers team (typed gap)");
                    continue;
                }
            };
            play_spawn_music(player.client, team, &mut out);
            leader_dialog_on_one(
                now,
                &level,
                &mut fleet,
                player.client,
                gametype_line,
                &mut out,
            );
            if *prematch_done_flag {
                fire_intro_on_one(
                    now,
                    &level,
                    &mut fleet,
                    player.client,
                    &authority.0.objectives,
                    &mut out,
                );
            }
        }
        for msg in win_msgs {
            let winner = msg
                .winner
                .and_then(|id| u8::try_from(id).ok().map(ClientId));
            controller.on_notify(
                now,
                Notify::GameWin(match winner {
                    Some(c) => Winner::Player(c),
                    None => Winner::Undefined,
                }),
                &level,
                &mut fleet,
                &mut out,
            );
            play_ffa_game_win(winner, &fleet, &mut out);
        }
        for msg in soon_msgs {
            let reason = match msg.reason {
                BusReason::Time => SoundReason::Time,
                BusReason::Score => SoundReason::Score,
            };
            if reason == SoundReason::Score && fleet_missing_pers_team(&fleet) {
                diag::warn!(
                    Audio,
                    "audio: score-ending music has no pers team (typed gap)"
                );
                continue;
            }
            if let Some(s) = suspense.as_mut() {
                s.on_notify(Notify::MatchEndingSoon(reason));
            }
            controller.on_notify(
                now,
                Notify::MatchEndingSoon(reason),
                &level,
                &mut fleet,
                &mut out,
            );
        }
        for _ in very_msgs {
            controller.on_notify(
                now,
                Notify::MatchEndingVerySoon,
                &level,
                &mut fleet,
                &mut out,
            );
        }
        if suspense.is_none()
            && MusicController::starts_suspense(&level)
            && controller.step() == MusicStep::WaitingForSoon
        {
            let wait = suspense_wait_ms(rng_state);
            *suspense = Some(SuspenseMusic::start(now, wait));
            diag::warn!(
                Audio,
                "audio: suspense timing uses the host RNG, not GSC (typed gap)"
            );
        }
        if let Some(s) = suspense.as_mut() {
            if s.step() == SuspenseStep::Waiting && now >= s.wake_at_ms() {
                let n = suspense_len().max(1);
                let track = (next_u32(rng_state) as usize) % n;
                let next_wait = suspense_wait_ms(rng_state);
                s.advance(now, &level, &fleet, track, next_wait, &mut out);
            }
        }
        for (team, cue) in objective_cues {
            let clients: Vec<ClientId> = fleet
                .players
                .iter()
                .filter(|p| {
                    let recipient = match p.pers_team {
                        Some(PersTeam::Allies) => gamemode_iw4::Team::Allies,
                        Some(PersTeam::Axis) => gamemode_iw4::Team::Axis,
                        _ => return false,
                    };
                    team.is_none_or(|t| t == recipient)
                })
                .map(|p| p.client)
                .collect();
            for client in clients {
                match cue {
                    crate::objectives::Cue::Dialog(line) => {
                        leader_dialog_on_one(now, &level, &mut fleet, client, line, &mut out)
                    }
                    crate::objectives::Cue::Sound(alias) => {
                        pending_svc.push_alias_u32(u32::from(client.0), false, alias);
                    }
                }
            }
        }
        advance_all(now, &mut fleet, &mut out);
    }

    if before == MusicStep::WaitingForSoon && controller.step() != MusicStep::WaitingForSoon {
        diag::warn!(
            Audio,
            "audio: g_hardcore is unwired; retail default 0 is in use (typed gap)"
        );
    }
    host.controller = controller;
    let allies = host.allies_prefix.clone();
    let axis = host.axis_prefix.clone();
    flush_outputs(&out, &mut pending_svc, allies.as_deref(), axis.as_deref());
}

fn fire_intro_for_fleet<const N: usize>(
    now: Millis,
    level: &Level,
    fleet: &mut Fleet<'_>,
    objectives: &sim::ObjectiveMatch,
    out: &mut Log<Output, N>,
) {
    let clients: Vec<ClientId> = fleet.players.iter().map(|p| p.client).collect();
    for client in clients {
        fire_intro_on_one(now, level, fleet, client, objectives, out);
    }
}

fn fire_intro_on_one<const N: usize>(
    now: Millis,
    level: &Level,
    fleet: &mut Fleet<'_>,
    client: ClientId,
    objectives: &sim::ObjectiveMatch,
    out: &mut Log<Output, N>,
) {
    let dialog = intro_dialog_key(fleet, client, objectives);
    leader_dialog_on_one_grouped(now, level, fleet, client, dialog, group::INTROBOOST, out);
}

fn intro_dialog_key(
    fleet: &Fleet<'_>,
    client: ClientId,
    objectives: &sim::ObjectiveMatch,
) -> &'static str {
    if !objectives.flags.is_empty() {
        return gamemode_iw4::dom::OBJECTIVE_DIALOG_LINE;
    }
    if !objectives.bombs.is_empty() {
        let attacking = fleet
            .players
            .iter()
            .find(|p| p.client == client)
            .is_some_and(|p| match p.pers_team {
                Some(PersTeam::Allies) => objectives.attackers == gamemode_iw4::Team::Allies,
                Some(PersTeam::Axis) => objectives.attackers == gamemode_iw4::Team::Axis,
                _ => false,
            });
        return if attacking {
            "obj_destroy"
        } else {
            "obj_defend"
        };
    }
    match fleet
        .players
        .iter()
        .find(|p| p.client == client)
        .and_then(|p| p.pers_team)
    {
        Some(PersTeam::Allies) => "offense_obj",
        _ => "defense_obj",
    }
}

fn suspense_wait_ms(state: &mut u64) -> Millis {
    const LO: u32 = 60_000;
    const SPAN: u32 = 60_000;
    let u = next_u32(state);
    (LO + ((u as u64 * SPAN as u64) >> 32) as u32) as Millis
}

fn seed_host_rng(host: &mut ScriptMusicHost, root: u64) {
    if host.rng_seeded {
        return;
    }
    host.rng_state = root ^ 0x9E37_79B9_7F4A_7C15;
    host.rng_seeded = true;
}

fn next_u32(state: &mut u64) -> u32 {
    let mut x = *state;
    if x == 0 {
        x = 0x9E37_79B9_7F4A_7C15;
    }
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    ((x.wrapping_mul(0x2545_F491_4F6C_DD1D)) >> 32) as u32
}

fn fleet_missing_pers_team(fleet: &Fleet<'_>) -> bool {
    fleet.players.iter().any(|p| p.pers_team.is_none())
}

fn sync_roster_from_world(host: &mut ScriptMusicHost, authority: &AuthorityWorld) {
    let mut rows: Vec<(u32, i32)> = authority
        .0
        .clients_scoreboard()
        .into_iter()
        .map(|(id, row)| (id.0, row.score))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut players = Vec::new();
    let mut queues = Vec::new();
    for (id, _) in &rows {
        let Ok(small) = u8::try_from(*id) else {
            diag::warn!(
                Audio,
                "audio: client id {id} does not fit the music controller"
            );
            continue;
        };
        let client = ClientId(small);

        let pers_team = authority.0.gsc_pers_team(*id).map(|t| {
            if t == 0 {
                PersTeam::Allies
            } else {
                PersTeam::Axis
            }
        });
        players.push(Player { client, pers_team });
        if let Some(existing) = host.queues.iter().find(|q| q.client() == client).cloned() {
            queues.push(existing);
        } else {
            queues.push(PlayerDialog::new(client));
        }
    }

    let winner = players.first().map(|p| p.client);
    host.losing = players
        .iter()
        .map(|p| p.client)
        .filter(|c| Some(*c) != winner)
        .collect();
    host.players = players;
    host.queues = queues;
}

fn flush_outputs(
    out: &Log<Output, 32>,
    pending: &mut net::PendingSvcSounds,
    allies_prefix: Option<&str>,
    axis_prefix: Option<&str>,
) {
    for item in out.iter() {
        match item {
            Output::PlayLocalSound { client, alias } => {
                queue_local(pending, client, false, &alias, allies_prefix, axis_prefix);
            }
            Output::StopLocalSound { client, alias } => {
                queue_local(pending, client, true, &alias, allies_prefix, axis_prefix);
            }
            Output::EntitySound { .. } => {
                diag::warn!(
                    Audio,
                    "audio: music host emitted unsupported EntitySound (typed gap)"
                );
            }
        }
    }
}

fn prefix_for<'a>(allies: Option<&'a str>, axis: Option<&'a str>, team: Team) -> Option<&'a str> {
    match team {
        Team::Allies => allies,
        Team::Axis => axis,
    }
}

fn compose_alias(allies: Option<&str>, axis: Option<&str>, alias: &Alias) -> Option<String> {
    match alias {
        Alias::Literal(name) => Some((*name).to_owned()),
        Alias::TeamMusic { team, suffix } => {
            prefix_for(allies, axis, *team).map(|p| format!("{p}{suffix}"))
        }
        Alias::Voice { team, line } => {
            prefix_for(allies, axis, *team).map(|p| format!("{p}{VOICE_INFIX}{line}"))
        }
    }
}

pub(crate) fn voice_prefixes_for_zone(
    catalog: Option<&assets::MenuCatalog>,
    bank: Option<&crate::SoundBank>,
    identity: &frame::LaunchIdentity,
) -> (Option<String>, Option<String>) {
    let Some(catalog) = catalog else {
        return (None, None);
    };
    let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE) else {
        return (None, None);
    };
    let (allies_cs, axis_cs, _) = arena_faction_charsets(catalog, bank, identity);
    let allies = table.lookup_col(&allies_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    let axis = table.lookup_col(&axis_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    (
        (!allies.is_empty()).then(|| allies.to_owned()),
        (!axis.is_empty()).then(|| axis.to_owned()),
    )
}

fn arena_faction_charsets(
    catalog: &assets::MenuCatalog,
    bank: Option<&crate::SoundBank>,
    identity: &frame::LaunchIdentity,
) -> (String, String, bool) {
    let from_ui = catalog.rawfile_text("mp/basemaps.arena").map(str::to_owned);
    let from_bank = bank.and_then(|b| b.0.rawfile_text("mp/basemaps.arena").map(str::to_owned));
    let arena_text = from_ui
        .or(from_bank)
        .or_else(|| assets::read_basemaps_arena(&identity.games_root));
    let row = arena_text
        .as_deref()
        .and_then(|text| assets::arena_charsets(text, &identity.zone));
    match row {
        Some(row) => (
            row.allieschar
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned()),
            row.axischar
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned()),
            true,
        ),
        None => (
            gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned(),
            gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned(),
            false,
        ),
    }
}

pub(crate) fn match_script_alias_names(
    allies_prefix: Option<&str>,
    axis_prefix: Option<&str>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    let mut push = |name: String| {
        if seen.insert(name.clone()) {
            names.push(name);
        }
    };
    for prefix in [allies_prefix, axis_prefix].into_iter().flatten() {
        for line in ["demolition", gamemode_iw4::dom::GAMETYPE_DIALOG_LINE] {
            push(format!("{prefix}{VOICE_INFIX}{line}"));
        }
    }
    for recipe in INIT {
        match recipe {
            Recipe::Music { alias, .. } | Recipe::SuspenseTrack { alias } => {
                if let Some(name) = compose_alias(allies_prefix, axis_prefix, alias) {
                    push(name);
                }
            }
            Recipe::Dialog { line, .. } => {
                for prefix in [allies_prefix, axis_prefix].into_iter().flatten() {
                    push(format!("{prefix}{VOICE_INFIX}{line}"));
                }
            }
            Recipe::Voice { .. } | Recipe::SuspenseInit => {}
        }
    }
    for prefix in [allies_prefix, axis_prefix].into_iter().flatten() {
        push(format!(
            "{prefix}{VOICE_INFIX}{}",
            gamemode_iw4::GAMETYPE_DIALOG_LINE
        ));
    }
    names
}

pub(crate) fn match_voice_alias_names(
    allies_prefix: Option<&str>,
    axis_prefix: Option<&str>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut names = Vec::new();
    let mut push = |name: String| {
        if seen.insert(name.clone()) {
            names.push(name);
        }
    };
    for prefix in [allies_prefix, axis_prefix].into_iter().flatten() {
        for stem in sound_iw4::BATTLECHATTER_STEMS {
            push(format!("{prefix}{}{stem}", sound_iw4::BATTLECHATTER_INFIX));
        }
    }
    for n in sound_iw4::DEATH_VOICE_MIN..sound_iw4::DEATH_VOICE_MAX_EXCLUSIVE {
        push(format!(
            "generic_death_{}_{n}",
            sound_iw4::death_voice_nationality(false)
        ));
        push(format!(
            "generic_death_{}_{n}",
            sound_iw4::death_voice_nationality(true)
        ));
    }
    names
}

fn queue_local(
    pending: &mut net::PendingSvcSounds,
    client: ClientId,
    stop: bool,
    alias: &Alias,
    allies_prefix: Option<&str>,
    axis_prefix: Option<&str>,
) {
    match alias {
        Alias::Literal(name) => {
            pending.push_alias_u32(u32::from(client.0), stop, *name);
        }
        Alias::TeamMusic { .. } | Alias::Voice { .. } => {
            let Some(name) = compose_alias(allies_prefix, axis_prefix, alias) else {
                diag::warn!(
                    Audio,
                    "audio: team music or voice prefix is unavailable (typed gap)"
                );
                return;
            };
            pending.push_alias_u32(u32::from(client.0), stop, name);
        }
    }
}
