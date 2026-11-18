---
name: reinterpretation
description: Recreate original game behavior in IW4L through its own Bevy architecture. Use when recovering or implementing game capabilities from compiled-code analysis or observed behavior, or when deciding between decompilation and reinterpretation.
---

# Decompilation and reinterpretation

**Decompilation** reconstructs the original implementation from compiled code,
preserving its algorithms, structures, and logic as closely as possible.

**Reinterpretation** recreates the system's behavior using an independent
architecture. IW4L is built on Bevy, allowing it to reuse existing engine
systems instead of rebuilding everything from scratch.

**Key difference:** decompilation reproduces *how it works internally*.
Reinterpretation reproduces *what it does*, using modern tools and abstractions.

## Apply this to IW4L

Use reinterpretation for IW4L implementation work. Establish the observable
contract: inputs, outputs, errors, state transitions, timing, and gameplay
effects. Follow [EVIDENCE](../EVIDENCE.md) for research sources, compatibility
contracts, and the boundaries between binary analysis and production code.

Map that behavior onto existing IW4L state and services. Prefer Bevy systems
and abstractions where they satisfy the contract; identify any missing
capability before introducing a new subsystem. Internal similarity to the
original engine is not an acceptance criterion.

Validate the resulting behavior against the established contract. Explain
which behavior is supported by evidence, which architectural choices belong
to IW4L, and what remains uncertain. Keep research and verification artifacts
according to [ARTIFACTS](../ARTIFACTS.md).
