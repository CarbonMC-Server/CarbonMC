# Release readiness — updated 2026-09-11

## Status

Carbon is a playable experimental prototype. It is not ready for untrusted public servers or valuable production worlds. A source/developer preview is a narrower target than a secure, reliable vanilla-compatible server; incomplete world features can be documented preview limitations rather than blockers.

No defensible completion percentage or calendar estimate is available. The five preview gates below and eight production work areas are planning groups, not equal-sized tasks or a claim that only thirteen code changes remain. Current passing local tests do not establish real-client compatibility, security, or legal clearance.

## Five preview release gates — three open, builds and documentation complete

1. **Distribution and provenance review.** Define a clean release file list; exclude local Pumpkin reference checkout, logs, saves, downloaded server jars, and generated research reports. Review the bundled configuration/chunk binary fixtures and dependency notices before distribution. `REFERENCE_POLICY.md` documents limited reference reviews, not a full repository audit.
2. **Reproducible release builds — complete for the validated developer-preview baseline.** The committed organization-repository snapshot passed native Windows MSVC/Linux GNU builds, exact independent-build archive comparisons, and separate fresh-job package smoke validation. Versioned review archives include launchers, notices, source/build/file manifests and checksums. See [RELEASE_BUILD.md](RELEASE_BUILD.md) for the tested commit and workflow evidence. This does not authorize a public release or close provenance/client/save-safety work.
3. **Real-client acceptance.** Record a two-client 26.2 test pass: join/leave, chat, permissions, building/mining, inventory/crafting, containers, combat/death, dimensions, chunk transitions, and restart/rejoin. Recent milestones have automated tests, not a recorded visual client acceptance pass.
4. **Save and upgrade safety — process-crash and native restore acceptance complete; power-loss acceptance remains open.** Schema 2 / generator 1 metadata, schema 1 migration on save, fail-closed downgrade checks, disposable recovery/interrupted-rotation/write-failure tests, and operator backup/restore instructions are implemented. See [SAVE_COMPATIBILITY.md](SAVE_COMPATIBILITY.md). Unedited terrain still uses current code. Six save-writer process-crash checkpoints and native Windows/Linux packaged non-empty restore/upgrade acceptance passed for commit 9696377 in release-validation run 34498092749. Actual power-loss acceptance remains outstanding because no expendable isolated environment is available.
5. **Release documentation and support scope — complete for the current developer-preview baseline.** Release notes, install/update/recovery instructions, known issues, bug-report steps, and offline/trusted-network warnings are written and reconciled with the current executable and local staging bundle. See [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) for evidence and documentation sign-off. Final tagged/platform artifacts must revalidate the checklist before publication; other gates remain separate.

Gates 2 and 5 are complete for the developer-preview baseline. Gates 1, 3 and 4 remain open; this is not approval to publish a release.

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

## Production acceptance criteria

Completion must be demonstrated against a named release commit, supported client version, operating systems, player/load limit, and supported feature set. Passing unit tests alone does not establish these criteria.

| Area | Evidence required before production use |
| --- | --- |
| Identity and network security | Online-mode encrypted/authenticated login works with real accounts; invalid sessions fail; identity, skin, and chat policies are enforced. |
| Abuse resistance | Malformed packets, slow clients, login floods, oversized saves, and resource exhaustion have tested bounds and predictable rejection behavior. |
| Storage and upgrades | Documented durability contract; crash and actual power-loss exercises; isolated-writer guarantees; bounded/validated loading; verified migration, backup, restore and rollback on supported filesystems. |
| Multiplayer | Recorded two-client acceptance, then latency/disconnect regression coverage across inventories, containers, world edits, combat, dimensions, death and rejoin. |
| Performance | Reproducible load and soak runs establish sustainable player counts, tick latency, memory use, disk behavior and overload recovery. |
| Gameplay and generation | Define supported mechanics and test them end-to-end. Full vanilla/Paper parity additionally needs the outstanding fluid, generation, progression, combat, container, potion and mob work in ROADMAP.md. |
| Distribution and support | Fixture/dependency provenance cleared; release packages tested on Windows/Linux; known issues, update/restore steps, reporting contact and maintenance scope recorded. |
| Extensions and administration | Stable versioned contracts, tested access control and resource limits for supported extensions; otherwise explicitly exclude runtime-loaded extensions. |

## Current implementation order

Prioritize storage acceptance, authenticated networking, abuse resistance, and real-client validation before advertising public-server readiness. Recent gameplay prototypes include variable-size/corner-optional portals, water extinguishing, dimension-aware lava hazards, and Fire Resistance. These do not close the three remaining preview gates.

Save-safety work now includes deterministic process termination at six save-writer stages and a packaged non-empty migration/restore/crash acceptance runner. Native Windows/Linux execution evidence and package hashes are recorded in SAVE_COMPATIBILITY.md. Hardware power-loss acceptance remains a separate unmet requirement; process termination leaves the operating system and disk caches running.
