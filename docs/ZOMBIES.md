# BO2 Zombies survival

Select Black Ops II > Zombies > a map > Start Match. This remains an early
survival runtime using your installed assets, not complete BO2 Zombies.
Direct launch: `IW4L_GAMETYPE=zclassic`, then `scripts/play.ps1 map t6:zm_nuked`.

Host rules live in `sim/script/host/t6_zombies.rs`. NPCs use authored zombie
spawn sites and initial player starts, collision-tested path nodes, staggered
replanning, crowd separation, installed walk/run/melee variants and per-swing authored swipe markers.
Normal waves use player-scaled totals, stepped health and diminishing spawn intervals.
Unsupported vertical transitions are rejected; installed animation states select rise/window clips; wooden windows use root-motion entry, tearing, queued traversal and native boards.
Hold Use on the inside to rebuild; repairs earn bounded points each round.
Authored priced doors remove their linked models/collision and reconnect routes.

Survivors start with M1911 (Origins: Mauser) and 500 points. Use nearby
wall buys for weapons/ammo. Hits/kills earn points. The box has owner-only pickup.
Native drops support Double Points, Insta-Kill, Max Ammo, Nukes and basic Carpenter.
Drop limits/timers, reserve refill and native buff-icon bindings work; advanced presentation and map rewards remain incomplete.
Two guns are retained; Mule Kick permits three and removes the extra gun on downing.
Pack-a-Punch takes an available native upgraded variant through a five-second
hand-in and owner-only pickup. Unsupported variants are refused without charging.
Attachment rerolls and full machine/access behavior remain incomplete.

Jugger-Nog, Speed Cola and Quick Revive have health/reload/revival effects.
Solo Quick Revive costs 500, supports recovery, and allows three purchases.
Basic additions: Double Tap doubles bullet damage to zombies; Stamin-Up grants
unlimited sprint; Deadshot improves hip accuracy; PhD blocks explosion/fall damage.
Their full fire-rate, speed, aim-assist and dive effects remain incomplete.
Electric Cherry, Vulture Aid and Who's Who effects remain Work in Progress.
Native perk-machine models and owned HUD icons load from the installation.
Power gates purchases; generic authored switches work on Die Rise and Buried.
Origins' six generators use paid proximity capture/refunds and local machine power;
all six gate Pack-a-Punch. Capture attackers/recapture, TranZit assembly and Mob afterlife are unfinished.
Co-op supports held-use teammate revival through separate clients; no split-screen.

| Map | Zone |
|---|---|
| TranZit | `t6:zm_transit` |
| Nuketown Zombies | `t6:zm_nuked` |
| Die Rise | `t6:zm_highrise` |
| Mob of the Dead | `t6:zm_prison` |
| Buried | `t6:zm_buried` |
| Origins | `t6:zm_tomb` |

All six have prepared geometry/collision/assets; complete gameplay is not established.
Nuketown verified combat, points, rounds, wall buys, box and a house door; TranZit verified Depot rendering, windows, repairs and solo revival.
Die Rise/Buried verified power; Origins verified six generators, shovels with original ownership HUD art and four initial digs with cash, ground-zombie and shared weapon rewards; weapon expiry and dig power-up rise verified; basic Zombie Blood idling works.
Pack-a-Punch purchases and additional perk effects need broader native verification. Native depth-feathered FX now submit with matching depth inputs; per-effect appearance and timing need verification.
Claymore proximity, explosion, owner pickup, limit and round ammo verified; full damage/lifecycle rules need work. Round weather uses native schedules/cadence and client-only rain/snow FX; rain replenishment verified, original rain/snow clouds render visibly; spawn-room snow masking verified, broader rooms/intensities/co-op need verification. Golden rewards/HUD need a native 30-dig run; helmet, grenades, regional snow/staff parts, transport, buildables, special enemies and quests remain unfinished.
Town/Farm/Bus Depot variants, Grief and Turned need submode/location handling.
