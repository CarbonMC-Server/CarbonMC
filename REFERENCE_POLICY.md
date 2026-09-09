# Reference and licensing policy

## Review on 2026-09-09: transferred-repository CI repair

Rechecked local Pumpkin-ref/LICENSE, README license section and assets/NOTICE.md. They confirm the server's GPL-3.0 and separate API/asset terms; no reference implementation is needed for this Carbon-specific packaging failure. The diagnosis comes from Carbon's GitHub job logs and a local reproduction. The release-input correction and regression tests were independently written. No Pumpkin source, workflow, tests, assets or prose were copied. Carbon remains GPL-3.0-only with the same independent-implementation policy and separate third-party obligations.


## Project license decision — 2026-09-07

The maintainer explicitly selected GPL-3.0 for Carbon. LICENSE now matches the GPLv3 text chosen in the GitHub repository and Cargo metadata uses the explicit SPDX identifier GPL-3.0-only. The earlier Carbon MIT notice is preserved in LICENSE-MIT-HISTORY; prior grants and third-party licenses are not revoked or overwritten. Earlier dated reviews mentioning Carbon's MIT license are historical. This decision does not relax Carbon's no-copy/no-translation/independent-test policy for Pumpkin or any other server implementation, irrespective of language. Distribution provenance review remains necessary.


## Review on 2026-09-06: release packaging

