# Carbon

Carbon is an experimental Minecraft server platform written in stable Rust. Its goal is a fast, extensible server, evaluated against Minecraft server software across implementation languages—not only Rust projects. Carbon is developed as an **original implementation**: its policy prohibits copying, translating, or closely adapting another server’s implementation, whether written in Java, Rust, C++, Go, or any other language. Other projects may inform high-level requirements and interoperability checks; Carbon code and tests are independently designed. See [REFERENCE_POLICY.md](REFERENCE_POLICY.md).

> **Status:** playable prototype targeting **Minecraft Java Edition 26.2 (protocol 776)**. In development/offline mode, unmodified clients can join Carbon, see and fight one another, travel between three prototype dimensions, build, progress through wooden/stone/iron/diamond equipment, use persistent furnaces and chests, see and pick up dropped items, fight mobs, die/respawn, and retain world/player data across restarts. Complete vanilla generation/game rules and secure account authentication are not implemented yet. Do not use it for production worlds.

Carbon is pinned to Java Edition 26.2 rather than echoing whichever protocol contacts it. A 26.2 client reaches the login parser; other versions receive an explicit mismatch. The protocol number comes from `version.json` embedded in Mojang's official 26.2 server jar. Carbon does not bundle that jar or copy its implementation.

Review package build/checksum instructions: [RELEASE_BUILD.md](RELEASE_BUILD.md). Windows/Linux builds, independent rebuild comparisons and package smoke checks pass. Other release gates remain open; review artifacts are not automatically published.

## Preview documentation

[Release notes and known issues](RELEASE_NOTES.md) · [Install/update/operations](OPERATIONS.md) · [Save recovery](SAVE_COMPATIBILITY.md) · [Support and bug reports](SUPPORT.md) · [Documentation sign-off](RELEASE_CHECKLIST.md). No public binary release is approved by this documentation milestone.

## What is included

- A Cargo workspace with small, independently testable crates.
- Async TCP networking and Minecraft VarInt packet framing.
- Status-list response and ping/pong support.
- A verified 26.2 offline join path, configuration registry/tag synchronization, initial play packets, keep-alive, player-centered incremental chunk streaming, and distant-chunk unloading.
- Native 26.2 heightmap, paletted-section, biome, and skylight encoding generated independently by Carbon.
- Deterministic rolling terrain with ocean water, underground cave chambers, depth-weighted ore veins, biome-correct client palettes, and cross-chunk oak, birch, and spruce trees exposed through the extension API.
- Independent Overworld, Nether, and End simulations with dimension-aware chunk palettes, lighting rules, block edits, dropped items, and persistent saves. Compact gravel pockets provide a server-authoritative ten-percent chance of flint underground in the Overworld and otherwise return gravel. Nether lava emits adjacent-chunk light and causes contact plus brief lingering burn damage; sparse soul-sand patches and basalt pillars decorate terrain away from the starter portal. A standard 4×5 obsidian frame remains dormant until its interior is used with flint and steel, and breaking its obsidian frame collapses the connected portal interior.
- A catalog of all 66 biomes synchronized by the bundled 26.2 registry, partitioned across their correct dimensions and selected in deterministic broad regions.
- Deterministic cross-chunk prototype structures with starter loot: biome-aware Overworld houses and trail lookouts, active starter portals, Nether portal frames and basalt waymarks, plus End obsidian obelisks and arches.
- Sparse 3D block overrides, validated timed mining/placement, wooden-tool speeds and durability, starter items, and synchronization of all 36 storage slots plus armor/offhand.
- Visible dropped-item entities with gravity, pickup delay, exact stack quantities, inventory pickup, and despawning.
- Craftable stack-aware buckets, verified 26.2 cow interaction decoding, server-authoritative milking, and drinkable milk that returns the bucket and cures all active status effects.
- Server-authoritative player health, hunger, food consumption, starvation floor, fall damage, zombie melee damage, player attacks, death/respawn, and type-correct mob/ore loot.
- A persistent status-effect engine with verified 26.2 client HUD synchronization for Speed, Slowness, Strength, Regeneration, Resistance, Hunger, and Poison; periodic effects and combat modifiers are enforced by the server.
- Persistent block edits, player inventories, vitals, tool damage, position, and dimension in `world-save.json`, with periodic autosaves, atomic replacement, a recoverable `.bak` copy, and a final clean-shutdown save.
- Dimension-scoped multiplayer presence with fractional movement, exact client yaw/pitch, head rotation, held/offhand item and armor visibility, arm-swing and hurt/death animation fanout, weapon/armor-aware PvP, sprint knockback, directional shields, disconnect/dimension cleanup, and reappearance after returning.
- Revisioned block-change fanout so connected players observe one another's building and breaking.
- Server-authoritative mobs with collision-aware A* navigation, safe diagonal movement, step/fall limits, periodic replanning, stuck recovery, zombie pursuit, head rotation, and daylight damage.
- Player position tracking from vanilla movement packets.
- Config loading with validation and useful defaults.
- A fixed-rate tick loop with overload reporting.
- Thread-safe world and player abstractions.
- Commands including `help`, `list`, `say`, `version`, `op`, `deop`, `kick`, `ban`, `tempban`, `pardon`, `banlist`, `allowlist`, `audit`, `effect`, `grantperm`, `revokeperm`, `permissions`, and `stop`, with granular permission enforcement for administrative actions.
- A Rust extension API and a statically linked example extension.
- Structured logging through `tracing`.
- Tests and continuous-integration checks.

