# Singleplayer (campaign): status and plan

Checked against retail files, except the load path.

Layout: campaign code in a file the multiplayer path also uses lives in a child
module `<file>/campaign.rs` (it keeps private access), declared by one line in
the parent, so the MP file carries one-line hooks rather than SP blocks.
`isspawner` and `playsound` keep their MP behaviour; `actors::register` runs
last and replaces them in campaign. Campaign `RoundScript` fields are
`RoundScript::campaign`.

## The `mw2-recompiled` reference

[`mw2-recompiled`](https://github.com/PaulCombal/mw2-recompiled) (a static
recompilation of the Xbox 360 build) is a reference to check our work against,
not a source. Nothing in the engine is copied, translated or ported from it:
every system here is an original Rust implementation written against the
retail data files and scripts, and the code carries no retail addresses,
symbol names or decompiled routines. It is used for three things only:

- **Checking file layouts.** `GameWorldSp`, `PathData`, `pathnode_t`, the node
  KD-tree and `VehicleTrack` field offsets are checked against its asset schema
  (`launcher/zone/defs/assets/GameWorldSp.txt`); what proves them is that every
  retail SP and Special Ops zone walks end to end.
- **Checking builtin signatures.** Argument counts, argument types and return
  shapes of GSC builtins and methods, compared against its dispatch tables when
  a retail script's use leaves one ambiguous. Behaviour is written from what the
  scripts expect.
- **Side-by-side comparison.** Running a level there
  (`MW2_CONSOLE="1:map <level>" ./build/mw2`) to compare entity state, notifies
  and dvars with ours when a campaign script deadlocks or an AI sequence
  stalls. A difference is a bug report against our implementation, fixed in
  our own terms.

Savegame notes (`docs/saves.md` there) inform the checkpoint plan the same way:
format facts to stay compatible with, not code.

## What already holds

- `fastfile_iw4` walks every retail SP and Special Ops zone, `GameWorldSp`
  (`PathData`, `VehicleTrack`, glass) included. Path nodes are decoded
  (`asset_world::path_nodes`) for SP AI; vehicle tracks are not consumed yet.
- `asset_transport::list_sp_maps`: non-MP IW4 zones with a
  `video/<stem>_load.bik` (SP has no `_load` zones and no `localized_common`).
  Resolve stems with `find_zone_file_version`, not `find_zone_file` (it aliases
  `boneyard` to `mp_boneyard`). Special Ops levels layer over a base level zone.
- `Catalog::iw4_sp()`: the generated MP list plus the hand-kept
  `profile/iw4_sp_catalog.rs` (every name `maps/trainer` reaches).
- `iw4l check-scripts <zone>... [<root module>...]`: links the named roots
  (`maps/trainer`), or else each GSC module in the zones (load order, later
  wins) as its own root, against `Catalog::iw4_sp()`, prints parse
  faults and unresolved names, exits 1 if any. Retail ships unloaded scripts
  that do not parse (`animscripts/traverse/stairs_*`).
- Compiler: `childthread` (ends with its spawning thread), vector constants.
- Video: `CinematicInGame[Sync|Loop]`, `StopCinematicInGame` and
  `IsCinematicPlaying` set `EngineState::cinematic`, timed by `.bik` header
  lengths read at install (`LevelData::videos`, only when scripts use them);
  `console::cinematic` plays it on the local client, not yet on remote ones.
  `video/<map>_load.bik`, if any, plays over a map load and holds advancement
  and admission (`frame::LoadCinematicPlaying`); skippable once the map is
  ready. In-world screens (`CinematicInGameLoopResident`) are refused.

## Where MP is hardwired

| what | where |
|---|---|
| common zone is `common_mp` | `assets::match_load::approve_map_load`, `session_load/common_cache.rs`, `common_walks.rs` |
| MP zone alias on bare stems | `asset_transport::discover::find_zone_stem` |
| spawns are `mp_dm_spawn*` | `gamemode_iw4::ffa`, `asset_world::map_entities` |
| Special Ops tile says NOT AVAILABLE | `crates/ui/menus/frontend.json` |

## Slices

1. Discovery, zone walks, compiler gaps — done. `code_post_gfx common <level>`
   over all 45 levels: 341 unknown builtins, 269 shared by every level.
