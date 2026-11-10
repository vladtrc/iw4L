# Game extensions and fork builds

The repository is one Cargo workspace with one lockfile. Every internal
package is inside the checkout. Source builds need no installed games;
running a map requires its referenced games' data.

```sh
cargo build --locked -p launcher                         # all games
cargo build --locked -p launcher --no-default-features   # IW4
cargo build --locked -p launcher --no-default-features --features t5
cargo build --locked -p launcher --no-default-features --features t6
cargo build --locked -p launcher --no-default-features --features iw5
cargo xtask check-workspace --structure-only
```

`extensions/{t5,t6,iw5}` contain small registration facades (`ext_<game>`).
Their fastfile, zone, material, world, audio, gameplay and load subpackages
are named `ext_<game>_<capability>`. T5 also has script. World owns model
and animation conversion; load composes the family-specific loading path.
`dxbc_sm5` is a shared shader-format package under `crates/`.

Core packages cannot depend on extensions, and extensions cannot depend on
other games' extensions. Lower capability packages cannot depend on their
registration facade or load package. The workspace checker enforces these
boundaries and refuses dependency paths outside the checkout.

For a fork, select only the games needed at the launcher. A new game provides
existing `game_family::GameFamily` contracts through its registration facade;
add its optional dependency and feature to each composition entry point that
should expose it. Generation tools are a separate composition entry point.
