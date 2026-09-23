# CarbonMC handoff — 2026-09-23

Current scope: finish the interrupted encrypted-login milestone before another roadmap feature. Root AGENTS.md was removed upstream; the preserved work/ci-repair/AGENTS.local.md was read. Existing work was retained. Reference policy followed; no Pumpkin implementation was needed or copied.

Implemented: RSA challenge exchange, continuous AES-CFB8, bounded HTTPS Mojang session verification, canonical UUID/name admission, cancellation/timeout/concurrency bounds, encrypted compression/configuration coverage and fail-closed session tests. Default skins and system chat remain; signed-chat/property verification and licensed-client acceptance are open.

Local Windows gnullvm validation: formatting, locked workspace/all-target check, strict Clippy, 231 Rust tests (two existing subprocess helpers ignored), and nine Python package/save regressions passed. Strict Clippy required mechanical changes after the pending Rust 1.88 minimum-version bump. Native hosted release checks are pending. Local build uses the existing ignored MSYS Perl/Make tools via OPENSSL_SRC_PERL and PATH; do not commit those tools, logs or helper scripts.

GitHub: origin is CarbonMC-Server/CarbonMC, main is unprotected with no PR requirement. Preserve upstream 990084f's README title while integrating local changes. Commit/push and CI evidence will be recorded here after verification. Real accounts/two-client acceptance, power-loss acceptance and distribution provenance remain separate blockers.
