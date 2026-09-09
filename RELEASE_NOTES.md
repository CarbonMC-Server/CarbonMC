# Carbon 0.1.0 developer preview — documentation baseline 2026-09-06

> Experimental, offline-only gameplay for local or explicitly trusted networks. Account identities are not authenticated. Do not expose this build to the internet or use valuable production worlds. This document is not a public release announcement.

This describes the current source snapshot, not a tagged or published binary release. The package version remains 0.1.0; identify builds by source revision when available and executable SHA-256 because that version alone does not distinguish milestones.

## Included prototype scope

Minecraft Java Edition 26.2, protocol 776: offline login/status, three generated dimensions, sparse persistent block edits, inventory/crafting, wooden-through-diamond equipment, basic PvP/mobs, furnaces/chests, dimension travel, and console administration with permissions. These features have automated coverage; a complete two-client acceptance pass is still outstanding. Carbon extensions are compiled Rust components; Paper/Bukkit plugins and arbitrary runtime-loaded plugins are not supported.

The latest save milestone writes schema 2 / generator 1, reads legacy schema 1, refuses readable newer versions instead of silently restoring an older backup, and preserves a good backup after corrupt-primary recovery. Nine new tests cover migration/recovery/downgrade and failed writes. Earlier prototype additions include gravel/flint, fixed-size portal ignition/collapse, Nether lava lighting and damage, End islands, and generated structure loot. This is a current capability summary, not a verified diff from a previous tagged release.

## Upgrade impact

Back up the complete stopped data directory, configuration, and old binary independently. Legacy metadata upgrades on the next successful save. Unedited terrain regenerates with current code and seed; generator metadata does not freeze terrain. There is no reverse migration. Use a matching pre-upgrade snapshot/binary/configuration for rollback. See [operations](OPERATIONS.md) and the detailed [save policy](SAVE_COMPATIBILITY.md).

## Known limitations

- Secure online authentication/encryption, signed-chat verification/reporting and compression are incomplete. `online_mode = true` refuses gameplay login; it does not enable a secure playable server.
- No production durability guarantee, chunk/region persistence, historical generators, multi-writer exclusion, or comprehensive persistence fuzzing. A backup is one rotating generation; separate snapshots are necessary.
- No full vanilla terrain, fluid simulation, falling-block physics, End progression, redstone/gameplay parity, or complete specialized containers. Lava has contact/burn damage and lighting but no flow or full fire/extinguishing behavior.
- Remote skins, combat/animation parity, performance/soak limits, and a recorded two-client acceptance pass remain incomplete. The default player capacity is a setting, not a tested capacity guarantee.
- Windows/Linux build and package smoke validation passed on the recorded hosted runner platforms. Wider OS acceptance, distribution provenance clearance and a published tagged release remain open. Workflow artifacts are review packages.

For scope and reports, see [SUPPORT.md](SUPPORT.md). Documentation verification is recorded in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md); other release gates remain separate.

## License update — 2026-09-07

At the maintainer’s request, Carbon now declares GPL-3.0-only. The prior MIT notice remains in LICENSE-MIT-HISTORY; earlier grants and third-party terms remain unaffected. This does not change the independent-implementation policy.