2. Static SP load: an SP request swaps `common_mp` for the SP common set
   (`code_pre_gfx`, `code_post_gfx`, `common`, `patch`, `ui`), loads the
   map, skips GSC, spawns at `info_player_start`. Done when you can walk it.
   Holds on desktop for trainer: the engine spawns the joining player
   (`MatchBootstrap::campaign`), kits come from `body_*` + the level's one
   `*_viewhands`. The level zone's WeaponDefs join the registry where the
   common zone lacks the name. Every campaign level loads (load-only sweep).
   Campaign mode: no gametype (`gametype=campaign`; no `g_gametype`,
   `onlinegame`, `sv_maxclients`), no MP rank tables or player data (the
   install's rank steps are skipped; weapon content gets one rank at 0 XP).
   Level flow (`host/mission.rs`): `missionsuccess`/`changelevel` load the
   named level (empty = menu) through `CampaignLevelCalled` → session
   `request_zone`; `missionfailed` and the player's death reload the level
   after 3 s of real time (retail `g_deathDelay`), resumed from the last checkpoint: `savegame` (or
   `savegamenocommit` + `commitsave`) copies the world at the end of the tick,
   and the restart swaps the fresh level's content into that copy (the
   authority clock continues from its tick). Checkpoints live in the process
   and drop on any other load. Until the restart loads, MISSION FAILED and the
   `ui_deadquote` line (set by scripts, `@` stripped, localized) show, sound fades
   out, the screen blurs to 32 and time slows from 1 to 0.1 over the 3 s, as retail
   does (no retry prompt). SP `setblur` blurs every player's screen. A level change carries the
   `game` array into the next level, as retail does; `_loadout` uses it
   (`game["weaponstates"]`) only for dcemp → dc_whitehouse and af_caves →
   af_chase, every other mission takes its own loadout. A menu start of
   another level drops it.
   The bootstrap kind is `GameModeKind::Campaign` (no teams, no team spawns;
   the player is placed by `MatchBootstrap::campaign`).
3. Frontend — done. `iw4l_select` tints its tiles (Special Ops blue,
   Campaign green, Multiplayer yellow) and displays each mode's soldier art
   (`mw2_main_{co,sp,mp}_image`) properly framed and contained within each
   tile, highlighting the focused mode (`ui_iw4l_focus`). The
   Campaign tile opens `iw4l_campaign`, the retail-style SP main menu: the
   last campaign level started (`campaign.txt` beside `settings.cfg`, written
   on start and on `missionsuccess`/`changelevel`) as its title, Resume Game,
   New Game (first level in story order), Mission Select
   (`iw4l_campaign_levels`: `list_sp_maps` without `so_*`, in story order
   (`console::campaign_menu`), 20 rows, mission names), Options, Main Menu,
   Quit; Stats and Credits are disabled. New Game and a Mission Select row
   open `iw4l_campaign_difficulty` (Recruit … Veteran); the pick is
   `getplayersetting("gameskill")` (`_gameskill` polls it each second), held
   process-wide across level changes and saved as `campaign.txt`'s second line
   (Resume Game restores it).
4. Level GSC — in progress. Campaign runs `maps/<level>::main` plus
   `codescripts/{delete,struct}` against `iw4_sp()`, started once the local
   player begins (SP scripts expect the player before `main`). Builtins not
   built yet are stubs (`natives/sp_pending.rs`, typed defaults, first call
   logged as `campaign builtin X not built yet`). Campaign weapon script names
   are bare (`m4_grunt`); level-zone world gun models join the catalog.
   SP `playsound` notifies after the alias's length (first variant: loaded PCM
   size, streamed WAV header read from the IWDs on first use; mp3 or unknown
   waits 2 s); SP profile fields last one
   session. Trainer runs; its faults left need actors, anim trees, vehicles.
   In-world video (`CINEMATIC_*` textures) once a mission needs it.
