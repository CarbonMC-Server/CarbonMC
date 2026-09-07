# Preview support scope and bug reports

> Experimental offline development server. Local/trusted networks only; no authenticated identities or production-world guarantees.

The current target is Java 26.2 / protocol 776 with the prototype features in [release notes](RELEASE_NOTES.md). There is no promised vanilla/Paper parity, uptime/SLA, supported player-count benchmark, old-save downgrade converter, or third-party plugin compatibility. Windows local documentation checks are recorded in the checklist; Linux source instructions await platform acceptance. Automated checks do not substitute for a two-client gameplay pass.

The source repository is [CarbonMC](https://github.com/iamvip4973-crypto/CarbonMC). No public support SLA or security contact is designated. Send reports privately to the maintainer through your existing project communication channel; do not post secrets or sensitive saves publicly. A release owner must designate public reporting channels before distribution.

## Reproducible bug report

Include:

- Carbon console `version`, source revision if available, executable SHA-256, OS/architecture and Rust toolchain for source builds.
- Exact Java client version, number of clients, and whether the problem reproduces on the supplied config in a disposable world.
- Expected result, actual result, and numbered reproduction steps, including dimension/coordinates and actions where relevant.
- Relevant startup/error logs with timestamps; include the first error, not only the final shutdown message.
- Sanitized configuration, working directory versus config path, and whether the executable came from a local build or another package.
- For save issues: schema/generator versions, upgrade history, whether primary/backup/tmp existed, last successful save, and whether the shutdown was clean or interrupted. Preserve originals before trying recovery.

Redact names/UUIDs, IP addresses, private paths, chat/moderation history, tokens and other sensitive data. Do not attach your whole server folder, Pumpkin reference checkout, Mojang jars, or assets. Supply a minimal synthetic reproduction or privately agreed sample rather than a valuable world. For identity/security issues, use the existing private maintainer channel and avoid posting exploit details publicly before coordination.

For crashes, first preserve data and logs; for rollback, follow [operations](OPERATIONS.md). A report is not permission to reset the world or change production data.
