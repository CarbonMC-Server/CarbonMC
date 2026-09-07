# Prototype save compatibility and recovery

## Scope and versions

Carbon saves sparse block edits across dimensions, inventories/item metadata, equipment, player vitals/locations/effects, and furnace/chest state in `world-save.json`, alongside the operator file. It does not store complete generated chunks, historical generator implementations, or the world seed. Keep the matching `Carbon.toml` and binary with each independent backup. Do not run multiple writers or edit files while Carbon runs.

| Input | Behavior |
| --- | --- |
| Schema `version: 1`, absent generator | Legacy generator 0; load existing optional-field defaults; next save writes schema 2 / generator 1. |
| Schema 1 or 2, generator 0 or 1 | Load supported payload; next save writes schema 2 / generator 1. |
| Schema 2, missing/malformed generator | Refuse; explicit unsigned generator metadata is required. |
| Schema 0, schema above 2, missing/malformed schema, or generator above 1 | Refuse readable incompatible metadata, even with an older backup or unfamiliar payload. |
| Primary missing | Load a compatible backup; if neither exists, start a fresh world. |
| Primary malformed JSON or invalid payload with supported metadata | Load compatible backup; refuse if none is usable. |
| Valid primary | Prefer it; ignore backup and temporary file. |

Loading never rewrites files. The next successful autosave (200 ticks, nominally ten seconds) or clean `stop` saves current metadata. Existing tolerant handling of unknown block/item names and malformed gameplay records remains unchanged; version checks are not comprehensive semantic validation.

Generator 1 identifies the current implementation; generator 0 means legacy/unrecorded generation. Unedited terrain always uses current code and the configured seed; saved edits override it. No old generator is retained and no terrain transformation is performed. Builds may intersect changed terrain. Future generation changes must deliberately update the generator constant; schema changes must define supported readers/migrations and tests.

## Backup, upgrade, and rollback

1. Stop Carbon cleanly and confirm it exited. Copy the complete server data directory, configuration, and matching binary to a separate dated location. Keep that snapshot unchanged; the rotating backup is not an archive.
2. Test the new binary on a disposable copy with the same world name/seed, isolated from players. Check startup warnings, known edits in each dimension, inventory, containers, and player locations. Save, stop, restart, and verify again before upgrading the working server.
3. For rollback, stop the new server, preserve its files separately, and restore the complete pre-upgrade snapshot, configuration, and old binary together. Progress since that snapshot is lost. Do not edit version numbers or give a migrated save to an old binary: old builds may lack these guards or omit newer fields.

There is no reverse schema 2-to-1 migration. This reader's downgrade guards cannot enforce safety in previously released binaries. A rotating `.bak` may already be migrated after another save, so a separate pre-upgrade snapshot is necessary.

## Recovery and interrupted writes

The writer flushes `world-save.json.tmp`, rotates the primary to `world-save.json.bak`, and renames the temporary file to the primary. If final rename fails, it attempts to restore the backup. Startup ignores `.tmp`, even if complete, because it was not committed. An interruption after rotation recovers from `.bak`; before rotation the primary remains authoritative.

After corrupt-primary recovery, startup leaves files unchanged. The next save first moves the corrupt primary to a unique `world-save.json.corrupt-<UUID>` file, preserving the good backup through replacement. Retain that evidence for investigation; remove it manually only when no longer needed. Readable incompatible versions are never quarantined or overwritten. The writer rechecks compatibility before saving, but this is not a concurrent-writer lock.

For manual restore: stop Carbon, preserve current files elsewhere, and copy a known compatible snapshot's primary into the data directory with matching config/binary. Move damaged `.bak` and `.tmp` files out of that directory so they cannot become recovery sources. Restart on a disposable copy and verify known state first. If both committed files are invalid, restore an independent backup; do not delete them merely to bypass startup failure. World-reset commands intentionally discard state and are not recovery tools.

## Validation and durability limits

`cargo test -p carbon-server save_tests` exercises real disposable files: unchanged legacy load then migration, metadata/rotation, future-payload downgrade rejection without mutation, corrupt-primary recovery and subsequent save, missing-primary interrupted rotation, ignored temporary files, unusable backups, and failed temporary writes. Existing workspace tests cover multi-dimension edits, item durability/equipment, containers/loot depletion, furnaces, effects, and restarts.

These are deterministic filesystem-state tests, not process-kill/power-loss fault injection. The prototype does not sync parent-directory metadata, provide transactional snapshots across state locks/files, lock out a second writer, bound JSON input size, or guarantee durability on every filesystem. Rollback rename can also fail. Admin JSON files retain their separate existing writer behavior. No real saved world or running server is used by these tests. Chunk storage, async persistence, broader validation/fuzzing, and Windows/Linux packaged-release restore acceptance remain future work.