Rechecked local `Pumpkin-ref/LICENSE`, README license/docs sections and `assets/NOTICE.md`; reference files remain excluded from release/source packages. The package format, explicit source selection, checksum manifests, launchers, smoke tests and CI were independently designed for Carbon. No Pumpkin source, prose, assets, packaging logic or tests were copied/adapted. Consulted [GitHub artifact documentation](https://docs.github.com/en/actions/tutorials/store-and-share-data) for transfer between build and smoke jobs and [Rust linkage documentation](https://doc.rust-lang.org/reference/linkage.html) for runtime-linkage behavior.

The local LLVM/MinGW LICENSE.TXT declares Apache-2.0 with LLVM exceptions and additional included notices. Local gnullvm packaging retains that exact notice for static runtime linkage. Cargo dependency and installed Rust runtime notice files are collected in the review package; this is an inventory, not a completed legal audit. Carbon's embedded protocol fixture provenance and all distribution obligations remain gate 1 work. No public release is authorized by generating local/CI review artifacts.


## Review on 2026-09-06: release documentation and support scope

Rechecked `Pumpkin-ref/LICENSE`, `README.md` (run/docs/license sections), and `assets/NOTICE.md` (server/plugin terms). Pumpkin's public-facing separation of setup, feature status, communication and licensing was used only as a high-level completeness cross-check. Carbon instructions, troubleshooting, bug-report fields and verification script were independently written from Carbon's CLI/config/runtime and local execution. No Pumpkin prose, source, tests or assets were copied/adapted; no dependency was added. The GPL-3.0 server and separate API/asset terms remain unchanged. No Pumpkin files are included in the local documentation-verification bundle. This does not close the broader distribution/provenance gate.


## Review on 2026-09-06: save-schema and generator-version safety

Consulted local `Pumpkin-ref/LICENSE`, root `Cargo.toml`, `README.md`, `crates/pumpkin-world/Cargo.toml`, and `assets/NOTICE.md`. The README lists world saving and Vanilla/Linear/Pump chunk loading/saving. This high-level cross-check confirms Carbon's sparse JSON save policy must not imply Pumpkin format compatibility, chunk storage, or preserved generated terrain. The world crate inherits GPL-3.0; the documented MIT/Apache plugin API exception does not cover world storage. No Pumpkin storage source, tests, algorithms, or assets were copied, translated, or adapted. New code and fixtures were independently designed from Carbon's existing state and save APIs; no dependencies were added.

GPL-covered reuse in a distributed derivative can carry GPL licensing and corresponding-source obligations; attribution alone does not satisfy them. See the [GNU licensing FAQ](https://www.gnu.org/licenses/gpl-faq.html.en). The local asset notice identifies separately governed Mojang data and other third-party content; it is not blanket redistribution permission. This milestone introduces no reference material into Carbon deliverables. This is a scoped provenance review, not a full repository/dependency/fixture legal audit; the distribution-review gate remains open.


## Review on 2026-09-06: probabilistic gravel and flint drops

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 before this milestone. Carbon's position-and-tick drop roll, fallback gravel stack, integration, and statistical regression test were independently designed around Carbon's existing block-break path. No Pumpkin source, algorithms, constants, tests, assets, or generated data were used, copied, or adapted. No dependency or attribution requirement was added. Fortune/Silk Touch modifiers, secure randomness, and falling-block behavior remain outside this milestone.

## Review on 2026-09-06: natural flint acquisition

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 and reviewed `Pumpkin-ref/assets/NOTICE.md` before this milestone. Carbon's seeded underground pocket placement and guaranteed prototype flint drop were independently designed using Carbon's existing world hash, sparse block palette, mining, and item-drop paths. The 26.2 gravel block-state and item registry numbers were treated only as interoperability facts; no Mojang data file was copied or redistributed. No Pumpkin source, algorithms, tests, or assets were copied or adapted, and no dependency or attribution requirement was added. Falling-block physics, probabilistic drops, Fortune/Silk Touch behavior, and broader sediment generation remain outside this milestone.

## Review on 2026-09-05: flint-and-steel Nether portal ignition

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 and reviewed `Pumpkin-ref/assets/NOTICE.md` before this milestone. Carbon's interaction routing, dormant-frame behavior, activation API, durability use, recipes, and tests were independently designed from Carbon's existing packet decoder, inventory, crafting, and frame-validation code. The 26.2 item registry numbers were treated only as interoperability facts; no Mojang data file was copied or redistributed. No Pumpkin source, algorithms, tests, or assets were copied or adapted, and no new dependency or attribution requirement was added. Natural flint acquisition, fire placement, off-hand ignition, variable frames, and sound/particle parity remain outside this milestone.

## Review on 2026-09-05: Nether portal frame integrity

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 before this milestone. Carbon's bounded nearby search, connected-interior traversal, revisioned collapse behavior, and tests were independently designed from Carbon's existing portal activation and block-edit APIs. No Pumpkin source, algorithms, constants, tests, or assets were copied or adapted. No dependency or attribution requirement was added. Generated starter portals, variable frames, and ignition items remain outside this milestone.

## Review on 2026-09-04: player-built Nether portal activation

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 before this milestone. Carbon's fixed-frame search, two-axis validation, revisioned portal fill, and tests were independently designed around Carbon's existing block query/edit APIs and established generated portal block. No Pumpkin source, algorithms, constants, tests, or assets were copied or adapted. No dependency or attribution requirement was added. Flint-and-steel ignition, variable frames, and collapse validation remain documented limitations.

## Review on 2026-09-04: cross-chunk lava light and lingering burns

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 before this milestone. Carbon's neighboring-chunk light-source collection, clipped light propagation, bounded burn timer, cleanup rules, and tests were independently designed from Carbon's existing chunk encoder and server tick state. No Pumpkin source, algorithms, constants, tests, or assets were copied or adapted. No dependency or attribution requirement was added; dynamic lava flow and full fire parity remain explicitly out of scope.

## Review on 2026-09-04: confirmed console world repair

Reconfirmed the GPL-3.0 status of `Pumpkin-ref` before implementing Carbon's reset workflow. The confirmation state, in-memory reconstruction, persistence replacement, player reset, and disconnect behavior were designed solely from Carbon's existing console loop and state ownership. No Pumpkin source, tests, assets, or implementation details were used. No dependency or attribution requirement was added.

## Review on 2026-09-04: safe join and respawn recovery

Reconfirmed `Pumpkin-ref/LICENSE` is GPL-3.0 before diagnosing Carbon's join and respawn behavior. The fix was derived entirely from Carbon's own saved-location, world-generation, and protocol-session paths: unsafe saved positions are relocated to clear supported terrain, respawn returns to the Overworld starter area, and the client receives a fresh chunk stream after its respawn world reset. No Pumpkin implementation, test, asset, or protocol layout was copied or adapted, and no dependency or attribution requirement was added.

## Review on 2026-09-04: Nether decorators and lava hazard/lighting

Rechecked `Pumpkin-ref/LICENSE` and the license declarations in its workspace before this milestone. Pumpkin's main server is GPL-3.0, so it remained a high-level behavioral reference only. Carbon's sparse soul-sand patch and basalt-pillar selection, starter exclusion, lava-contact tick rule, block-light construction, and tests were independently designed around Carbon's existing deterministic generator and protocol encoder. No Pumpkin source, tests, assets, layouts, constants, or algorithms were copied, translated, or closely adapted. The existing locally verified 26.2 lava state ID is reused as a protocol fact. No dependency was added; Carbon was MIT at that review date, and the existing Rust dependency set remains under permissive or public-domain-style terms as declared by its packages. Full distribution provenance review remains an open release gate.

## Review on 2026-09-04: Nether lower caverns and lava

Rechecked the local `Pumpkin-ref/README.md` goals and License & Attribution section. Pumpkin's main server remains GPL-3.0 and was retained solely as high-level context. Carbon's lower-chamber selection, ramp geometry, starter-region exclusion, bounded lava fill, sparse storage, and tests were independently designed from Carbon's existing Nether height fields. No Pumpkin Nether generator, noise/density parameter, lava algorithm, tests, data, or assets were copied, translated, or closely adapted. The source-lava block state ID (`102`) was read from the existing local Mojang-generated 26.2 `blocks.json` report; only that protocol identifier fact is embedded and the report is not distributed. This static prototype is not vanilla lava or Nether parity, and full distribution provenance review remains open.

## Review on 2026-09-04: cave material regions and aquifers

Rechecked the local `Pumpkin-ref/README.md` goals and License & Attribution section. Pumpkin's main server remains GPL-3.0 and was retained solely as high-level context. Carbon's exposed-wall region selection, basalt/sandstone/earthen palettes, selected-junction water geometry, bounded static behavior, optimization, and tests were independently designed from Carbon's existing cave graph and block model. No Pumpkin biome or aquifer implementation, density/noise parameters, fluid rules, tests, data, or assets were copied, translated, or closely adapted. No new block-state data or external dependency is introduced. This is not vanilla 3D biome or aquifer parity, and full distribution provenance review remains open.

## Review on 2026-09-04: natural cave entrances

Rechecked the local `Pumpkin-ref/README.md` goals and License & Attribution section. Pumpkin's main server remains GPL-3.0 and was used only as high-level context for playable world generation. Carbon's entrance eligibility, protected-spawn exclusion, two-leg slope geometry, world-coordinate reconstruction, connection to existing cave nodes, and tests were independently designed. No Pumpkin cave generator source, density parameters, algorithms, tests, assets, or layouts were copied, translated, or closely adapted. No external data or dependency is introduced. This is an original prototype rather than vanilla cave parity or a full provenance audit.

## Review on 2026-09-04: structure metadata and starter loot

Rechecked the local `Pumpkin-ref/README.md` License & Attribution section and retained Pumpkin's GPL-3.0 server as high-level reference only. Carbon's structure-kind/anchor/chest metadata, lazy deterministic loot initialization, dimension-specific item sets, persistence behavior, replacement protection, and tests were independently designed around Carbon's existing generator and chest save model. No Pumpkin loot table, structure metadata code, generator source, tests, assets, probabilities, or item selections were copied, translated, or closely adapted. No external data or dependency is introduced. The small Carbon loot sets are prototype gameplay, not vanilla-parity data, and the full distribution provenance review remains open.

## Review on 2026-09-04: biome-aware and dimensional landmarks

Rechecked the local `Pumpkin-ref/README.md` License & Attribution section before extending Carbon's structure generator. Pumpkin's main server remains GPL-3.0 and was retained strictly as high-level project context. The dry/snowy/woodland palette rules, Nether basalt waymark, End arch, selection rules, support checks, and tests were independently designed from Carbon's existing blocks and deterministic structure-cell model. No Pumpkin generator source, structure template, test, asset, parameter, or layout was copied, translated, or closely adapted. No external dependency or data is introduced; full distribution provenance review remains an open release gate.

## Review on 2026-09-04: Overworld trail lookouts

Rechecked the local `Pumpkin-ref/README.md` goals and License & Attribution section. Pumpkin's main server remains GPL-3.0 and was used only as high-level project context. Carbon's lookout selection, supported stone-and-timber geometry, cross-chunk reconstruction, and tests were independently designed from Carbon's existing structure-cell system. No Pumpkin implementation, tests, templates, or assets were inspected for layout or copied, translated, or adapted. The milestone introduces no external data, dependency, or bundled asset and is not a full provenance or legal-clearance audit.

## Review on 2026-09-04: End portal travel

Rechecked the local `Pumpkin-ref/README.md` License & Attribution section and retained the GPL-3.0 reference-only boundary. The portal-field geometry, routing rules, separated arrival coordinates, persistence behavior, and tests were independently designed for Carbon; no Pumpkin implementation, tests, or assets were copied, translated, or adapted. The End portal block state ID (`9468`) was read from the existing local Mojang-generated 26.2 `blocks.json` report. Only that protocol identifier fact is embedded, and the report is not added to distribution. This limited review does not replace the repository-wide distribution and provenance gate.

## Review on 2026-09-03: End island topology

Consulted the local `Pumpkin-ref/README.md` licensing section and retained its reference-only role. The central-island/outer-anchor layout, finite column slabs, void filtering, and entry fallback were independently designed around Carbon's existing data model. No Pumpkin implementation, tests, or assets were copied or adapted, and no new external data or dependency is bundled. This is original prototype generation, not a vanilla-parity or comprehensive licensing-clearance claim.

## Review on 2026-09-03: Nether caverns

Rechecked `Pumpkin-ref/README.md` License & Attribution and retained the reference-only boundary. The compact ceiling representation and seeded ceiling field are independently designed extensions of Carbon's existing terrain/noise and placement encoding. No Pumpkin world-generator code, tests, or assets were copied or adapted. Existing Carbon block types/IDs are reused; no new external data is bundled. This remains prototype behavior rather than a claim of vanilla parity or legal clearance of the entire repository.

## Review on 2026-09-03: surface vegetation

Rechecked the local `Pumpkin-ref/README.md` licensing section and followed the existing reference-only restriction. No Pumpkin implementation, tests, or assets were copied or adapted. Short grass, fern, and dead bush state IDs (2248, 2249, 2250) were read from the existing local Mojang-generated 26.2 `blocks.json` report, not from Pumpkin. Patch density and biome selection are original Carbon prototype rules. Only identifier facts are embedded; the report is not added to distribution. Full asset/provenance review remains a release gate.

## Review on 2026-09-03: biome terrain and coasts

Rechecked `Pumpkin-ref/README.md` License & Attribution and followed the previously reviewed GPL/reference-only boundary. This milestone uses project-level world-generation goals as context, not Pumpkin terrain source or parameters. The biome-family elevation profiles and 96-block interpolation policy are original Carbon choices using Carbon's existing biome catalog and noise function. No Pumpkin code, tests, or assets were copied or adapted. The release-readiness assessment explicitly leaves full distribution/provenance review open.

## Review on 2026-09-03: connected Overworld caves

Consulted the local `Pumpkin-ref/README.md` goals and License & Attribution section, and its `Cargo.toml` GPL-3.0 declaration. Used its performance and playable-world goals as high-level inspiration only; no Pumpkin terrain implementation, tests, or assets were copied, translated, or adapted. Carbon's new cave network uses an independently designed jittered cell graph with overlapping spherical cuts, its existing seeded hash, and existing block-placement encoding. Tests independently check connected air space, cross-chunk reconstruction, seed variation, edit precedence, and protected layers. This is not a vanilla-parity claim or a new legal clearance of the reference checkout.

## Review on 2026-09-03: command visibility

Rechecked the local `Pumpkin-ref/README.md` licensing section and workspace license declaration. Used its project-level security/extensibility goals as context only. No Pumpkin command-tree code, permission code, tests, or assets were copied or adapted. Carbon's existing command registry and verified 26.2 command encoder define this implementation; the new access predicates, live snapshot comparison, and regression tests were written independently. The previous GPL/reference-only restrictions still apply.

At the time of this earlier review, Carbon was declared MIT; the current project license is GPL-3.0-only as recorded above. The local `Pumpkin-ref` is a reference checkout, not a Carbon dependency or a source of code to copy.

## Review on 2026-09-03: negative permission nodes

Consulted `Pumpkin-ref/README.md` (project overview and License & Attribution), `Pumpkin-ref/Cargo.toml`, and `Pumpkin-ref/LICENSE`. The main server declares GPL-3.0. The README identifies separate MIT/Apache-2.0 plugin API licensing and separate third-party asset notices; those exceptions are not blanket permission to reuse server code.

For this milestone, Pumpkin provides project-level context only. No Pumpkin permission implementation, test implementation, or asset was used to implement Carbon's deny rules. The matching algorithm, precedence policy, persistence compatibility, and tests extend Carbon's existing permission model independently.

Carbon policy: consult this local reference for future milestones, but do not copy or translate its server implementation. Review each proposed dependency, asset, or code reuse separately, including notices and distribution requirements. This limited review is not a legal opinion or a complete provenance audit of the pre-existing repository.

The [GNU GPLv3 text](https://www.gnu.org/licenses/gpl-3.0.html), particularly sections 4 and 5, explains notice and licensing requirements for covered distributions. A reference checkout retains its own license: Carbon's project license does not relicense it. Review packaging before distributing the repository with reference material included.
