# Carbon roadmap

## Current priority: world generation

1. **Done (prototype):** connected deep Overworld tunnel network with seeded junction chambers, cross-chunk continuity, and protected surface/bedrock layers.
2. **Done (prototype):** distinct biome-family terrain profiles, smooth region-boundary/coast blending, elevation-based sea-level water, and protected spawn terrain; seeded short-grass, fern, and dead-bush surface patches with support/interaction handling. Flowers, plant loot, growth, and richer decorators remain future work.
3. **Done (prototype):** enclosed Nether terrain with a seeded ceiling/bedrock roof, seed-selected lower cavern chambers connected to the upper interior, and bounded static lava basins; finite End islands with a central landing area, surrounding void gap and seeded outer islands; plus active End portal fields with automatic round-trip travel and safe separated arrivals. Nether/End decorators, gateways, and full fluid/hazard behavior remain future work.
4. **Done (prototype):** richer structure families now include houses and trail lookouts with dry/snowy/woodland material palettes, Nether ruined portals and basalt waymarks, and End obelisks and arches. Each generated landmark has lightweight kind/anchor/chest metadata and deterministic starter loot with depletion persistence.
5. **Done (prototype):** sparse land-only cave mouths outside the protected spawn area, with two-leg descending passages reconstructed across chunks into seeded deep-network junctions; deterministic basalt, sandstone, and earthen exposed-wall regions; and bounded static aquifer pools in selected junctions.
6. **Done (prototype):** seed-selected lower Nether chambers with continuous cross-chunk ramps into the main cavern and shallow bounded lava basins below their midpoint, kept away from the starter arrival region.
7. **Done (prototype):** deterministic sparse Nether soul-sand patches and basalt pillars outside the starter area, periodic server-authoritative lava-contact damage, and initial lava-emitted block light in transmitted Nether chunks. Fire duration, lava flow, cross-chunk light propagation, and full generation parity remain longer-term work.
8. **Done (prototype):** lava light now propagates into adjacent transmitted chunks, and players continue taking bounded server-authoritative burn damage briefly after leaving lava. Dynamic lava flow, client-visible player fire state, extinguishing rules, and full fluid parity remain future work.
9. **Done (prototype):** completed player-built 4×5 obsidian frames automatically activate six-block Nether portal interiors in either horizontal orientation, using ordinary revisioned block edits so activation fans out and persists. Flint-and-steel ignition, variable frame sizes, and automatic collapse after frame damage remain future work.
10. **Done (prototype):** breaking any obsidian block in an activated player-built frame collapses the connected portal interior through revisioned persistent block edits in both supported orientations. Flint-and-steel ignition, variable frame sizes, and generated-portal integrity rules remain future work.
11. **Done (prototype):** complete player-built portal frames remain dormant until a nearby interior is used with flint and steel; the shapeless iron-ingot/flint recipe works in both crafting grids, successful ignition consumes durability, and activation remains limited to the Overworld and Nether. Natural flint acquisition, fire blocks, variable frame sizes, and generated-portal integrity rules remain future work.
12. **Done (prototype):** deterministic compact gravel pockets generate underground in the Overworld, gravel is placeable and shovel-mineable, and broken gravel provides a route to flint so portal ignition is obtainable through normal world play. Falling-block physics, probabilistic gravel/flint drops, Fortune/Silk Touch behavior, fire blocks, and variable portal frames remain future work.
13. **Done (prototype):** gravel now makes an independent server-authoritative ten-percent flint roll for each completed break and otherwise drops itself, using block position and server tick for repeatable testable outcomes. Fortune/Silk Touch modifiers and cryptographic randomness are not implemented; falling-block physics, fire blocks, and variable portal frames remain future work.

14. **Done (prototype):** variable rectangular Nether portal frames from 4×5 through 23×23 outer blocks ignite with flint and steel in either orientation, with bounded validation, required obsidian corners, revisioned persistent interiors, and frame-damage collapse. Cornerless frames, fire blocks, and generated-portal integrity parity remain future work.

These are incremental prototype milestones, not a promise of exact vanilla generation. Permission groups and other administration work remain on the backlog while world generation is the priority.

Release planning: see [RELEASE_READINESS.md](RELEASE_READINESS.md) for five preview gates (three open; builds and documentation complete) and eight larger production-readiness work areas; these are not equal-sized tasks or a completion percentage.

## Phase 1 — protocol foundation

