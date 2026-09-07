# Release readiness — updated 2026-09-06

## Bottom line

Carbon is a playable experimental prototype. It is not ready for untrusted public servers or valuable production worlds. A source/developer preview is a narrower target than a secure, reliable vanilla-compatible server; incomplete world features can be documented preview limitations rather than blockers.

No defensible completion percentage or calendar estimate is available. The five preview gates below and eight production work areas are planning groups, not equal-sized tasks or a claim that only thirteen code changes remain. Current passing local tests do not establish real-client compatibility, security, or legal clearance.

## Five preview release gates — four open, documentation baseline complete

1. **Distribution and provenance review.** Define a clean release file list; exclude local Pumpkin reference checkout, logs, saves, downloaded server jars, and generated research reports. Review the bundled configuration/chunk binary fixtures and dependency notices before distribution. `REFERENCE_POLICY.md` documents limited reference reviews, not a full repository audit.
2. **Reproducible release builds — implementation in progress; gate open.** Native Windows/Linux review packaging, standalone launchers, source/build/file manifests, dependency/runtime notice collection, deterministic ZIP metadata, checksums and a separate-job CI smoke/rebuild workflow are implemented. Local Windows gnullvm package startup/recovery passed with a toolchain-free runtime PATH. The reviewed committed baseline, real repository metadata, exact independent-build comparison and Windows MSVC/Linux hosted-run evidence still need completion. See [RELEASE_BUILD.md](RELEASE_BUILD.md). Artifacts are for review; no automatic release publication.
3. **Real-client acceptance.** Record a two-client 26.2 test pass: join/leave, chat, permissions, building/mining, inventory/crafting, containers, combat/death, dimensions, chunk transitions, and restart/rejoin. Recent milestones have automated tests, not a recorded visual client acceptance pass.
4. **Save and upgrade safety — prototype milestone implemented; release gate remains open.** Schema 2 / generator 1 metadata, schema 1 migration on save, fail-closed downgrade checks, disposable recovery/interrupted-rotation/write-failure tests, and operator backup/restore instructions are implemented. See [SAVE_COMPATIBILITY.md](SAVE_COMPATIBILITY.md). Unedited terrain still uses current code. Actual crash/power-loss exercises and Windows/Linux packaged-release restore acceptance remain outstanding.
5. **Release documentation and support scope — complete for the current developer-preview baseline.** Release notes, install/update/recovery instructions, known issues, bug-report steps, and offline/trusted-network warnings are written and reconciled with the current executable and local staging bundle. See [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) for evidence and documentation sign-off. Final tagged/platform artifacts must revalidate the checklist before publication; this does not close the packaging or other gates.

Gate 5 has a local documentation-baseline sign-off. Gates 1–4 remain open; this is not approval to publish a release.

## Eight major work areas before a production-ready public server

1. **Identity/security:** online-mode encryption and account/session verification, authenticated identities/skins, and a defined signed-chat policy. Currently `online_mode = true` rejects login because secure authentication is unimplemented.
2. **Protocol and abuse resistance:** compression, broader timeouts/rate limits, bounded resource usage, decoder/persistence fuzzing, and adversarial connection tests.
3. **Durable storage and upgrades:** chunk/region storage, async persistence, power-loss/crash testing and broader migration tooling beyond the implemented prototype schema/generator metadata and backup policy.
4. **World generation:** full 3D biome/decorator rules, fluid/aquifer simulation, richer Nether/End generation, portal progression, structures and loot. Current terrain, cave material regions, static aquifers, islands, and portal fields are original prototypes, not vanilla parity.
5. **Gameplay coverage:** remaining inventory/container/crafting rules, fluids/game rules, effects/brewing/projectiles, combat and enchantment parity, dimension-specific mobs, and clearly scoped remaining vanilla mechanics.
6. **Multiplayer correctness:** systematic real-client regression tests, synchronization under latency/disconnects, and authoritative movement/action validation.
7. **Performance and reliability:** load/soak tests, tick/network/memory benchmarks, leak and overload behavior, and repeatable Windows/Linux release validation.
8. **Extension/admin contracts:** stable versioned API/event contracts and documented capabilities, safe extension resource limits before runtime loading, and remaining permissions/administrative tooling. Runtime-loaded plugins may be deferred if explicitly unsupported.

“Production ready” must name a supported feature set and operational limits. A complete vanilla/Paper replacement is a substantially larger target than an experimental Carbon preview.

## Current implementation order

Finite End topology, portal travel, dimensional landmarks and loot, Overworld cave regions/aquifers, connected lower Nether chambers, sparse Nether decorators, periodic lava-contact and lingering burn damage, and adjacent-chunk lava light are now implemented as prototypes. Dynamic lava flow, client-visible fire state, and extinguishing rules remain open. Four preview gates remain open; automated generation tests do not close real-client acceptance, distribution review, save/upgrade exercises, or release packaging gates.

The Nether ceiling/bedrock roof, connected lower chambers, bounded static lava, deliberate flint-and-steel ignition for player-built fixed-size portals, ten-percent flint drops from underground gravel, frame collapse, End island topology, and separated-arrival End portal fields are implemented as prototypes. Falling-block and enchantment-aware gravel behavior, fire blocks, variable frames, strongholds, End progression, richer decorators, and full lava behavior are still open. Four preview gates remain open, including real-client visual acceptance of the terrain and portal transition.

Biome terrain/coasts, first surface plants, and the initial Nether cavern pass are implemented as prototypes. Next world work is End islands/portals and structures; flowers, plant loot/growth, and richer decorators remain open. These terrain additions do not close the remaining four preview release gates; documentation gate 5 was completed separately. Before advertising public-server readiness, prioritize security, persistence, and client acceptance over adding more world content. See `ROADMAP.md` for the detailed backlog.
