# Install, operate, update, and recover Carbon

> Use only a local machine or an explicitly trusted development network. Offline clients choose their identities; names, operators and allowlists do not establish account ownership. Keep `127.0.0.1` unless you deliberately accept that risk. Do not use production worlds.

## Build the current source

Use stable Rust with Cargo and the native compiler/linker required by the Rust toolchain (on Windows, the appropriate C++ build tools for an MSVC toolchain). The first build may need network access for dependencies. No Java runtime or Mojang server jar is required to run Carbon itself; the gameplay client is Minecraft Java 26.2.

From the checkout containing Cargo.toml and Cargo.lock:

```console
cargo build --release --locked --bin carbon
```

Use a new, empty working directory for a new world. On Windows PowerShell, from the checkout:

```powershell
New-Item -ItemType Directory carbon-preview
Copy-Item target/release/carbon.exe carbon-preview/
Copy-Item Carbon.toml carbon-preview/
Set-Location carbon-preview
.\carbon.exe --config Carbon.toml --check
.\carbon.exe --config Carbon.toml
```

The directory name must not already contain a server/world you want to retain. Linux source-build equivalent (not locally release-accepted):

```sh
mkdir carbon-preview
cp target/release/carbon Carbon.toml carbon-preview/
cd carbon-preview
./carbon --config Carbon.toml --check
./carbon --config Carbon.toml
```

A redistributed binary's native runtime dependencies must be established by the packaging gate; a build working on its developer machine does not prove fresh-machine portability. There is no approved download URL or published release package in this snapshot. Review packages can now be generated using [RELEASE_BUILD.md](RELEASE_BUILD.md); their standalone launchers run the packaged binary directly and require no Cargo at runtime. The original local root/server shortcuts remain separate development conveniences.

After the startup log, connect a Java 26.2 client to `localhost` (or the configured loopback port). Enter `version`, `help`, and `list` in the server console. Enter `stop` and wait for `server stopped cleanly` before copying data or closing the window. A forced termination can lose unsaved progress. `--help` describes supported arguments; there is no `--version` flag (use the console command).

## Connection protection limits

Carbon applies these fixed developer-preview transport limits before trusting a client. They count status checks and incomplete logins as well as players; operator status does not bypass them.

| Limit | Behavior |
| --- | --- |
| Concurrent connections | 128 total, at most 8 per source IP; IPv4-mapped IPv6 shares the IPv4 quota. Excess sockets close without spawning a session. |
| New connection tasks | Global token budget: 128 burst, refilling at 64 per second. |
| Initial handshake | Must finish within 10 seconds. |
| Status/login/configuration | Combined setup deadline of 30 seconds from acceptance. Successful intermediate packets do not extend it. |
| Play frame | A complete frame within 30 seconds, including idle time before its first byte. Partial reads and server update ticks do not reset the deadline. |
| Inbound frame size | Positive length, at most 2 MiB; malformed/overflowing prefixes are rejected before body allocation. |
| Inbound packets | Per connection: 240 burst, refilling at 120 packets per second. |
| Inbound bytes | Per connection: 2 MiB plus 5 bytes burst, refilling at 1 MiB per second; includes prefixes. |
| Outbound write | Each complete write must finish within 10 seconds, also respecting setup's earlier deadline. |

Exceeding a transport limit closes the connection. These are admission/input bounds, not a supported player-count or latency guarantee. Shared NAT/proxy addresses share the 8-connection quota; the limits are currently code constants, not configuration keys. Slow or unusually bursty clients can be disconnected. Keep offline/trusted-network restrictions in place: these controls do not provide authentication, DDoS protection, comprehensive parser fuzzing, or bounds on all world/save memory.

Stopping the server interrupts pending connection I/O, drains session cleanup, and returns inventory cursors/crafting contents before the final save. Disconnected and failed-login sessions release their connection slots. Real-client acceptance of the limits remains a separate release gate.

## Configuration and data placement

`--config` selects an existing TOML file; a missing file is an error and is not generated automatically. `--check` validates TOML and configuration fields without starting the server or binding a port. It does not test logging-filter syntax, save compatibility, port availability, or gameplay connectivity. Runtime startup performs those additional checks.

The supplied Carbon.toml explicitly uses loopback and offline mode. Omitting fields uses compiled defaults, including `0.0.0.0:25565` and `online_mode = true`; do not assume an empty config is equivalent to the supplied file. Keep the explicit supplied network settings. Leave the standard 20 TPS unless testing tick-based behavior deliberately.

**Data files are relative to the working directory, not to `--config`.** Runtime uses `operators.json` there and stores world/admin data alongside it. Launch from the intended data directory every time. Changing the world name does not select a new save directory. Starting from another directory can appear to lose a world by creating a different save.

Preserve `world-save.json`, `.bak`, any `.corrupt-*` evidence, `operators.json`, `banned-players.json`, `allowlist.json`, `permissions.json`, `moderation-audit.jsonl`, and your config/binary. Some files appear only after use. Logs may be console output unless separately captured. Never run two writers against one directory.

The root `Start Carbon.cmd` delegates to `server/Start Carbon.cmd`, which changes into `server/` and invokes Cargo using the parent manifest. It requires the checkout/toolchain and does not launch the old `server/Carbon.exe`. The ignored local server folder contains user data and is not a clean release package. The standalone commands above are the verified path for the current build.

## Update and rollback

1. Enter `stop`, confirm clean exit, and copy the whole data directory plus matching config/binary to a separate dated backup. Keep it unchanged.
2. Build the new source in its checkout, then test its binary against a disposable copy of that snapshot, isolated from players. Keep the same seed and world name. Validate configuration, start, inspect state, stop, and restart. Check edits in each dimension, containers, inventory, and player positions with clients before operational use.
3. Only after verification, replace the binary in the stopped working directory; retain existing data/config. Do not copy sample config over custom settings. Avoid running the new binary from the source checkout by accident.
4. For rollback, stop, preserve the newer state separately, then restore the entire independent pre-upgrade snapshot with its original binary/config. Newer progress is lost. Do not lower version fields by hand. `.bak` alone may already have migrated.

## Recovery and troubleshooting

See [SAVE_COMPATIBILITY.md](SAVE_COMPATIBILITY.md) for the full compatibility/recovery matrix. A missing or corrupt primary may load a compatible backup. Newer readable schema/generator metadata is a startup error, even if an older backup exists. On the next successful save after corrupt-primary recovery, Carbon quarantines the corrupt primary and retains the good backup.

| Symptom | Action |
| --- | --- |
| Missing config / invalid TOML | Check working directory and explicit path; run `--check`; unknown fields are rejected. |
| Invalid logging filter | Correct `logging.level` or `RUST_LOG`; `--check` does not validate the filter. |
| Address already in use | Stop the conflicting instance or use another loopback port; do not start a second writer on the same data. |
| Login refused | Verify Java 26.2 and explicit offline mode; review bans/allowlist/capacity. Online mode is not playable yet. |
| Empty or wrong world | Stop and check the working directory and configured seed before further saves. |
| Unsupported save version | Use a compatible newer binary or restore an independent old snapshot with its matching binary. |
| Both committed saves invalid | Preserve them for diagnosis and restore an independent snapshot while stopped. |
| Save/permission/disk error | Preserve the full error, resolve storage/access issues and verify a successful save; do not assume progress is durable. |

`repair` is a destructive reset, not a repair/recovery utility: confirmation clears world and player state. Do not use it to bypass a load error. Report reproducible problems using [SUPPORT.md](SUPPORT.md).