## Architecture

```text
carbon-server       composition root, runtime, network, tick loop, commands
    |-- carbon-api       stable-facing commands, events, extensions, world/player types
    |-- carbon-config    TOML schema, defaults, validation
    `-- carbon-protocol  codecs and Minecraft handshake/status packet primitives

hello-carbon        example extension linked by the server binary
```

The dependency direction points inward: protocol and configuration know nothing about the runtime, while extensions only depend on `carbon-api`. This makes it possible to evolve the network implementation without forcing extensions to depend on server internals.

## Run

Install [stable Rust](https://rustup.rs/), then:

```console
cargo run --release --bin carbon -- --config Carbon.toml
```

Add `localhost` in an unmodified Minecraft Java 26.2 client. The included development configuration binds only to `127.0.0.1` and uses `online_mode = false`, allowing a local client to join Carbon's starter world. Secure `online_mode = true` intentionally rejects login until Mojang session authentication and encryption are implemented.

The local Windows `Start Carbon.cmd` delegates to the ignored `server/` folder and runs Cargo from there; it requires the checkout/toolchain and does not execute the old `server/Carbon.exe`. It is not a standalone published package. For an isolated current build, follow [OPERATIONS.md](OPERATIONS.md).

Never expose offline mode to an untrusted network: it does not verify Minecraft accounts, so clients can choose their identity. Keep the default loopback bind unless you deliberately accept that risk on a trusted network.

Configuration can be checked without binding a port:

```console
cargo run --bin carbon -- --config Carbon.toml --check
```

Set `RUST_LOG` to override the configured logging filter, for example `RUST_LOG=carbon_server=debug`.

## Commands

Enter commands in the server terminal, with or without a leading slash. Typing `repair` starts a destructive full-world reset and asks `Confirm reset world? [y/n]`; only `y` or `yes` rebuilds every dimension from the configured seed and clears block edits, containers, entities, player inventories, vitals, effects, and saved locations. Connected players are disconnected so they can rejoin the fresh world. Operators, bans, allowlist entries, permissions, moderation history, and `Carbon.toml` are preserved.

```text
help
list
say Welcome to Carbon!
hello
mobs
chunks
spawnmob cow -8 65 -8
give CarbonTest oak_log 16
op CarbonTest
deop CarbonTest
ban Griefer
tempban Spammer 2h Repeated chat spam
kick NoisyPlayer Please follow the server rules
pardon Griefer
allowlist add CarbonTest
allowlist list
audit 20
effect give CarbonTest regeneration 30 2
effect clear CarbonTest regeneration
grantperm Helper carbon.command.say
permissions Helper
revokeperm Helper carbon.command.say
stop
```

`hello` is registered by the example extension.

Players can use ordinary Minecraft chat in the current offline-development mode. Carbon validates messages, rate-limits each connection, publishes them through a bounded revisioned history, and announces joins/leaves. Operators can also use `/say <message>` in game; console `say` remains available. Player messages are deliberately rendered as server-authored system chat because signed-profile authentication, Mojang chat-session verification, reporting, and moderation are not implemented yet.

The normal player inventory has a native server-authoritative 2×2 crafting grid for logs→planks, vertically arranged planks→sticks, and four planks→a crafting table. Place and right-click that table to open Carbon's verified 26.2 3×3 container, which supports shaped wooden, stone, iron, and diamond tools/weapons; all four iron and diamond armor pieces; furnaces, chests, buckets, crafting tables, sticks, planks, and shields; plus capacity-checked repeated shift-crafting. Three iron ingots in a V craft a bucket. Hold it and right-click a nearby cow to receive an exact milk bucket; drinking it clears every active status effect and returns the empty bucket. Wooden tools lead to cobblestone, stone tools harvest iron and lapis, iron tools harvest gold/redstone/diamond, and a diamond pickaxe can harvest obsidian. Each tier has distinct mining speed, durability, attack damage, and attack speed. Diamond armor adds its proper armor points, durability, and toughness calculation. A furnace uses eight cobblestone around an empty center; place and right-click it to open the verified 26.2 furnace screen. Raw iron/copper/gold smelt into matching ingots, cobblestone becomes stone, raw beef/pork become cooked food, and logs become charcoal. Coal and charcoal burn for 1,600 ticks, logs/planks for 300, sticks for 100, and every current recipe takes 200 ticks. Cooked meat restores eight hunger and 12.8 saturation. A chest uses eight planks around an empty center and provides 27 persistent slots with left/right clicks and shift-click transfers. Furnace and chest contents belong to their world position, synchronize between viewers, persist across restarts, and drop exactly when broken. `/craft_planks`, `/craft_sticks`, `/craft_table`, `/craft_pickaxe`, `/craft_axe`, `/craft_shovel`, `/craft_sword`, and `/craft_shield` remain available as recovery/development commands. Recipes consume exact inputs and produce exact output stacks.

Use `/equip_iron` as a recovery/development shortcut or craft iron and diamond armor normally; `/equip_shield` places a shield in the offhand. Equipment persists across restarts and is visible to nearby players. The normal player inventory screen supports server-authoritative left/right pickup, placement, splitting, merging, swapping, shift-click transfers and auto-equipping, hotbar/offhand number-key swaps, Q/Ctrl-Q throws, left/right drag distribution, storage collect-all, and armor/offhand interaction. Incompatible placement and malformed predictions are rejected with a complete slot/cursor resync. Iron and diamond armor reduce server-authoritative zombie and PvP damage using armor/toughness calculations and wear at their verified durability limits. Wooden, stone, iron, and diamond weapons/tools use tier-specific attack damage and speed attributes: early swings scale from 20% damage to a fully charged hit, charged sprint attacks add knockback, and a fully charged axe block disables the target's shield for five seconds. A shield raised for at least five ticks blocks PvP attacks from the front while taking durability damage. Fully charged descending attacks deal 1.5× critical damage and fan out the critical animation. Grounded, non-sprinting charged sword attacks sweep nearby players and mobs for one damage without re-hitting the primary target.

Hold a wooden sword or axe and use `/enchant_sharpness` to apply persistent prototype Sharpness I. It adds one attack damage and synchronizes the verified enchantment-glint override to the owner and nearby players. Carbon does not yet serialize the vanilla Sharpness tooltip because Mojang's generated report identifies the component type but not its runtime enchantment-holder mapping. Vanilla enchanting UI, higher-level commands, recipe-book placement, and drag/number-key gestures inside crafting input slots remain future work.

Walk into the active starter Nether portal near spawn to travel between the Overworld and Nether. Carbon applies the vanilla-style 8:1 horizontal coordinate scale and a five-second portal cooldown. A separate active End portal field is generated near `(8, -8)` in the Overworld, with a return field in the End; arrivals are deliberately separated from the destination field to prevent immediate bounce-back. Use `/dimension_overworld`, `/dimension_nether`, or `/dimension_end` for development/recovery travel.

`spawnmob` supports `cow`, `pig`, and `zombie`. New mobs appear for connected clients without requiring a reconnect.

Run `op <player>` from the server console to grant operator status; names are matched case-insensitively and the player does not need to be online. Use `deop <player>` to revoke it. `kick <player> [reason]` sends the verified Minecraft 26.2 play-disconnect packet to an online player. `ban <player> [reason]`, `tempban <player> <duration> [reason]`, `pardon`, and `banlist` manage structured records in `banned-players.json`; durations accept `s`, `m`, `h`, `d`, or `w` up to 365 days, expire automatically, and banning an online player disconnects them immediately. Existing name-only ban files remain readable. `allowlist add`, `allowlist remove`, and `allowlist list` manage `allowlist.json`. Set `server.allowlist_enabled = true` in `Carbon.toml` to enforce the allowlist. Operators bypass the allowlist and player-capacity check, but not bans. These lists are persisted beside the running server.

Every administrative command and denied player attempt is appended to `moderation-audit.jsonl` with its Unix timestamp, actor, action, target, and result. Carbon keeps the latest 1,024 valid entries in memory for `audit [count]` (default 20, maximum 100), preserves the full append-only file, and tolerates a malformed/torn line during restart recovery.

Administrative built-ins require individual `carbon.command.<name>` permission nodes. Operators implicitly have every permission. `grantperm`, `revokeperm`, and `permissions` manage case-insensitive player grants in `permissions.json`; exact nodes, global `*`, and suffix wildcards such as `carbon.command.*` are supported. Permission-management commands require `carbon.command.permissions`, preventing a helper with only `carbon.command.say` from escalating their own access. The console always bypasses permission checks.

The administrative `effect give <player> <effect> <seconds> [level]` command applies levels 1–5 for up to one hour; `effect clear <player> [effect]` removes one or all effects. Supported names are `speed`, `slowness`, `strength`, `regeneration` (or `regen`), `resistance`, `hunger`, and `poison`. Effects show in the vanilla client HUD, survive reconnects and clean restarts, expire on server ticks, and clear on death. Speed and Slowness currently rely on the vanilla client's movement modifier because Carbon does not yet enforce movement speed server-side.

Carbon initially sends a 17×17 chunk window, generates newly visible terrain whenever the player crosses a chunk boundary, tells the client to unload chunks that leave the window, and periodically evicts inactive unedited chunks from the server cache. Broad deterministic regions select from the complete dimension-appropriate 26.2 biome catalog, and each chunk transmits its registry ID. Tree-capable regions generate oak, birch, or spruce using distinct native block states and tree shapes; logs drop their own matching log item in an exact stack of one. Tree canopies contribute to chunk heightmaps and lighting. Falling below Y −32 triggers a spawn rescue, while ordinary falls use survival damage. Broken blocks and defeated mobs create visible item entities that collide with generated and player-placed terrain. Carbon autosaves every ten seconds at 20 TPS and saves again during a clean `stop`; writes rotate the prior valid save to `world-save.json.bak`, which is used automatically if the primary is missing or has corrupt JSON/payload data. Readable unsupported version metadata stops loading instead of falling back. Keep both files with the server to retain and recover progress. The old clean-room flat chunk capture is retained only as a byte-for-byte test oracle for the native encoder and is not used at runtime.

Save schema 2 records generator version 1. Legacy schema 1 upgrades on the next successful save; unsupported schemas and newer generators fail closed. Restore a separate pre-upgrade snapshot with its matching binary/configuration for rollback. See [SAVE_COMPATIBILITY.md](SAVE_COMPATIBILITY.md) for the compatibility matrix, backup instructions, interrupted-write recovery, and limits.

## Generation status and known gaps

The End now has an original finite central island, an empty surrounding gap, and seeded outer islands with tapered undersides. Void columns are empty all the way down: the client receives explicit End-stone slabs over an air foundation rather than an endless terrain fill. Obelisks require island support across their footprint. Portal or command travel into the End uses the central landing area at `(-8, 65, -8)`, while End-to-Overworld portal travel returns near `(0, 65, 0)`. This is prototype topology and travel: portal-frame activation, strongholds, gateways, cities, chorus plants, dragons, the dragon exit portal, and vanilla-parity distribution are not implemented. The existing prototype void-rescue behavior still applies.

**End upgrade caution:** previously continuous unedited End terrain will become void or finite islands. Back up valued saves first. Edits survive, but old builds can float and saved player positions are not automatically migrated; the new fallback applies to dimension switching, not all old saved positions.

Nether terrain has an original uneven cavern ceiling (Y=83–109) beneath a bedrock roof at Y=127. The existing upper floor remains unchanged and continues to drive safe dimension-travel arrivals; the main cavern has at least 22 air blocks of vertical clearance before structures. Away from the starter region, selected cells add a lower spherical chamber and a continuous rising connector into the main interior. Each lower chamber has a shallow, bounded source-lava basin below its midpoint using the verified 26.2 state ID `102`. Ceiling columns remain compact and expand only for chunk transmission, while lower layers use sparse overrides. Saved edits override all generated material. This is not vanilla Nether parity: lava is static, with contact/lingering burn damage and adjacent-chunk light, but lacks flow, full fire/extinguishing rules, and bucket interaction; Nether vegetation, decorators, and dimension-specific mobs also remain unfinished. Existing unedited terrain may gain lower chambers and lava after a generator update. Carbon's vertical chunk range is unchanged.

**Nether upgrade caution:** previously empty space now contains ceiling material. Back up valued saves; players or builds above the cavern floor may intersect newly generated terrain. Generator metadata does not preserve historical terrain; see [save compatibility](SAVE_COMPATIBILITY.md).

The first surface-decoration pass adds seeded short-grass patches in grassy biomes, ferns in temperate taiga regions, and sparse dead bushes in dry regions. Plants require suitable dry support and free space, do not overwrite trees/structures, and avoid the starter clearing. They are walk-through, replaceable when building, instantly breakable, and removed/persistently cleared when their support becomes unsuitable. Their verified 26.2 block states are encoded without adding collision-height obstacles. This is decorative coverage only: seeds, sticks/plant-item drops, shearing, growth, flowers, and full vanilla placement rules are not implemented. Existing world edits remain authoritative; regenerated unedited land may gain plants.

Biome-family height profiles now distinguish high peaks, foothills, plateaus, plains, deserts, wetlands, shallow seas, and deep oceans. A 96-block smooth transition straddles region boundaries, including negative coordinates; the starter area remains flat. Sea-level water fills low columns regardless of biome label, and new tree bases below sea level are skipped. Region selection and block-material/biome labels are still coarse; this does not implement vanilla climate placement, natural beach decoration, aquifers, fluid simulation, or every biome's unique terrain.

**Terrain upgrade caution:** this change affects unedited surface elevations as well as underground generation. Back up valued saves first. Existing edits are retained, but buildings may meet changed surrounding terrain and saved player positions may need rescue; generator metadata does not freeze terrain; see [save compatibility](SAVE_COMPATIBILITY.md).

For publishing scope and remaining work, see [RELEASE_READINESS.md](RELEASE_READINESS.md): an experimental preview and a secure production server have different requirements.

World generation is the current development priority: connected caves, biome terrain/coasts, surface plants, initial Nether/End terrain, portal travel, richer structures, starter loot, natural cave entrances, and the first underground-region pass are implemented as prototypes (see `ROADMAP.md`). The deep Overworld has an original seeded tunnel network with larger chambers at junctions. Sparse entrances appear only on dry land outside the protected spawn area; each uses a two-leg descending passage to reach a seeded junction instead of a sheer shaft. Entrances and deep connections are reconstructed from world coordinates, so adjacent chunks agree regardless of generation order. The main network is centered at Y=-24 through -12 with radius-three tunnels and radius-five junctions; existing isolated cave pockets remain. Seeded regions replace exposed stone with basalt, sandstone, or dirt accents, while selected junctions contain bounded lower-half water pools. These are visual/material cave regions and static aquifers, not full 3D biome registration or fluid simulation. Cave vegetation, entrance decoration, underground mob rules, and vanilla cave parity are not implemented.

**Existing-world caution:** Carbon persists edits and generator-version metadata, not complete generated chunks or historical generators. On restart/regeneration the new tunnels can change previously explored, unedited underground terrain. Saved edits override generation, but this does not freeze the surrounding geology or guarantee an old underground player position stays safe. Back up the save before trying this build on a valued world. No existing save or running server is modified by the development tests.

The biome registry catalog is complete for the bundled 26.2 configuration snapshot, but biome generation is intentionally still a prototype. Every biome can be selected and shown to the client, while many currently share a grass/dirt, sand/sandstone, snow/dirt, Netherrack, or End-stone terrain profile. Ocean regions contain source water up to sea level, caves carve across chunk boundaries, and stone contains coal, iron, copper, gold, redstone, lapis, and diamond veins with verified 26.2 states and matching drops. Vanilla climate blending, aquifers, ore distribution parity, richer vegetation, decorators, and biome-specific mob spawning remain unfinished.

The implemented structures are original deterministic starter structures, not replicas of every vanilla structure. Overworld structure cells select between enclosed houses and open trail lookouts, using sandstone palettes on dry terrain, spruce-and-stone palettes on snowy terrain, and birch accents in birch forests. Nether cells select between portal frames and basalt/soul-sand waymarks; supported End cells select between obelisks and obsidian/End-stone arches. These structures reconstruct consistently across chunk boundaries and carry lightweight kind, anchor, and loot-chest metadata. Each chest receives three seed-stable stacks from a small dimension-specific Carbon loot set when first accessed; claimed contents persist, breaking the chest drops remaining contents, and replacing it does not regenerate loot. This is not vanilla loot-table parity. Existing saved block edits remain authoritative, while unedited regenerated terrain may gain the new chests. Villages, strongholds, fortresses, bastions, monuments, mansions, mineshafts, trial chambers, general loot-table data, jigsaws, template rotation, and comprehensive structure persistence metadata remain unfinished. Nether terrain is an initial enclosed cavern prototype, and End terrain consists of finite prototype islands rather than vanilla-parity generation. The generated End portal fields provide basic travel only; frame activation and progression are absent, and Nether/End-specific mobs and gameplay are not implemented.

Before a public production-ready release, Carbon still needs real two-client regression testing for chat, dimension transitions, combat/equipment, furnace/chest screens, remote-player skins/movement, and the new liquid/cave palettes; online-mode authentication/encryption and signed-chat verification/reporting; compression; broader packet/action rate limits; crash-safe region storage; permission hardening; double/trapped chests, hoppers, blast furnaces, smokers, furnace XP/more fuels, and remaining specialized containers; skin/property verification; enchantment-holder/tooltip serialization and enchanting UI, critical/sweeping parity, shield-disable chance parity, and remaining animation synchronization; fuzzing; benchmarks; and broader storage migration tooling beyond the prototype [save policy](SAVE_COMPATIBILITY.md). Food use currently applies immediately rather than after the vanilla use-duration animation. The current scope is an experimental prototype or developer preview, not a production-ready replacement for existing Minecraft server platforms. Distribution still requires the release-gate checks.

## Extension model

Extensions implement `carbon_api::Extension` and are registered at startup. The example is compiled into the binary, keeping the initial design safe and portable. A future dynamic loader can sit above the same lifecycle after the ABI, sandboxing, permissions, and version-negotiation policy are defined; Rust dynamic-library ABI is intentionally not assumed to be stable.

```rust
use async_trait::async_trait;
use carbon_api::{Extension, ExtensionContext, ExtensionMetadata};

