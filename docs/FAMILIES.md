# Family identity and composition

`asset_core::FamilyId` is an open family token used at asset, zone and script
boundaries. A game declares its own const ID; parsing resolves installed tokens
only and returns None before installation or for unknown tokens. Tokens use
lowercase ASCII letters, digits, underscores and hyphens, with 1–64 bytes.
`FamilyRegistry` iterates the installed families in token order. Only the IW4
reference family has a constant in core.

The builder calls `game_family::install` once with its family values. Every
`GameFamily` supplies a required `lane()` alongside its decoder and subsystem
policies. The installer validates each lane's declared owner and registers it in
`assets::ZoneLanes`; duplicate or mismatched registrations refuse. A missing lane
returns None, and common/material/world loading reports a refusal rather than
selecting another family. Missing-lane capability queries return MissingDecoder.
`assets::PreparedWorld` and the shared lane product types contain the normalized
outputs; private game loaders own stream parsing and native capture state.

`GameFamily::common_sources()` requires an explicit source policy. Games own
supplemental common/startup file plans and optional same-tree material donor
eligibility. Reference IW4 supplies no supplemental plan. The central installer
registers these policies; shared material donor selection visits enabled owners,
deduplicates matching paths and refuses conflicting plans. Material seed
contributions are likewise explicit optional planners. Core visits all declared
seed plans, overlays their startup captures, merges missing real materials into
the host, then loads their declared common sources. Each startup source declares
whether to retain its CAC tables; captured table ownership remains separate for
each source. Common capture visits enabled installed policies, preserves seeded
and deferred material contributions, and publishes source products in the
family's explicit `publication_order` (then token order). Shared donor paths are
captured once. Scene catalogs, material donors, UI images, and family compilers
are collections rather than fixed native slots. Compiler output is threaded into
the next compiler; deferred material scenes are remapped after the map pool wins.
Expected source ownership is checked before invoking a lane.

`ModelFamily.lod` explicitly supplies rigid/skinned controls, view-state update
and LOD selection. Reference selection lives in core; IW5/T6 explicitly reuse it,
and T5 owns its projection/hysteresis and distance selection. Renderer view
snapshots are keyed by installed model family and shared immutably across work.
Unknown LOD profiles refuse selection. Retained LOD keys include world-unit,
culling controls and all family projection bits. Team policies declare icon alias
suffixes; audio policies declare optional surface-explosion alias projection, so
renderer/HUD clients do not select those behaviors by native identity.

`GameFamily::map_scripts()` also requires an explicit policy. Native policies
project map declarations/entities into host-rule scripts and declare whether
reference faction rows need projection. Reference and IW5 policies retain their
packaged map sources. Built-ins contributed by every installed family are
available to every map family, so native equipment scripts remain available when
that family's weapons are used on another map. Unknown map-script policy lookup
refuses. Native radiation/destructible/equipment source files live with their
owners; the shared declaration writer consumes normalized compass/team data.

`GameFamily::localization()` supplies a complete `LocalizationFamily`: native
loader, source discovery plan and explicit first-wins priority. Shared loading
combines plans from installed families and advances progress over the files
actually found. The current priorities preserve IW4, T5, IW5, T6 precedence;
equal priorities use family-token order. Native archive names, versions and
language selection live in game code. Missing/ambiguous archives report gaps;
unknown loader lookup refuses before consulting the localization cache.
UI/bootstrap call `load_localized_strings_for_zone`, which resolves the anchor's
owner and consumes that family's plan, including missing-file diagnostics. The
world pass combines installed-family plans and counts only discovered sources.

The builder also installs a `GamesProvider` before discovery starts. The
`game_family` crate supplies `Iw4Family`; core supplies its reference lane and
decoder. Each game supplies its own envelope recognizer and decoder; zone stream
memory and format-specific header access stay with that game's parser.
`asset_transport::NamespaceTrees` and sound archive indices are keyed by family
ID. Discovery uses registered anchor zones and returns map groups keyed by ID.
Desktop implements the provider, including environment lookup, Steam libraries,
and Windows shortcuts. Core asks `asset_transport::games_root()` for that
provider's root and performs portable file traversal itself.

It has no default. Capture builders start without a family; capture requires
explicit selection. Failed map opens return a refusal and publish `MapLoadFailed`.
Empty worlds require an explicit family draw policy. `WorldFamily` also explicitly
supplies an optional objective-model completion hook. T6 fills missing authored
objective captures from its common world-model catalog, retaining native family
and capture-zone ownership. Other families supply `None`; the shared match walk
only invokes the installed policy.

