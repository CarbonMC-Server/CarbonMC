# Reproducible release builds

Gate 2 is in progress. Scripts produce local review artifacts, not an approved public release. Gates 1, 3 and 4 remain separate. Do not distribute embedded fixtures or binaries until the provenance review is complete.

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

Full gate closure requires: reviewed versioned source baseline and real repository metadata; passing Windows and Linux build/reproducibility jobs; passing independent artifact smoke jobs; and recorded run URLs/checksums here. Local checks cannot substitute for remote platform results. This checkout initially had no commits, remote, working Linux installation, or GitHub CLI. The repository owner is supplying the destination.