struct MyExtension;

#[async_trait]
impl Extension for MyExtension {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata::new("my-extension", "0.1.0")
    }

    async fn on_load(&mut self, context: &mut ExtensionContext) -> carbon_api::Result<()> {
        // Register commands and event interests here.
        let _ = context;
        Ok(())
    }
}
```

The API is intentionally narrow. Compatibility guarantees should only be declared after the project has real consumers and versioning tests.

## Permission exceptions

The in-game command tree now shows only commands the player can use and refreshes on the next network update (normally within 50 ms) after grants, revocations, deny rules, or operator changes. The server rechecks access when each command arrives; a stale suggestion cannot bypass a denial. In-game `/say` now uses `carbon.command.say`, including explicit denies. The development shortcuts `/equip_iron`, `/equip_shield`, and `/enchant_sharpness` require `carbon.command.equip_iron`, `carbon.command.equip_shield`, and `carbon.command.enchant_sharpness` respectively (or operator status). Crafting and dimension shortcuts remain public. Denied supported play commands return a message and are audited.

Command-registry `help` output is filtered using the same requirements as registry execution; the console sees all entries. The play tree still advertises only the implemented play shortcuts and `/say`: full in-game routing for administrative/extension commands and `/help` remains future work. Filtering does not introduce argument completion for those commands.

Prefix a stored permission node with `!` to deny it. Every matching deny overrides every grant, regardless of specificity. For example:

```text
grantperm Helper carbon.command.*
grantperm Helper !carbon.command.stop
grantperm Helper !carbon.command.op
grantperm Helper !carbon.command.permissions
permissions Helper
revokeperm Helper !carbon.command.stop
```

This illustrates exceptions, not a recommended safe helper role: `carbon.command.*` includes other powerful administrative commands. Prefer individual grants for untrusted helpers. Permission managers can change their own rules, so do not grant permission management to a player whose restrictions must be enforced.

Exact denies (`!carbon.command.stop`), subtree denies (`!carbon.command.*`), and a global deny (`!*`) are supported. Subtree patterns match descendants only, not the parent node or similarly named prefixes. Operators and the console still bypass rules. Revoking a grant does not create a deny; removing a deny can restore access from an existing broad grant. Rules remain case-insensitive, take effect on the next command, and survive restart in the existing `permissions.json` format. Downgrading to a version without deny support requires removing these rules first; older versions reject them.

## Development

```console
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Important next milestones are documented in [ROADMAP.md](ROADMAP.md). Contributions should preserve the clean-room rule: implement behavior from public protocol documentation and original research, never by copying proprietary or third-party server source.

Use the local `Pumpkin-ref` for high-level reference only, following [REFERENCE_POLICY.md](REFERENCE_POLICY.md). Carbon uses GPL-3.0-only for its own current code, but its independent-implementation policy still prohibits copying, translating, or closely adapting reference server code. Separate asset/API licenses remain separate obligations.

## Naming and affiliation

Carbon is an independent project and is not affiliated with Mojang Studios, Microsoft, or any other Minecraft server project. “Minecraft” is used only to describe protocol interoperability goals.


## License

Carbon’s current project license is **GPL-3.0-only**; see [LICENSE](LICENSE). Earlier Carbon snapshots used MIT; their notice is retained in [LICENSE-MIT-HISTORY](LICENSE-MIT-HISTORY), and this change does not revoke earlier grants. Third-party dependencies and data retain their own terms. Reference projects are not included or relicensed by Carbon.
