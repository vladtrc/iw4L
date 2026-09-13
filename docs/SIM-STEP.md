# How `sim::step` is grounded in retail — and how it is not

An analogue of `sim::step` in the IW3 lineage **did** exist, and a fairly
direct one: `G_RunFrame`. The single `TickInput → step → Snapshot` funnel is
ours. Retail has several entrances into one global mutable world.

The IW3-lineage sources read here are a hint about the shape of the
algorithm, not about IW4 offsets.

## World step ≈ `G_RunFrame`

```text
SV_RunFrame()
  → G_RunFrame(svs.time)
       → G_RunFrameForEntity(ent)
            → G_RunMissile / G_RunItem / G_RunCorpse
            → G_RunMover / G_RunClient / G_RunThink
       → ClientEndFrame(...)
```

`SV_RunFrame` calls `G_RunFrame(svs.time)` (`sv_main_mp.cpp`). Inside,
`level.time` / `frametime`, XAnim and scripts are updated; then the entities,
and the clients at the end. The dispatcher on `eType` is literally
`G_RunFrameForEntity` (`g_main_mp.cpp`).

`sim::step(world, tick, …)` is a normalized `G_RunFrame`, not an idea from
scratch.

## A single input → step → snapshot funnel did not exist

The ordinary input path in COD4 is **separate** from `G_RunFrame`:

```text
network usercmd → ClientThink(clientNum)
                    → SV_GetUsercmd(...)
                    → ClientThink_real(ent, &cmd)
```

Only under `g_synchronousClients` does `G_RunClient()` inside `G_RunFrame` call
`ClientThink_real()` exactly once per server frame (`g_active_mp.cpp`). The
dvar's own description:

> Call 'client think' exactly once for each server frame to make smooth demos

This is Quake 3 heritage: separate VM entrypoints `GAME_CLIENT_THINK` →
`ClientThink` and `GAME_RUN_FRAME` → `G_RunFrame`. The Q3 comment: normally
`ClientThink_real` runs several times per server frame; with
`g_synchronousClients`, exactly once (`g_active.c`, `g_public.h`).

The snapshot is a separate stage too: `SV_BuildClientSnapshot` /
`SV_WriteSnapshotToClient` live in `sv_snapshot_mp.cpp`, and are not the return
value of `G_RunFrame`.

```text
retail:                         here:

     usercmd                         human ─┐
        |                            bot   ─┼→ TickInput → sim::step → Snapshot
        v                            replay─┘
  ClientThink_real
        |
        v
  global game state
        ^
        |
SV_RunFrame → G_RunFrame
        |
        v
  global game state
        |
        v
  BuildClientSnapshot
        |
        v
     network
```

Server, prediction, replay and the determinism test are all obliged to go
through one function here (`crates/sim/src/lib.rs`).

## Where the border runs

|             | IW3 lineage                                     | here                     |
| ----------- | ----------------------------------------------- | ------------------------ |
| World step  | `G_RunFrame(levelTime)`                         | `sim::step(...)`         |
| State       | global `level`, `g_entities`, `g_clients`       | an explicit `&mut SimWorld` |
| Time        | written into global state                       | the `tick` / `msec` argument |
| Usercmd     | a separate `ClientThink_real`                   | `TickInput`              |
| Bot input   | a separate game path                            | the same `TickInput`     |
| Replay/demo | special synchronous behavior                    | the same `sim::step`     |
| Snapshot    | built separately after the game step            | the result of `step`     |
| Determinism | not an architectural border                     | held by the funnel       |

`sim::step` is retail-like in itself. What is modern is not the existence of a
step function, but:

> everything capable of changing the authoritative simulation must enter
> through one typed input and leave through one snapshot boundary.

No such strict funnel exists in the IW3 lineage: several entrances into one
global mutable world. `g_synchronousClients` shows that IW themselves had
already run into the consequences of that split, and for demos temporarily
shifted the old model toward what is always in place here.

## IW4

There is no directly reconstructed MW2 tree. The public MW2 projects are
patch DLLs over the retail binary, and the bodies of `G_RunFrame` /
`ClientThink` are not in them (a catalogue of names, not an architecture). By the IW3 lineage, IW4 is expected
to be a development of the same `G_RunFrame` / `ClientThink` pair rather than a
different scheme. This is a hypothesis.

The funnel cheat sheet is [`ENTITIES.md`](ENTITIES.md).
