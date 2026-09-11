# Reproducible release builds

Gate 2 is complete for the validated developer-preview baseline recorded below. Scripts produce local review artifacts, not an approved public release. Gates 1, 3 and 4 remain separate. Do not distribute embedded fixtures or binaries until the provenance review is complete.

## Build contract

Python 3.11+ and Rust 1.98.0 are required. `tools/release/settings.toml` pins the compiler version for release builds; the general development toolchain can remain stable. Cargo.lock is mandatory and builds use `--locked`. The packager checks the actual compiler version and native target, even when RUSTUP_TOOLCHAIN overrides rust-toolchain.toml. CI uses Python 3.12. It supports native x86_64 Windows MSVC and Linux GNU, with local Windows gnullvm as a separately labeled validation target.

Run from the repository root:

```console
python tools/release/package.py --output dist/review --build-dir work/release-build
python tools/release/smoke.py --artifacts dist/review --work work/release-smoke
```

The output directory must be empty. Normal builds require a clean committed checkout with every selected input tracked. A clean commit is not proof of human review: the owner must approve the baseline separately. For an uncommitted local experiment only, add `--allow-unversioned`; the manifest/archive clearly says `local-review`. Local gnullvm additionally requires `--llvm-notice` pointing to its LLVM/MinGW LICENSE.TXT, and statically links the runtime. MSVC uses static CRT; Linux uses the host glibc, so its supported OS baseline must be validated on the selected runner/platform.

## Contents and identity

Only the executable, supplied config, standalone launcher, specified operator/project documents, collected dependency/runtime notices, and manifests enter the binary archive. No recursive copy of server/, Pumpkin-ref/, logs/, saves, jars, research outputs or target/ occurs. The companion source archive uses an explicit file/directory allowlist; unexpected extensions and symlinks are rejected. The protocol fixtures remain required build inputs; their presence is recorded, not cleared for redistribution by this milestone.

BUILD_INFO.json records package version, target, compiler/Cargo versions, source-file digest, commit when available, lockfile digest, and an explicit distribution-not-approved flag. SOURCE_MANIFEST.json hashes every selected source input. FILES.sha256 covers the package files; SHA256SUMS covers the binary/source ZIPs. Dependency notices are collected from local Cargo package metadata and Rust runtime notices; collection is not a complete legal audit.

ZIP entries have fixed timestamps, order and permissions, and use stored compression to avoid zlib-version differences. The compiler/linker flags remove common path/timestamp differences. This is a repeatable build recipe; bit-for-bit binary reproducibility is claimed only where independent-build comparisons actually pass. Build twice with different empty `--build-dir` and `--output` locations; compare their SHA256SUMS. Do not change source documents between those runs because docs are versioned inputs.

## Standalone use

Extract to a new directory, verify SHA256SUMS before extraction and FILES.sha256 before running, then use start-carbon.cmd on Windows or `sh start-carbon.sh` on Linux. The launcher enters its own directory and runs its packaged binary, not Cargo; Rust is not required at runtime. `--check` is forwarded and failure exit codes are preserved. Stop using the console `stop` command. Offline/trusted-network warnings and all save limitations in OPERATIONS.md still apply.

Extraction must retain Linux executable permissions; the verification helper does so explicitly, since some ZIP tools do not. Windows requires a compatible Windows 10+ environment; Linux runner compatibility and native dependency checks must pass before describing platform support as verified.

## Validation and publication boundary

The release-validation workflow builds Windows/Linux artifacts and checks them on separate fresh hosted jobs with a sanitized runtime PATH. It also builds twice and compares exact archive checksums. It uploads workflow artifacts for review only; it creates no GitHub Release, tag, or publication. Manual dispatch and pull-request validation do not approve redistribution.

Release validation requires a reviewed committed baseline, canonical repository metadata, matching independent-build checksums, and successful package smoke tests on both native platforms. Those checks passed for the baseline below. Future source or toolchain changes must pass the same workflow; a local build alone is insufficient.

## CI repair after repository transfer — 2026-09-09

Canonical repository: https://github.com/CarbonMC-Server/CarbonMC. Failed run 34133404735 was caused by commit bc8cc90 deleting AGENTS.md while SOURCE_FILES still required it. Both native build jobs failed the source-selection Python test with `Unsafe or missing source input: AGENTS.md`; dependent smoke jobs were correctly skipped. The transfer itself was not the failure: neither workflow hardcodes an owner/repository, uses custom secrets, release upload URLs or badges. Artifact names and target triples match between the build and smoke matrices; contents-read permissions are sufficient for these review workflows.

AGENTS.md is now excluded from release inputs whether present locally or absent. Required source/config/license files still fail closed when missing; regression tests cover both cases. Checksum/rebuild comparisons, strict Clippy, tests, artifact validation and separate smoke jobs remain enabled. Both workflows now explicitly run locked `cargo check` as well. Cargo/support URLs and the local origin remote point to the organization.

## Verified baseline — 2026-09-09

Source: `400067d4da2f25c525e8cc4209b6622ecd0fd80a`. [Release validation run](https://github.com/CarbonMC-Server/CarbonMC/actions/runs/34353608625): all four jobs passed. Windows Server 2022 (`x86_64-pc-windows-msvc`) and Ubuntu 22.04 (`x86_64-unknown-linux-gnu`) each passed locked Cargo check/tests, strict Clippy, formatting, seven package regression tests, two independent release builds and exact archive checksum comparison. Separate fresh jobs downloaded the artifacts and passed launcher/configuration, clean stop, restart and recovery checks with the compiler toolchain removed from the child process PATH. Normal CI passed in run 34353608555.

Download the workflow artifacts for the binary/source ZIPs and SHA256SUMS; each binary archive also contains FILES.sha256 and build/source manifests. These are validated review artifacts, not a published GitHub Release. Validation establishes these runner platforms, not every Windows/Linux version or production readiness.

The regression fixtures also resolve their temporary root to match production path handling, avoiding Windows short-path aliases being mistaken for paths outside the source root. The production containment check was not relaxed.


## Save acceptance baseline — verified 2026-09-11

Commit `96963773dde6102baeab793f556fc69bfef3dca3` passed [release validation run 34498092749](https://github.com/CarbonMC-Server/CarbonMC/actions/runs/34498092749), including native Windows MSVC/Linux GNU builds, exact independent archive comparisons, and both fresh-job smoke checks. Each package passed all 17 non-empty save migration/restore/crash checks in addition to seven package checks. See SAVE_COMPATIBILITY.md for the executable/archive hashes and acceptance details. Gate 4 still requires actual power-loss evidence; gates 1 and 3 remain open.