- **Done:** pin protocol 776 and complete offline login through Login Acknowledged.
- **Done:** enter configuration, advertise brand/features, and negotiate known data packs.
- **Done:** synchronize the 26.2 configuration registries/tags needed by the vanilla client.
- **Done:** enter play, stream playable chunks, track joins/leaves, and exchange keep-alives.
- Add generated packet registries and golden fixtures for every connection state.
- Implement login encryption, authentication, and compression.
- Add bounded buffers, connection timeouts, rate limits, and fuzz tests.
- Test status compatibility against supported client versions.

## Phase 2 — playable core

- **Done:** deterministic in-memory chunk/block model and extension-facing block queries.
- **Done:** visible cow, pig, and zombie entities with deterministic wandering/chasing AI.
- **Done:** decode player positions and stream relative mob movement to 26.2 clients.
- **Done:** stream a player-centered 17×17 procedurally generated window and rescue players at the void floor.
- **Done:** align mob simulation with visible terrain and synchronize head rotation, sunlight fire/damage, hurt events, and removal.
- **Done:** native chunk heightmap, palette, section, biome, and light encoding, verified byte-for-byte against a clean-room fixture.
- **Done:** deterministic cross-chunk oak trees with log/leaf palettes, canopy heightmaps, and lighting.
- **Done:** sparse block edits, validated basic breaking/placement, item drops, starter inventory, hotbar synchronization, and `/give`.
- **Done:** player/mob combat, zombie melee attacks, health/hunger, regeneration, fall damage, death/respawn, mob drops, and multiplayer block-update fanout.
- **Done:** bounded A* mob navigation with terrain collision, diagonal corner checks, step/fall limits, replanning, and stuck recovery.
- Replace the captured configuration registry snapshot with generated registries.
- **Done:** unload distant client chunks and periodically evict inactive, unedited server chunks while retaining player edits and active entity regions.
- **Done (prototype):** command-driven crafting recipes, wooden tools with mining speeds/durability, visible item entities with pickup, and exact/type-correct block and mob loot.
- **Done (prototype):** JSON persistence for sparse block edits, inventories, vitals, and tool damage with autosave and clean-shutdown save.
- **Done (prototype):** atomic JSON replacement, backup rotation, invalid-primary recovery, server-authoritative food consumption, and terrain-aware dropped-item collision.
- **Done (prototype):** climate-driven plains, forest, birch-forest, and taiga regions with correct biome palette IDs plus distinct oak, birch, and spruce generation/drop types.
- **Done (prototype):** independent Overworld, Nether, and End simulations, command-driven dimension switching, dimension-aware saves/light/terrain, and the complete 66-entry 26.2 biome catalog.
- **Done (prototype):** deterministic cross-chunk Overworld houses, Nether ruined portal frames, and End obsidian obelisks.
- **Done (prototype):** a second deterministic Overworld structure family: supported stone-and-timber trail lookouts with open interiors, cross-chunk reconstruction, and seed-stable selection alongside houses.
- **Done (prototype):** biome-aware Overworld structure palettes using sandstone in dry regions, spruce and stone in snowy regions, birch accents in birch forests, plus seeded basalt waymarks in the Nether and supported obsidian/End-stone arches in the End.
- **Done (prototype):** lightweight generated-structure kind/anchor/loot-chest metadata, seed-stable dimension-specific three-stack loot, lazy chest initialization, saved depletion, direct-break drops, and replacement protection against loot regeneration.
- **Done (prototype):** ocean water, cross-chunk cave chambers, and depth-weighted coal/metal/redstone/lapis/diamond ore veins with matching drops.
- **Done (prototype):** deterministic connected deep cave tunnels and junction chambers, with east/south cell connections reconstructed independently across chunk boundaries, including negative coordinates. Existing isolated cave pockets remain alongside this network.
- **Done (prototype):** seeded dry-land cave entrances and continuous descending connections into deep cave junctions, with cross-chunk reconstruction and saved-edit precedence.
- **Done (prototype):** deterministic exposed-wall cave material regions and seed-selected deep-junction aquifer pools, limited to existing Carbon blocks and static water behavior.
- Implement faithful 3D cave-biome registration/decorators, aquifer and fluid simulation, richer vegetation, and climate/ore-distribution parity.
- **Done (prototype):** active starter portals, automatic Overworld/Nether travel, cooldown, and 8:1 horizontal coordinate scaling.
- **Done (prototype):** initial Nether cavern ceiling and bedrock roof while preserving floor terrain and portal clearance.
- **Done (prototype):** finite End-stone island slabs, void-separated central/outer topology, compact column bounds, void-safe structure filtering and dimension-entry fallback.
- **Done (prototype):** generated active Overworld/End portal fields, automatic Overworld/End travel through feet-or-head contact, the existing five-second portal cooldown, and fixed safe arrivals separated from the destination portal.
- **Done (prototype):** connected lower Nether chamber regions and static lava basins with verified 26.2 source-lava state encoding, deterministic bounds, and dimension isolation.
- Implement falling-block physics and Fortune/Silk Touch gravel modifiers, fire blocks, cornerless portal frames, stronghold placement, End exit/gateway progression, richer End topology/decorators, lava flow, and dimension-specific mobs/game rules.
- Implement the remaining vanilla structure families, template/jigsaw placement, loot tables, location commands, and structure save metadata.
- **Done (prototype):** craftable/placeable persistent furnaces, coal fuel, raw iron/copper/gold and cobblestone smelting, exact outputs, progress synchronization, shared world-position state, save/reload, and safe content drops on break.
- **Done (prototype):** craftable/placeable 27-slot chests with verified 26.2 screens, left/right and bidirectional shift-click transfers, shared multiplayer state, item-metadata persistence, save/reload, and exact content drops on break.
- **Done (prototype):** complete wood→stone→iron survival equipment progression, shaped stone/iron tool and iron armor recipes, corrected ore harvest tiers, tiered mining speeds, verified item IDs, durability, combat damage/speed, Sharpness, sweeping attacks, and axe shield disabling.
- **Done (prototype):** diamond tools and full diamond armor with verified 26.2 IDs, shaped recipes, diamond-tier speed/durability/combat attributes, armor toughness, auto-equip validation, and diamond-only obsidian harvesting.
- **Done (prototype):** cooked beef/pork, log-to-charcoal smelting, coal/charcoal/log/plank/stick fuel values, exact burn accounting, proper cooked-food hunger/saturation, save compatibility, and verified 26.2 item IDs.
- **Done (prototype):** verified 26.2 unsigned/signed chat-packet parsing, bounded revisioned multiplayer fanout as server-authored system chat, validation, per-connection rate limiting, join/leave announcements, and operator-only in-game `/say`.
- Implement signed-profile/chat-session authentication and reporting, richer components, moderation controls, double/trapped chests, hoppers and other specialized containers, and remaining multiplayer fanout.
- **Done (prototype):** persistent revisioned status effects with verified 26.2 update/removal packets and HUD icons; operator `effect give|clear`; server-side Regeneration, Poison, Hunger, Strength, and Resistance behavior; Speed/Slowness client movement; expiry and death cleanup.
- **Done (prototype):** exact bucket crafting and stack limits, verified 26.2 normal entity-interaction decoding, reachable-cow milking, persistent milk/empty buckets, and milk-based clearing of all status effects.
- Add the remaining vanilla effects, particles/ambient flags, effect stacking parity, potion data components, brewing and projectiles, beacons, effect-caused death attribution, and authoritative movement-speed validation.
- **Done (prototype):** dimension-scoped remote player tab/entity spawning, fractional movement, exact yaw/pitch and head rotation, arm-swing and hurt/death fanout, cooldown/range-checked player-vs-player combat, departure cleanup, and backward-compatible position/dimension persistence.
- **Done (prototype):** shared selected-hotbar/equipment state, visible held items and full iron armor, persistent equipment, vanilla-style armor reduction, and revisioned PvP knockback fanout.
- **Done (prototype):** wooden sword/shield recipes and verified item IDs, weapon-dependent damage, sprint-aware knockback, delayed directional shield blocking, persistent offhand state, and verified armor/shield/tool durability and breakage.
- **Done (prototype):** verified per-weapon attack speeds, 20%–100% cooldown damage scaling, charge-scaled knockback, fully charged wooden-axe shield disabling, and combat-state reset across death/dimension changes.
- **Done (prototype):** persistent per-slot Sharpness levels with server-authoritative damage and verified glint override, falling critical hits with animation fanout, and charged grounded sword sweeps against nearby players/mobs.
- **Done (prototype):** verified 26.2 player-container click decoding, raw storage/armor/offhand slot mapping, server-authoritative left/right pickup transactions, cursor synchronization, armor compatibility checks, and rejection/resync for unsupported gestures.
- **Done (prototype):** player-inventory quick-move and auto-equip, number-key/offhand swaps, Q/Ctrl-Q throwing, left/right drag distribution, collect-all, verified container-slot synchronization, and an exact-input 2×2 crafting grid for planks, sticks, and crafting tables.
- **Done (prototype):** world-interactive verified 26.2 3×3 crafting-table screen, shaped wooden tool/weapon and shield recipes, exact ingredient consumption, capacity-checked repeated shift-crafting, player-slot gestures, and safe grid return on close/disconnect.
- Add skins/property verification, chest/hopper/blast-furnace/smoker screens, furnace XP and more fuel/recipe coverage, recipe-book placement and remaining crafting-input gestures, enchantment-holder/tooltip serialization and enchanting UI, critical/sweeping parity, shield-disable chance parity, and the remaining vanilla combat/animation rules.
- Replace the prototype JSON save with chunk storage behind an async persistence trait.
- Introduce entity-component storage and deterministic tick scheduling.
- **Done:** persistent, case-insensitive operator management and operator-only command enforcement.
- **Done (prototype):** persistent case-insensitive player bans and allowlist membership, login-time enforcement, operator bypass for allowlist/capacity, full-server rejection, and administrative console commands.
- **Done (prototype):** revisioned targeted live-session disconnects, verified 26.2 play-disconnect encoding, operator `/kick` with validated reasons, and immediate online-player removal when banned.
- **Done (prototype):** backward-compatible structured ban records, persistent reasons, bounded duration parsing, `/tempban`, automatic expiry cleanup, informative denial messages, and reason-aware `banlist` output.
- **Done (prototype):** durable append-only JSONL moderation auditing for administrative commands and denied attempts, bounded restart recovery, torn-line tolerance, and operator `audit [count]` inspection.
- **Done (prototype):** persistent case-insensitive permission grants, exact/global/hierarchical wildcard matching, per-command requirements, operator/console bypass, audited denials, and grant/revoke/list commands.
- **Done (prototype):** persistent explicit negative permission nodes with deny-overrides-grant precedence, exact/subtree/global matching, case-alias merging, audited command enforcement, and preserved operator/console bypass.
- **Done (prototype):** permission-filtered play-command trees with live refresh, shared visibility/execution checks, permission-based in-game `/say`, restricted equipment/enchantment shortcuts, and sender-filtered command-registry help.
- Add authenticated UUID/IP bans, audit rotation/export, groups/inheritance, and full in-game administrative command routing/argument schemas.

