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

The suite also kills a separate test process at six actual save-writer checkpoints: temporary open, temporary sync, backup removal, primary rotation, commit, and corrupt-primary quarantine. Each case reloads and saves again, checking the expected old/new committed block state and quarantine evidence. Pause hooks are compiled only into the test executable; shipped binaries contain none. These are process-crash tests, not hardware power-loss fault injection. The prototype does not sync parent-directory metadata, provide transactional snapshots across state locks/files, lock out a second writer, bound JSON input size, or guarantee durability on every filesystem. Rollback rename can also fail. Admin JSON files retain their separate existing writer behavior. No real saved world or running server is used by these tests. Chunk storage, async persistence, broader validation/fuzzing, and hardware power-loss acceptance remain future work. Packaged Windows/Linux acceptance passed for the committed baseline recorded below.


## Packaged migration, restore, and crash acceptance

`tools/release/smoke.py` runs `save_acceptance.py` against the verified extracted executable/launcher with the compiler removed from the child process PATH. It creates new directories under the specified work directory, uses ephemeral loopback listeners, and never opens an installed server's save.

A non-empty schema-1 fixture is upgraded and restarted. Exact witnesses cover edits in all dimensions, player vitals/location, damaged tools/offhand equipment, Fire Resistance, chest contents and furnace output. Effect duration may decrease by bounded elapsed ticks; other witness data must remain exact. A copied independent binary/config/save snapshot is restored without changing its source. Corrupt/missing primary recovery ignores uncommitted temporary files; future schema/generator and unusable backups must fail without mutating save evidence. Finally, the packaged server is forcibly killed before and after its first autosave and restarted twice.

`save-acceptance.json` records the binary hash, platform, process results, and success/failure. The release-validation workflow uploads it beside package smoke evidence for both native platforms. The acceptance oracle has mutation tests proving it rejects missing/changed gameplay data. Native platform results are recorded below.

**Gate 4 remains open for actual power-loss acceptance.** Process kills do not discard filesystem caches or exercise loss of directory metadata. Do not treat this suite as a guarantee against power interruption, multi-writer corruption, or all-files transactional consistency. Completing that requirement needs an isolated expendable VM/storage test environment and retained restart evidence; never power-cycle a user's working machine or a valued world for this test.


### Local execution evidence — 2026-09-10

Windows 10 build 19045, local gnullvm standalone review package `carbon-0.1.0-local-review-ab3b8fe7a10f-x86_64-pc-windows-gnullvm`: all 17 save-acceptance checks and seven package smoke checks passed with sanitized runtime PATH; 147 package file hashes verified. Evidence is retained locally at `work/save-gate-smoke/package smoke v5i1o8p2/save-acceptance.json` and `result.json`. This package is a labeled local-review build, not native MSVC/Linux release sign-off.

All 182 workspace tests passed; the child-process entry test is intentionally ignored during the ordinary harness run and explicitly invoked/killed by the passing crash parent test. Nine Python package/acceptance tests and formatting, compilation, and strict Clippy passed. Native Windows/Linux workflow evidence is recorded below. Actual power-loss acceptance remains blocked by the lack of an expendable isolated test environment.


### Native release acceptance — verified 2026-09-11

**Process-crash and packaged migration/restore acceptance: complete for commit `96963773dde6102baeab793f556fc69bfef3dca3`.** [Release validation run 34498092749](https://github.com/CarbonMC-Server/CarbonMC/actions/runs/34498092749) passed all four jobs; [ordinary CI](https://github.com/CarbonMC-Server/CarbonMC/actions/runs/34498092750) also passed. This is a scoped acceptance result, not completion of the power-loss requirement or permission to publish a release.

| Platform | Release build and independent archive comparison | Fresh-job package checks | Save acceptance |
| --- | --- | --- | --- |
| Windows Server 2022, x86_64 MSVC | Passed | 7 process checks; 146 file hashes | All 17 checks passed; real process kills before/after autosave |
| Ubuntu 22.04, x86_64 GNU, glibc 2.35 | Passed | 7 process checks; 148 file hashes | All 17 checks passed; real process kills before/after autosave |

The six save-writer crash checkpoints passed in the platform test suites. The downloaded `save-acceptance.json` reports were checked against the executable bytes inside their corresponding archives, and archive hashes matched `result.json`. Both build manifests identify the commit above. Review artifacts remain subject to the workflow's 14-day retention; hashes and this acceptance record are retained in source. Future candidate commits must run the workflow again.

| Target | Executable SHA-256 | Binary archive SHA-256 |
| --- | --- | --- |
| x86_64-pc-windows-msvc | `8a83f1e6553dd7d8dc78139002da32ab005701eb2cd703bc441e62dee30a3f84` | `e7ff3580e1e650150af64b58930622628cf0f6f1d121a4bb4c1d5538e3985dde` |
| x86_64-unknown-linux-gnu | `83e85d87bd2de7ebe5002f7ead591617e9dfbe7adf24892bf3384dff8eeb4f86` | `adebf86cfd8faed84eea887fb40d851f2d18a08fbadadde10c482a33e4b00abf` |

Remaining gate-4 acceptance: actual power-loss/restart evidence on expendable isolated storage. The maintainer confirmed no such environment is available. The gate remains open; the production storage limitations listed above are unchanged.