Map-common shared surfaces and animation sources are stored in family-keyed
collections. Surface ownership comes from the opened zone header; animation
ownership comes from captured sources. Runtime common payloads override
supplemental payloads for the same family. A map consumes only its own family's
payload; missing data produces an empty collection, never another family's data.
Common scene completion likewise filters normalized captures by their recorded
family before retaining requested names and merging. Source slot names do not
establish scene ownership. Existing map captures retain precedence.

Assets retain their captured family through native compilation and publication.
T6 weapon rows, common meshes, clips, materials and sounds stay T6. Missing native
components refuse dependent capabilities. Another family's row or material
surface cannot stand in for them. T6 sound spatial policy uses captured native
alias flags and dry/near curves; six-speaker pans are explicitly projected into
the host stereo output.

The map selects soldier kits. A soldier's body and first-person hands belong to
the map's family. Kit arms take precedence over weapon-authored hands; authored
hands are eligible only when they belong to the soldier's family. Missing hands
leave first-person composition unresolved. T6 maps capture both faction viewhands
and require the selected kit hands for every weapon family. `SoldierPresentations`
prepares each selected kit once, retaining its body, native animation profile,
published mesh owner and checked hand mounts. `SoldierFpvPresentation` owns
hand admission; unused WeaponDef hands are optional. Head capabilities distinguish
absence, bound family/pose/mount and refusal; required collision refuses invalid heads.

`FamilyFpvMesh<F>` is issued by a published catalog after checking family and
owner. `SoldierFpvConnection<G, H>` connects native gun family G to native soldier
hands family H for IW4, IW5, T5 and T6. Both meshes must have the same owner.
`NativeFpvConnection<F>` names the same-family case. Attachments and auxiliary
models must belong to the weapon's family. The assembled skeleton validates
mounts; missing hands or mount tags refuse composition without substitute models.

Third-person body clips, animation sources, trees and scripts must match the
soldier's family. Missing foreign profiles refuse; IW4 clips are not a fallback.
The prepared body profile supplies simulation trees and persistent remote
animation; rendering does not rebuild character bindings per entity.
World policies provide family-specific sky, lighting and shadow inputs. Material
adapters compile those inputs and authored state into shared renderer products.

The simulation host shares engine primitives while registered `ScriptFamilies`
provide catalogs, native overrides and player/entity/weapon-argument policies.
Each family initializes its owned `RoundExtensions`; its `RoundState` traces
script values into the VM collector. Round resets drop that state. Model damage,
radius, stage queries and restarts dispatch through registered `ModelBehaviors`.
Model `OwnedExtensions` clone their mutable state independently with the DObj.
T5 owns its native script state and destructible implementation.

Weapon rows still contain family-specific extension fields; registry binding
and renderer code banks retain their existing interfaces.

`WeaponTableFamily` declares whether supplemental table capture applies. Native
callbacks return owned tables and diagnostics; T6 owns patch-zone loading and
its admission list. Unknown policy lookup refuses, while an explicit None means
no supplemental source is needed. Camouflage descriptors also own input table
names, compiler selection, image source and labels. Shared preparation iterates
installed policies and retains a family-keyed list of image plans instead of
selecting one native identity.

Prepared presentation requirements come from weapon runtime policies. T6 owns
its crossbow trigger and authored borrowed effect/sound keys; shared capability
collection queries normalized availability and validates the resulting required
presentation rows for client roles. Dedicated authority does not require those
presentation rows. FX corpus collection iterates installed families and retains
its existing dependency ordering/duplicates. A present sound alias remains
available even when its captured bool fact is false.

The session weapon compiler traverses one published registry to build complete
simulation rows. `SimWeaponContent` validates dense row order and script alias
targets and derives readiness from execution results, retaining refusal reasons.
It publishes immutable execution arrays. Match installation checks
the registry revision and exact product owner. Class projection and the session
manifest reject foreign publications and preserve execution refusals. `SimContentBuilder::bootstrap` keeps the empty
world explicit; installed matches use `for_match` with a compiled product.


Material preparation uses a concrete `VertexLayout` descriptor, not a closed
family enum. The descriptor supplies routing tables and host stride overrides;
IW4's reference descriptor lives in core, T5/T6 descriptors live with their games.
The builder installs `MaterialAdapters` keyed by descriptor owner. IW5 uses the
IW4 layout and SM3 encoding; T5 uses its own layout with the reference SM3
compiler. T6 owns its DXBC ABI builder and WGSL lowering. Each adapter returns a
common pass ABI and a `PreparedShaderProgram`; the frontend validates newly
lowered WGSL and derives GPU binding contracts through that common API. Shader
cache keys include the layout owner and the adapter's source-affecting key.