5. Script anim trees — in progress. `host/script_anim.rs`: `useanimtree`,
   the `setanim`/`setflaggedanim` family, `clearanim`, anim time/weight,
   `animscripted` with root motion (zone `delta_quat` keys), notetrack
   notifies, `getanimlength` and the clip queries. Trees are the level's
   `animtrees/<tree>.atr`, kept to the `%anim` names scripts use
   (`session::match_apply::script_anim_library`); clips from the xanim catalog.
   `spawndrone` is a script model run through its `aitype/<type>::main`
   (team, weapon, character); campaign links an aitype for each `actor_*`
   spawner. Trainer's props, vehicles and drones animate and notetracks fire;
   scenes on actors wait on 6.
6. AI actors — in progress. Actors are script entities (`EntityKind::Actor`,
   `host/actors.rs`), not bot clients. `dospawn`/`stalingradspawn` spend spawner
   `count`, run the aitype `main` and `animscripts/init::main`; aitype `precache`
   then `spawner` run before level main. Retail animscripts drive them: the engine
   picks `stop`/`move`/`cover_*`/`combat` from goal route, claimed node and
   `enemy`, holds `animcustom`, `animscripted` (via `scripted::init`), `pain` and
   `death`, and switches with `killanimscript` + `end_script`. The motor walks the
   route at the anim tree's blended root-motion speed (`animmode` none) or applies
   root motion, turning per `orientmode`. Path nodes are script objects; goals
   route with the bots' A\* (`bots::nav::campaign`); node objects take the keys of
   the map's `node_*` entity at their origin (`script_linkto`, ...). A route crossing a
   `Begin` node (`target` = an `End` node's `targetname`) runs `animscripts/traverse/<animscript>::main`
   (`getnegotiationstartnode`/`endnode`), then the actor stands at the end node; no such script = plain walk. `usecovernode` claims a `Cover *` node (`claimedby`, lapses when the claimer's `node` moves) and goes to it; `findbestcovernode`/`findshufflecovernode` pick the free cover in the goal area with the best retail score (`findbestcovernode` looks at the 256 covers nearest a search centre: the goal point when the goal radius is at most 716.8, else a 716.8 circle around the actor pulled toward the goal far enough to stay inside it) (`ai_coverScore_*` weights: nearness to the actor, sight of the enemy, the enemy inside the node type's arc, toward or away from the enemy by engagement range, priority spawnflag 0x40, −32 for a node marked dangerous for the actor's team (any running mark), +4 for each other remembered hostile past `engagemindist` inside the node's arc, +8 for allies on a node out of the player's line of fire: a player holding still (25 units, 10°) for 750 ms marks the nodes within 25 units of its aim, up to 150 units out, for 1.5 s). Dangerous nodes follow retail: an actor's death marks the node nearest it (within 192) and that node's links for its team, pain marks its claimed node; a mark lasts 15 s, a second inside it extends it to 60 s. `atdangerousnode`: at the claimed node and it holds a second mark. `iscovervalidagainstenemy`: the enemy is inside the claimed node's arc (prone: not within 800). `setgoalvolume` clamps the goal into an `info_volume` box; `isingoal` tests the goal area. `getnodesinradius(origin, radius, min, [height 512], [type])` returns up to 256 nodes inside the circle and height band, at least `min` away, of one type or any cover kind ("Cover"). Sight picks the
   `enemy` by the retail threat score (seen 1000 / remembered 500, current-enemy
   bonus, `threatbias` and group bias, nearness within 2500, −150 per other
   attacker, 100 × how much better the target shoots at that range (AI: weapon
   graph; player: `ai_playerNear/FarAccuracy` × 5); memory 10 s); `createthreatbiasgroup`/`setthreatbiasgroup`/`setignoremegroup`/`setthreatbias` feed it (`ignoreme`/`ignoreall`/`favoriteenemy` too; `setentitytarget` overrides it until `clearentitytarget`); `shoot` rolls the retail `finalaccuracy` (`accuracy` × target
   `attackeraccuracy` × the weapon's AI-vs-AI/AI-vs-player accuracy graph ×,
   against players, stance and sideways speed), misses wide on purpose, and fires the actor's weapon through the turret
   bullet path, which raises `EV_FIRE_WEAPON` (fire sound, muzzle flash on the
   attached gun's `tag_flash`) for actors, turrets and vehicles alike. Tags of
   attached models answer on the sim side (`gettagorigin`, `muzzle`). Damage
   fills the `damage*` fields. Map `script_vehicle`s are live, spawners
   `vehicle_dospawn()`; ground vehicles roll `info_vehicle_node` paths
   (`resumespeed` returns to the speed the path last set). Level
   `weapon_*` and actor drops are weapon items, level `misc_turret`s turrets of
   their `weaponinfo`; ownerless level rockets count as the player's. Trainer and favela run with no
   script faults. Most levels reach no combat without input; roadkill's opening
   firefight does. `setviewmodel` stores the name; `getviewmodel` returns it.
   Arms bind at load: the level's hands (`setViewmodel` literal from its own
   script, else its `_loadout` branch for `level.script`) are prepared for every
   weapon (`match_walk/campaign.rs`). Runtime switches are ignored; per-hands FPV prep is the gap. `setthreatbiasagainstall`
   and `iscovervalidagainstenemy` are in. `weaponisauto`/`weaponburstcount`/
   `weaponisboltaction` read the weapon's fire type; `lastknownpos`/`lastknowntime`
   answer for the last hostile seen (others: where they are now);
   `canattackenemynode` traces muzzle → the enemy's node (else eye);
   `maymovetopoint`/`maymovefrompointtopoint` need ground within 32 units under
   the point and every 15 units of the way there (unless told not to check the drop) and sweep a body-sized box at
   step height; `maymovetopoint` also refuses a move (30 units past the point)
   across a friendly line of fire. Hostile bullets passing
   within 128 units (a player's miss into the sky counts; an actor's does not) (`ai_eventDistBullet`) of an actor's eye add
   0.15 to `suppressionmeter` (−0.01 per 50 ms) and notify `bulletwhizby`; at a
   Cover Left/Right node a shot from its front starts `suppression`, which ends
   (`suppression_end`) `suppressionduration` after the last one. `issuppressed`
   reads both; `ignoresuppression` zeroes the meter. AI events:
   `addaieventlistener` entities get `ai_event` (type, originator, position) for
   gunshot/silenced_shot (other teams), gunshot_teammate and death (own team),
   explode, bulletwhizby, projectile_impact (other teams; a projectile-type
   weapon's impact, heard along the line from its owner to the impact, within
   45 of the listener's eye height), player footsteps (other teams: sprint steps
   `footstep_sprint`, walk steps `footstep_walk`, run steps/jumps/landings
   `footstep`, or `footstep_walk` crouched or in ADS), within the `ai_eventDist*` dvars (an actor with an
   enemy: `ai_busyEventDist*` for events its enemy did not raise; it also spots
   others only within `fovcosinebusy`). Actors also react on their own (retail):
   a hostile's gunshot or footsteps, or an explosion it set off within 512,
   tells them where it is (as if seen) unless seen in the last 2 s; an ally's
   `gunshot_teammate` teaches the ally's enemy, an ally's `pain` (`ai_eventDistPain`)
   or `death` its `lastattacker`; gunfire, explosions and ally pain set
   `alertlevel` "combat"; explosions notify the actor `explode` (position,
   originator) unless `ignoreexplosionevents` (no investigate-the-spot for far
   explosions, no `new_enemy` event; no projectile ping: retail maps it to `grenade danger`,
   which only battlechatter reads, for frags). An owned missile with explosion
   damage notifies its owner `projectile_impact` (weapon, position, explosion
   radius) when it explodes (AC-130, remote missile); stand-in-owned rockets
   (vehicles, ownerless `magicbullet`) notify nobody.
   Spawn fields take retail values (`fovcosine` 0.01, `suppressionwait`, `walkdist`, …); 5 corpses are kept.
   `setengagementmin/maxdist` set the `engage*` fields (falloff out of order is a
   script error; the cover score's toward/away term reads them). `updateplayersightaccuracy`
   checks four points up a player enemy (eye 10, ¾ ½ ¼ height 30 each) for sight in
   the actor's view; its shots then hit at 0.5 + score/200 of their accuracy (1
   against other enemies). `getanglestolikelyenemypath` faces the first node the
   actor sees on the route from its enemy's last known position (every 500 ms).
   `getdoorpathnode`: the next `Door`/`Door Interior` node on the route;
   `comparenodedirtopathdir`: the route reaches that node heading its way;
   `isindoor`: the nearest node within 192 has spawnflag 0x2000.
   `nearclaimnode[andangle]`: at the claimed node (and within 45° of its yaw).
   `getvalidcoverpeekouts`: sides of a cover node with room to step out (32 units
   sideways, then clear ahead). `startcoverarrival` walks the actor to the
   arrival start, then runs `cover_arrival` on root motion; a new route notifies
   `cover_approach` when `requestarrivalnotify` is set. `disconnectpaths` keeps
   routes out of the entity's box until `connectpaths`; `badplace_cylinder`/
   `badplace_brush` keep boxes out of the routes of actors on their teams
   until they lapse or `badplace_delete` (actors already inside stay). Stances: `allowstand`
   and friends gate players (command buttons) and actors (`isstanceallowed`).
   `enableinvulnerability`/`enabledeathshield` gate damage. SP has no damage callback: the engine takes the player's health itself and notifies `damage` and `death`. The health shield (`enablehealthshield`, on by default) leaves 1 health when a bullet would kill, for the player's `deathinvulnerabletime` ms from the first such hit, re-armed at full health; inside `criticalbulletdamagedist` a bullet kills outright unless that window runs. `makeentitysentient(team)` puts a non-AI entity among the hostiles actors pick from (`freeentitysentient` takes it out); `issentient` answers for it. Path node objects are
   GC roots, and take their map entity's keys (plan position, nearest height)
   typed by `radiant/keys.txt`. Actors read a player's origin from its player
   state (goals, facing, cover choice, `getshootatpos`).
   A teammate's bullet as near leaves a line of fire for 2 s
   (`ai_friendlyFireBlockDuration`, `ismovesuppressed`): a player's always (and
   `bulletwhizby`; one ahead of a moving actor drops its route until the line
   lapses), an AI's only within 128 units of a standing actor.
   Not yet: `setfixednodesafevolume`/`clearfixednodesafevolume`
   (no consumer in the retail dump). AI grenades: `checkgrenadethrow[pos]` plans
   an arc at the enemy or a point, `isgrenadepossafe` keeps allies out of the
   blast, `throwgrenade` explodes the grenade there after its fuse (both need and spend `grenadeammo`); its
   world model flies there on a gravity arc (no bounce). Actors notice live grenades
   (players' and AI ones) within `ai_eventDistGrenadePing` every 250 ms — with
   an enemy only at `grenadeawareness` odds — set `grenade`, get `grenade danger`
   from a hostile's, and inside the blast + 128: from 64 units run
   `grenade_return_throw` (`pickupgrenade` takes it, `throwgrenade` throws it at
   the enemy with the fuse it had, stretched 1 s if under 3 s), else run out of
   the blast on a route (their goal comes back after), else `grenade_cower`; an actor at a claimed cover node whose arc faces the grenade stays in cover. `reacquirestep(dist)` steps `dist` to a random side of the line to the enemy, to a spot inside the goal that sees it, and walks there; `flagenemyunattackable` makes any other enemy win the threat pick for 5 s. `setproneanimnodes`/`enterprone`/`exitprone`/`updateprone` blend the prone nodes in and out; while prone, `updateprone` weights the legs-up/legs-down nodes by the ground slope along the actor's facing (slope / up or down angle, at most 0.99, aim takes the rest; the body itself does not pitch). `bulletspread` picks a point in a cone 8192 out. `setruntopos` walks the actor to a spot inside its goal and notifies `runto_arrived` within 16 units (`goal_changed` on a new spot); `findreacquiredirectpath`/`reacquiremove` walk the route toward the enemy, cut at the goal radius; `checkcoverexitposwithpath` refuses a cover exit that points against the next waypoints. Actors remember where they last saw each hostile for 10 s; remembered hostiles stay in the threat pick, `lastknownpos` answers from it, and `getenemyinfo` writes into it as if the actor just saw the target. `isknownenemyinradius`/`isknownenemyinvolume` (the color system's fixed-node safety) ask that memory. `melee` strikes 64 units from the eye toward the enemy (or the given direction) for the weapon's melee damage and returns what it hit. An actor stops when a friendly player's box 0.1 s ahead blocks its next step; `pushplayer(true)` walks on through (the player is not shoved). `getdroptofloorposition` drops an AI-sized box up to 1024 units; `isinscriptedstate` is true under `animscripted`/`animcustom`; `iswaitingonsound` while a `playsound` notify is pending. `useturret` mans a turret that aims at hostiles it acquires; a
   turret weapon with an `aiScript` (`saw/stand`) runs
   `animscripts/<aiScript>::main` on the gunner, which fires it
   (`shootturret`); one without fires on its own, its shots scattered inside the `setaispread` cone. The gunner is not placed on
   the turret's tag. `startfiring`/`stopfiring` make a turret shoot where it aims at its weapon's fire rate, with or without a gunner.
   Auto and sentry turrets with a side (a team or an owner) also pick hostile actors and sentient
   entities, so allied gunners and player sentries fight AI. `setsuppressiontime` keeps a turret firing
   30 units beside where its target dropped out of sight while the target stays within 64 of that spot.
   `isturretactive`: an actor or player mans it; leaving a turret notifies `turret_deactivate`.
   `hideviewmodel`/`showviewmodel` hide the player's arms (campaign meta, not on the wire).
   Dogs (aitype `type` "dog") run `animscripts/dog/dog_<script>` in place of each
   animscript (`init`, `move`, `stop`, `combat`, `pain`, `death`, `scripted`); they
   fight (`dog_combat`) only inside `meleeattackdist` + 15 of their enemy and, while
   that enemy is inside `pathenemyfightdist` or their goal area, route to it instead
   of the goal. `getaispeciesarray` filters on it. `getcorpsearray` returns the kept
   corpses, `clearallcorpses` removes them. `getgroundposition` sweeps its box down.
7. Player: `notifyoncommand` binds console commands (campaign set in
   `controls/campaign.rs`). Use triggers reach 72 units (retail SP `player_useRadius`). Tagged links ride the tag in campaign, so rig scenes
   place the player; `playerlinktoblend` eases onto the tag over its blend time, `lerpviewangleclamp`
   sets the linked view limits at once. `turret useby(player)` mans a turret
   (aims with the view, fires on attack). `mountvehicle` drives with arcade
   handling over the ground. `magicgrenade[manual]` and AI grenades fly
   under the weapon's projectile bounce physics (`predict_projectile_state`, ownerless: no player is
   credited) and explode wherever they are when the timer runs out; AI flee from the predicted resting spot. `allowfire`/`allowmelee`
   refuse those buttons. Lock-on targets (`target_set`/`target_remove`/`target_istarget`/`target_getarray`, at most 32) feed the Javelin and Stinger scripts; `target_isinrect`/`target_isincircle` project the target onto the player's screen (640 wide at the given fov). Their HUD shaders are not drawn. `linktoblendtotag` links onto the tag (no ease). Objectives follow retail (`objectives/campaign.rs`): text, up to eight
   positions (`objective_additionalposition`), pointer text and entity offset per
   objective; `objective_current` makes the listed ones current (others active).
   Only current objectives show on the compass and as on-screen markers
   (`objective_onscreen`, label = pointer text, "Follow" on an entity, else
   metres). Adding, updating, completing or failing one prints the retail game
   message; the pause menu lists the objectives (`ui_campaign_objectives`).
   Not drawn: the edge arrow and look-at fades. Esc in a campaign level
   (no `g_gametype`) opens `iw4l_campaign_pause`: Resume, Restart Mission (the
   level from its start), Options, Quit to Main Menu (`disconnect`, back to
   `iw4l_campaign` via `ui_iw4l_mode sp`). While it is open `frame::GamePaused`
   stops virtual time (authority fixed steps and client prediction both run on
   it) and holds match-scope sounds; menu sounds still play.
8. Campaign runtime cost. Campaign is local-only, so multiplayer-only work is
   cut while `asset_transport::campaign_zone_set()`: the listen loopback hands
   the local client its snapshot and frame in memory (no delta encode, wire
   serialize, parse and decode per tick; `loopback_live.rs`), and the killcam
   `FrameArchive` is not filled (up to 32 MB of snapshots;
   `authority_bookkeeping`). Already idle in campaign: bot think (no bots),
   UDP hub and master bridge (online lobbies only). Left running: MP HUD
   systems (they hide themselves only while they run; gate once a profile shows
   a cost), player collision history (AI bullets trace the player against it).
   Next: a profile of a heavy scene.
