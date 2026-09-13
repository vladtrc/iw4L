# IW4L

![IW4L running IW4: planting the bomb](docs/screenshots/bomb-plant.png)

IW4L is a standalone, experimental Call of Duty runtime written in Rust on top
of [bevy](https://bevyengine.org/) and [wgpu](https://wgpu.rs/). It reads the
game data of an installation you already own and runs it in its own engine.

Development uses reverse engineering of the original binaries and public
technical references to understand game data and behaviour. IW4L implements its
own runtime architecture; it does not aim to reconstruct the original source
code or to reproduce every behaviour exactly. Compatibility is incomplete, and
behaviour may differ from the original games.

IW4L is not affiliated with, endorsed by or supported by the rights holders of
the original games. **No game assets are contained in this repository or in any
IW4L release.** You supply your own legally obtained installation.

This whole project is written by an LLM.

## Scope

| area | what IW4L promises |
|---|---|
| IW4 / Modern Warfare 2 | the primary target of the runtime |
| IW5 (MW3) / T5 (Black Ops) | experimental: selected maps and weapons |
| IW4L multiplayer | experimental matches between IW4L clients |
| a drop-in replacement for the original games | not claimed |
| a general engine for every older CoD title | not claimed |
| the original network protocols, ABI or modified executables | not a goal, and no compatibility is promised |

Where reproducing a detail exactly would cost disproportionate complexity and
the demo scenarios do not need it, IW4L implements it differently or leaves it
out. That is a scope decision, not a blanket excuse: a desync in IW4L's own
multiplayer is a bug in IW4L, while a difference from the original animation
timing may be an accepted limitation.

## What runs today

* loading IW4 maps, weapons, models, materials, effects and world geometry
* rendering the world, models, materials and effects
* player and entity simulation, animation, sound and effects
* p2p matches between IW4L clients, introduced through a master server
* selected IW5 and T5 data (see [`docs/MAP-LOAD.md`](docs/MAP-LOAD.md) for what
  each lane actually accepts; MW3 x64 Steam zones are detected and refused)

Verified scenario for this cut: `mp_boneyard` from a retail MW2 multiplayer
install on Linux — map load, movement, weapons, bots and effects, driven by
`make scenario` and `make chaos`. Anything past that list is untested rather
than promised. Expect missing gameplay systems, incomplete compatibility, bugs
and desyncs.

## Game data

IW4L needs a copy of the games whose content you want to load — MW2 for the
IW4 path, MW3 or Black Ops for the experimental lanes. Point `IW4L_GAMES` at
the folder holding those game trees.

The original installation is used **read only**. IW4L does not patch it,
replace files in it, or write into it; its own caches, settings, demos and logs
go to `iw4l-artifacts/` next to the IW4L binary.

## Build and run

```bash
cp .env.example .env          # IW4L_GAMES — folder containing the game trees
make map mp_boneyard          # run a map
make map mp_boneyard CMDS='spawn assault; wait 2s; quit'
make help                     # every recipe
```

Live runs use the `[profile.play]` profile, optimized for development builds.
`PROFILE=release` builds the full release binary. Windows setup is documented
in [`docs/WINDOWS.md`](docs/WINDOWS.md).

## Releases, network and updates

Builds are published as pre-releases (`v0.1.0-demo.N`, "IW4L Technical Demo
N"). Each one names the commit it was built from, the platforms it was actually
run on, the demo scenario and its known limitations, and carries `LICENSE`,
`NOTICE` and the bundled font licences. A platform appears in that list only
when a build for it was run, not when it merely compiled.

* **Nothing is stable yet.** The Rust API, the config format, caches, replay
  files and the network protocol may change between any two builds. Play a
  session on one release.
* **The network is experimental**, meant for arranged playtests between people
  who agreed to play. It is not a vetted environment for connecting to
  strangers.
* **Updating is an explicit action.** A release archive runs on its own; no
  build replaces its own binary without you asking it to.
* **No telemetry.** IW4L sends nothing home. Diagnostic files are yours to
  attach to a report. The demo master keeps operational logs for its own
  running; what it records is in [`docs/MASTER.md`](docs/MASTER.md).
* **The demo master is experimental infrastructure** with no uptime promise.
  Running IW4L locally never depends on it, and you can run your own.

Security reports: [`SECURITY.md`](SECURITY.md).

## Documentation

Detailed implementation notes live under `docs/` — start at
[`docs/INDEX.md`](docs/INDEX.md).

| file | about |
| ---- | ----- |
| [`docs/RUN.md`](docs/RUN.md)           | running the game, console scripts, commands and traps |
| [`docs/PERF.md`](docs/PERF.md)         | Perfetto tracing and performance analysis             |
| [`docs/RENDER.md`](docs/RENDER.md)     | rendering pipeline                                    |
| [`docs/MAP-LOAD.md`](docs/MAP-LOAD.md) | map loading and asset installation                    |
| [`docs/ENTITIES.md`](docs/ENTITIES.md) | simulation data flow and entity taxonomy              |
| [`docs/SIM-STEP.md`](docs/SIM-STEP.md) | simulation step architecture                          |
| [`docs/ANIM.md`](docs/ANIM.md)         | animation system                                      |
| [`docs/WINDOWS.md`](docs/WINDOWS.md)   | portable Windows build                                |
| [`docs/DEPLOY.md`](docs/DEPLOY.md)     | release, publishing and deployment                    |
| [`docs/MASTER.md`](docs/MASTER.md)     | running your own master server                        |

## Contributing and support

IW4L is a personal experimental project; see
[`CONTRIBUTING.md`](CONTRIBUTING.md) for what a useful bug report contains and
how changes are reviewed. Reports are welcome, with no promised fix date, and
"add everything the original had" is not a roadmap.

## Acknowledgements

IW4L is written from scratch, but it was not worked out in a vacuum.

* [OpenAssetTools](https://github.com/Laupetin/OpenAssetTools) and its
  [iw4x-x64 fork](https://github.com/iw4x-x64/oat) — open-source modding tools
  whose asset-structure headers document the on-disk layouts IW4L reads.
* [IW4x](https://github.com/iw4x/iw4x-client) — a custom client for MW2 (2009);
  read as a cross-reference for asset and protocol behaviour.
* [KisakCOD](https://github.com/SwagSoftware/KisakCOD) — an open-source CoD4
  reimplementation; read as a cross-reference for engine structure in a
  neighbouring generation of the same engine family.
* [Ghidra](https://github.com/NationalSecurityAgency/ghidra) — the reverse
  engineering framework the original binaries were read with.

Nothing from these projects is vendored, linked or distributed with IW4L.
[`NOTICE`](NOTICE) carries the full attribution, their licences, and the
bundled fonts.

## License

IW4L is licensed under the [Apache License 2.0](LICENSE). Copyright and
attribution notices are in [`NOTICE`](NOTICE).

This applies to the IW4L source code only. Call of Duty, Modern Warfare, Black
Ops and related game assets, trademarks and other intellectual property belong
to their respective owners and are not distributed with this project.