Animation capture in each game publishes normalized `RawXAnimParts` into the
shared catalog. `AnimationFamilies` supplies source decoding, grammar data,
leaf selection and optional type aliases. Native pose-condition mappings and
modifiers live in the games; the shared parser consumes those descriptors.
`PlayerAnimSources` refuses unknown or mixed families. Collision counting uses
sets of family IDs. The animation crate needs ECS component/resource traits,
so it depends on bevy_ecs rather than the full Bevy facade.


Audio capture and playback semantics use `AudioFamilies`, keyed by `FamilyId`.
Games own their native alias capture, flag interpretation, source-zone lists and
compressed-codec lowering. Shared catalogs store explicit channel/looping facts,
named curve references and normalized external byte ranges with standard audio
geometry. Typed native capture state lives behind the common claim/cache lifecycle.
Core playback consumes registered decode plans and a budgeted PCM sink; T5 owns
its stream/ADPCM wrapping and WMA implementation. External PCM/FLAC decoding
consumes generic ranges without knowing a native SAB entry. Resident clip keys
include family identity as well as encoded content and conversion geometry.
`AudioFamily` explicitly supplies bus trim/compression/order and UI alias remaps.
Mixer storage and script channel groups are built from installed audio families;
no enum-indexed four-family arrays remain. Bus buffers and envelopes are allocated
at resource construction, and render media must name a registered bus. Native
mix order is explicit (IW4, T5, IW5, T6) to preserve floating-point accumulation.
`TeamFamily.announcer` supplies native line projection; shared route assembly
handles team music and verifies the resulting alias in its captured namespace.
Reference teams explicitly retain reference routes. Registry installation precedes
audio resource construction.

Material frame/local-light binding uses registered `MaterialBindingFamilies`.
Games prepare demanded native producers; shared writers filter writes to the
catalogue's requested constants and reject stale generations. Fog row layouts
and sky targets are descriptors, with native constant IDs supplied by their
owners. `MaterialCompilers` registers game state/draw/technique policies against
normalized draw inputs. State and catalogue compilation refuse unregistered
families; scene construction forwards that refusal to match installation.

IW5/T5 material stream capture lives in each game's `material::capture`.
The shared catalogue exposes build-slot and zone-link operations; native readers
supply owner context and lowered records. `SourceTechniqueOccupancy` stores owned
bit/count vectors rather than a foreign format's array sizes. Shared links key
on `AssetPointerIdentity`; `MaterialLinkAddress` carries the owner's aliasability
decision. Native pointers are never presented as IW4 stream pointers. In
particular, T5's runtime blocks do not inherit IW4's persistent-block rules.

Shared material records carry native selector vectors and a single owner fallback
technique table. Native compiler policies validate selector counts and supply
selector diagnostics, feature-name variants, sort-capacity rules and zone-preview
names. Reference technique/code tables live in core. Catalogue/runtime diagnostics
and material census use family-keyed maps rather than one field per game. Native
T6 technique/material conversion extends the shared catalogue through game-owned
traits; its types and shader lowering stay with T6.

Common CAC tables carry their opened source family through startup and common
assembly. Source groups preserve order and duplicate rows; a missing common zone
does not erase an independently captured startup owner. Unowned nonempty table
groups are refused. Supplemental common UI images likewise retain the opened
zone owner instead of receiving a fixed family at final publication.

## Composition decisions

IW4 is the engine's reference representation and remains in core. Native world
vertices and models are converted by their family into that representation,
with explicit family ownership. The planned opaque bytes plus runtime layout
interpretation was replaced by normalized rows so shared consumers use one
validated representation.

The builder owns the installed set, cross-family showcase classes and desktop
service composition. Core's showcase list is empty until installed. Installation
validates every preset weapon owner; variants of the builder must supply presets
for their selected set. Each family contributes its own bot weapon rows through
`GameFamily::bot_weapons`; their union is visited in family-token order.

Asset toggles remain `sv_assets_<family>`, generated from the installed set.
They are a UI projection of the registry, not a hardcoded list of game slots.

Registries use `OnceLock`: one installed family set per process. Registration
order is canonicalized by `BTreeMap<FamilyId, _>` before consumers iterate.
Comparisons of independent installed sets therefore use separate processes;
there is no global registry reset API.

The master executable belongs to core with its protocol implementation. The
builder owns deployment tooling because it selects the release composition.
Launcher composition stays in builder; desktop supplies platform services.

Shared dependencies, patches and profiles are declared once in the root
workspace manifest. `cargo xtask check-workspace` checks content boundaries,
internal dependency paths and extension layering before workspace build/clippy.
`--structure-only` performs the structural checks. Runtime gates remain the
owner-approved scenarios. Game composition is described in [EXTENSIONS.md](EXTENSIONS.md).
