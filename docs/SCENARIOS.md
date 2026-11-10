# Scenario utilities

`utils/scenario` compiles Rust/YAML actor timelines into runtime JSON tapes,
verifies authority receipts against declared expectations. Layer ownership: [SCENE-CONTRACT.md](SCENE-CONTRACT.md).
Concrete maps, positions, inputs, bindings, choreography, checks and film edits
belong to an external scene project or local artifact, outside the published tree.

## Compile and verify

Use `scenario::author::{Scene, Actor, Bindings}` from caller-owned Rust code,
or author equivalent YAML. Actor segments supply movement, buttons, weapon
selection and turns. `Scene::compile` validates actor uniqueness, time ranges
and quantum, resolves weapon roles and angle bases, and returns program/manifest.
Weapon IDs and delta angles come from explicitly supplied prepared-content bindings.

```sh
cargo run -p scenario -- build /path/to/scene.yml /path/to/bindings.yml /path/to/build
cargo run -p scenario -- verify /path/to/build /path/to/receipts.json /path/to/verdict.json
cargo run -p scenario -- compare /path/to/first-take.json /path/to/second-take.json
cargo run -p scenario -- convert /path/to/input.yml /path/to/output.json
```

The compiler uses complete commands on a positive divisor of 50 ms. Runtime
quantum zero preserves irregular recording times: [INPUT-PROGRAM.md](INPUT-PROGRAM.md).
The manifest records program SHA-256, bindings, tracks, beats and expectations;
`sources.json` fingerprints the compiler, lockfile and inputs. Verification checks
actor sets, tape/ensemble digests, submitted bytes, sequences and completion.
Missing evidence and unknown expectations fail; matching commands alone does
not prove matching state. Expectations include `shots`, `pickup_from_death`,
`deferred_can_swap`, `gravity_portal`, `native_final_kill` and `survives_hipfire`.
Supply actor, source, weapon roles and beats explicitly where required.

## Execute from an external Rust project

Use [scenario_runtime](HEADLESS.md) for in-process preparation and execution.
The external program supplies objects, callbacks and an output sink directly.