## Phase 3 — extension platform

- Stabilize versioned event and command contracts.
- Define capabilities and resource budgets for extensions.
- Evaluate WebAssembly for sandboxed runtime-loaded extensions.
- Add extension dependency resolution and configuration namespaces.

## Save-schema and generator-version milestone — completed prototype (2026-09-06)

Scope: preview save/upgrade safety within the existing JSON persistence prototype. Schema 2 records generator 1; schema 1 loads without rewriting until save; unsupported schemas/newer generators stop loading without backup rollback. Recovery preserves a known-good backup and quarantines corrupt primaries on the next save. Disposable-file tests cover migration, backup recovery, interrupted rotation, failed writes, and downgrade refusal. Operator procedures are in [SAVE_COMPATIBILITY.md](SAVE_COMPATIBILITY.md).

Historical terrain generators, chunk/region storage, async persistence, actual power-loss testing, cross-platform release acceptance, and production durability remain outside this milestone. The broader release gates remain open.

## Release documentation milestone — 2026-09-06

**Done for the current developer-preview baseline:** release notes, operations/update/recovery instructions, known issues, support/reporting scope, and local documentation-verification script and sign-off. See [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md). Gates 1, 3 and 4 remain open; final tagged/platform artifacts require documentation revalidation before publishing.

## Release packaging milestone — complete for preview (2026-09-09)

Review-package builder and standalone launchers, source/file/build manifests, notice inventory, checksum validation, independent rebuild comparison, and Windows/Linux separate-job CI are implemented. Local Windows package smoke checks pass without the compiler/runtime toolchain on PATH. Gate 2 is complete for the committed baseline validated on Windows MSVC and Linux GNU, including independent rebuild comparison and fresh-job artifact smoke tests. See [RELEASE_BUILD.md](RELEASE_BUILD.md).

## Engineering gates

- Golden protocol fixtures and cross-version integration tests.
- Fuzz packet decoders and persistence readers.
- Benchmark tick latency, allocation pressure, and network throughput.
- **Done (prototype):** document clean shutdown, JSON recovery, schema migration and generator/downgrade semantics in [SAVE_COMPATIBILITY.md](SAVE_COMPATIBILITY.md). Production durability validation remains open.
