# Release documentation acceptance — 2026-09-06

## Sign-off

**Gate 5: complete for the current 0.1.0 developer-preview documentation baseline.** Verified against the source and a freshly built Windows executable. This sign-off covers release documentation/support scope, not authorization to publish or a claim that all release gates passed. The eventual tagged Windows/Linux release must revalidate this documentation against its own artifacts before publication.

| Requirement | Evidence / result |
| --- | --- |
| Release notes identify build and scope | RELEASE_NOTES.md: current snapshot, protocol 776/Java 26.2, feature scope, save changes, upgrade impact; no invented prior release comparison. |
| Install/run instructions match executable | OPERATIONS.md: locked release build, standalone working directory, explicit config, CLI check/start/stop. Verified locally on Windows. Linux instructions are explicitly awaiting platform acceptance. |
| Update/rollback/recovery | OPERATIONS.md and SAVE_COMPATIBILITY.md: separate snapshots, matching binary/config, working-directory rules, corrupt-primary recovery, no reverse migration. Local backup recovery and snapshot-copy restore verified. |
| Known issues and support boundaries | RELEASE_NOTES.md and SUPPORT.md: incomplete authentication/gameplay/durability and no claimed player-capacity or support SLA. |
| Bug-report instructions | SUPPORT.md: build hash/revision, environment, steps, expected/actual behavior, sanitized logs/config and save history. Existing private maintainer channel is the current route; no public tracker/contact is invented. |
| Conspicuous network warning | README, release notes, operations and support identify offline/trusted-network restrictions and production-world limits. |
| Local bundle behavior reconciled | Fresh binary/config/docs copied to an isolated verification bundle. Existing server shortcut documented as Cargo-based; stale ignored server/Carbon.exe is not presented as the current package. |
| Reference policy followed | REFERENCE_POLICY.md records local Pumpkin README/license/notice cross-check; no copied source, prose, assets or added dependencies. |

## Validation record

- Windows 10 build 19045; rustc 1.98.0, Cargo 1.98.0.
- `cargo build --release --locked --bin carbon`: passed.
- Executable SHA-256: `d2fd89b084eafc5785b904539fbcef9f0d44f8d8804e79fa7b3cf5abb9ece11c`.
- `python tools/verify_release_docs.py --binary target/release/carbon.exe`: ten process checks plus save-file assertions. Uses a new directory beneath work/release-docs and an ephemeral loopback port. Verifies help, valid/missing/invalid config, unsupported flag, console commands, current-directory saves, clean stop, restart/rotation, corrupt-primary quarantine/recovery, and restoration from a copied snapshot. It preserves its bundle and JSON evidence for inspection.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace --quiet`: 176 passed, 0 failed (3 config, 53 protocol, 120 server); other test/doc-test targets also passed.
- Final executable verification: ten process checks passed; evidence at `work/release-docs/verified-cq7h62oi/verification.json`.
- New documentation relative links: checked against the checkout; all resolve.

Raw verification logs and staged bundles are local evidence under ignored `work/release-docs/`; they are not published artifacts. Reproduce the verification with the command above. No existing server data or reference files are changed by the check.

## Other gates and publication prerequisites

Gates 1, 3 and 4 remain open: distribution/fixture/dependency provenance, two-client acceptance, and broader save/crash/power-loss acceptance. Windows/Linux release build and package smoke validation passed; see [RELEASE_BUILD.md](RELEASE_BUILD.md). The organization repository and committed baseline are established. Public release approval and a reporting/security contact remain separate prerequisites. Re-run this checklist when preparing a new release candidate.
