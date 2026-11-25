# Code Architecture

This document defines where Windows Receiver code belongs. Product behavior
follows the [functional decision][functional], [message ordering decision][time],
and [UI decision][ui].

## 1. Runtime shape

One Cargo workspace and one app process per Windows user. `windows-app` owns
the Slint event loop and Windows integration. `receiver-core` owns Receiver
logic and runs on its own in Linux CI.

## 2. Repository layout

```text
crates/
├── receiver-core/src/
│   ├── lib.rs          # Public core interface
│   ├── pairing/        # Pairing state, QR payload and key confirmation
│   └── delivery/       # Decryption, message validation and latest OTP
└── windows-app/
    ├── build.rs        # Compile the Slint UI
    ├── src/
    │   ├── main.rs     # Wire modules and start the app
    │   ├── service/    # HTTP, WSS, Receipts and reconnect
    │   ├── ui/         # View state, user actions and popup lifetime
    │   ├── platform/   # Clipboard, shortcut, tray, windows and startup
    │   └── storage/    # Settings, registration and key protection
    └── ui/             # Slint views and assets
generated/
├── http-client/        # OpenAPI client
└── wss-models/         # AsyncAPI payload models
scripts/
├── contracts/          # Contract pins and generation
└── quality/            # Local and CI checks
packaging/windows/      # Installer and signing configuration
.github/workflows/     # Invoke repository scripts
docs/prototypes/        # Approved UI references
```

Create module directories as their code lands. Use `snake_case` Rust filenames
and name modules by responsibility. Keep handwritten Rust and Slint files
within 500 lines.

## 3. Module rules

- `windows-app` depends on `receiver-core`. Only `service/` consumes generated
  DTOs. Core interfaces use portable types and take no Slint, Windows, transport,
  or generated crate dependencies. Cargo metadata checks crate dependencies.
- Core `pairing/` owns provisional and active keys and the transitions from
  `credential_ready` to `credential_issued`. The first delivery WSS upgrade
  completes enrollment. The service adapter uses R for add-Sender creation and
  delivery, and B for pairing WSS, completion, cancellation, and first-R
  redemption. Persist the operation ID and attempt before redemption, and R
  before first use. Resume a persisted R with a bounded delivery handshake
  independent of the redemption deadline. A terminal pairing state determines
  the outcome when followed by Close `1008`.
  In `add_sender`, save the new key provisionally before confirmation and
  promote it on `completed` on the existing Receiver. Keep the delivery
  worker and its connection running during this operation.
- The delivery adapter captures arrival time and order, then attempts the
  Receipt before decryption. It owns reconnect, heartbeat, and Close handling.
  The same worker owns first enrollment and subsequent reconnects, or starts
  directly with the stored Receiver credential on launch. Windows power and
  connectivity notifications drive retry timing. A failed resume probe
  triggers an immediate replacement attempt. Keep control frames responsive
  during Receipt writes and authentication. Reserve space in the ordered
  authentication queue before writing a Receipt. Keep that queue across
  socket replacements. Hold deliveries for the in-flight add-Sender
  association with a separate capacity until its key is promoted or the
  attempt ends. Leave excess deliveries for that association unacknowledged
  and continue authenticating deliveries from active Pairings. Drain
  released deliveries before authenticating later arrivals for the same Pairing.
  The tray tooltip shows Ready only while connected
  with at least one active Pairing.
  Core `delivery/` selects the key by Pairing ID and orders OTPs by `receivedAt`
  with arrival time as fallback and arrival order as the tiebreaker. An empty
  OTP still allows the full SMS popup and leaves the latest OTP alone.
- Slint renders state and emits actions. Rust owns popup copy and shortcut
  copy decisions. Send worker results through the Slint event loop. Keep
  clipboard writes and system events in `platform/`.
  A Windows lock clears visible SMS popups and invalidates pending display
  callbacks. Delivery still updates the latest OTP while locked.

## 4. Persistent state

- Store settings and Receiver registration in separate versioned files under
  per-user Local AppData. Use current-user DPAPI for credentials and keys.
  Commit key references and protected blobs together.
- Use one writer. Stage and flush the new file before replacing the current
  one. Keep the previous committed data on failure. Read supported older
  formats and leave unreadable or newer unsupported files intact.
- Store interface language and launch-at-login preferences in
  `Pin Glide/settings.json`. Version 1 defaults to following the Windows UI
  language and enabling launch at login. Version 2 adds the popup duration
  in positive whole seconds, defaulting to 30. Preserve the full integer in
  settings. Each new SMS popup takes the saved duration at creation. Schedule
  long durations in bounded timer intervals. A duration beyond `u64` seconds
  has no reachable timeout during the run. Use the per-user Windows Run
  entry for `--startup` and show its effective state in Settings. Respect Windows
  Startup Apps overrides. Stage and flush the settings file before changing
  the Run entry. Restore the previous entry if replacement fails and show
  rollback failures in Settings.
  Version 3 adds the optional global copy shortcut. The tray window owns its
  Windows registration. Keep the previous registration until the candidate
  registers and the settings file commits.
- Store Receiver registration in `Pin Glide/registration.json` under Local
  AppData. Version 2 adds `provisional_pairing` with the session, Receiver,
  Sender and Pairing IDs and the protected key blob. Preserve the active
  Receiver credential and Pairings when staging a provisional key. Version 3
  adds the in-flight enrollment with its Service origin, expiry and operation
  ID. Protect B until redemption finishes and protect R before opening
  delivery. Promote R and the provisional Pairing together after the
  authenticated delivery upgrade. Version 4 adds the local Sender name to
  each active Pairing. Commit the first available default name on promotion
  so reception and crash recovery do not depend on the naming window. Read
  versions 1 through 3 and migrate them on the next successful write.
- Stage new-Receiver enrollment in `registration.pending.json`. A successful
  authenticated delivery upgrade promotes that record atomically. An explicit
  replacement can recover malformed registration data. A failed replacement
  leaves the previous registration, including any provisional Pairing, intact.
  Continue reading older in-file enrollment records on startup.
- Scope key loss and Pairing removal to the affected Pairing. Check the Pairing
  and Sender identities before applying a removal. Apply authoritative cleanup
  immediately, persist the intent in `registration.cleanup.json`, and retry
  registration replacement off the connection. On normal shutdown, stop
  delivery and flush accepted intents to the journal before exit. After Receiver
  revocation, the registration retains only a revocation marker. Keep settings.
  Missing R requires explicit new registration. Installation upgrades preserve
  data. Uninstall removes app-owned data and registrations.
- Keep SMS bodies in their popups and the latest OTP in memory. Keep both
  out of persistent storage and logs. Leave the OS clipboard alone on cleanup.
- Write fixed delivery rejection categories to `Pin Glide/delivery-rejections.log`
  under Local AppData. Reset the log at 16 KiB and keep the newest record.

## 5. Generated code

| Output | Generation input |
| --- | --- |
| `generated/http-client/` | `openapi.yaml`, OpenAPI Generator `7.27.0`, async `reqwest` with `native-tls` |
| `generated/wss-models/` | `asyncapi.yaml`, AsyncAPI Parser `3.6.6`, named payloads passed to Modelina `5.10.1` |

Use both [Service `v1.0.2` assets][service] from commit
`fb134954164acfc360816c36beeaec04863958ab`. The tag, commit, asset hashes,
and generator versions live in `scripts/contracts/service.lock.json`.
With Node, Java, and authenticated GitHub CLI available, run
`npm ci --prefix scripts/contracts --ignore-scripts` and
`node scripts/contracts/generate.mjs --check` to verify the checked-in
crates against a clean regeneration. Use `--write` to update the crates and
commit them with the workspace lockfile. CI runs the same drift check.

The app owns WebSocket transport and checks message type and version. Preserve
optional wire fields and the required empty-OTP value. The Service adapter
creates `new_receiver` sessions using the installation's HTTPS origin and
native certificate and hostname verification. The explicit compile-time Cargo
feature `development-transport` additionally permits HTTP origins and WS
upgrades for development only. HTTPS origins continue to use HTTPS/WSS and
native verification in both modes. Redirects are disabled in both modes.
Unsigned packaging does not enable development transport by itself.

## 6. Tests

Place Rust unit tests next to their modules. Put integration tests and
synthetic fixtures in the owning crate's `tests/` directory. Cover pairing
transitions, receipt ordering, empty OTP, and storage upgrades at their
interfaces. Keep quality gate script tests in `scripts/quality/*.test.mjs`.
Keep the Windows installer lifecycle test with its NSIS source in
`packaging/windows/`. See [Windows Receiver distribution](packaging/windows/README.md)
for its invocation and the packaged Receiver's interactive first-use test.

## 7. Local development and CI

| Command | Checks |
| --- | --- |
| `node scripts/quality/install-tools.mjs [slint-lsp\|cargo-deny]` | Download all pinned tools or the named tool. Add the printed paths to local `PATH`. |
| `node scripts/quality/check.mjs` | Rust and Slint formatting, 500-line limit, crate dependencies, and Clippy with warnings denied |
| `node --test scripts/quality/*.test.mjs` | Quality and changed-path classification tests |
| `node scripts/quality/check.mjs source` | Rust and Slint formatting and handwritten file size |
| `node scripts/quality/check.mjs dependencies` | Workspace dependency direction |
| `node scripts/quality/check.mjs lint` | Windows x64 MSVC Clippy with warnings denied |
| `node scripts/quality/check.mjs core` | Portable core lint and tests |
| `node scripts/quality/check.mjs build` | Windows x64 MSVC Debug link |
| `node scripts/quality/check.mjs release` | Windows x64 MSVC Release link |
| `node scripts/quality/check.mjs advisories` | RustSec advisories |
| `node scripts/quality/check.mjs service` | Windows library, pairing confirmation and automated lifecycle tests with default features, plus Service tests with `development-transport` (including local HTTPS certificate and hostname verification) |
| `cargo run --locked -p windows-app` | Open Welcome on Windows or reactivate the tray instance |
| `cargo run --locked -p windows-app -- --startup` | Start in the tray without opening Welcome |
| `cargo test --locked -p windows-app --test lifecycle startup_stays_hidden_and_explicit_launch_reopens_the_same_process -- --ignored --exact` | Check single-instance activation, window size, and hide-to-tray on an interactive Windows desktop using isolated settings |
| `cargo test --locked -p windows-app --test settings_desktop -- --ignored --test-threads=1` | Check Settings navigation, language selection, startup rollback and reduced motion on an unlocked Windows desktop |
| `cargo test --locked -p windows-app --test pairing_desktop -- --ignored` | Check key-check motion, QR removal and terminal states on an interactive Windows desktop |
| `cargo test --locked -p windows-app --test sms_desktop -- --ignored` | Check SMS popup rendering, scrolling and dismissal on an unlocked Windows desktop. Writes a synthetic OTP to the clipboard. |
| `cargo test --locked -p windows-app --lib sms_stack_matches_the_reference_timeline -- --ignored` | Check SMS stack motion, overflow, and work-area and DPI changes in an isolated process on an unlocked Windows desktop. |

The source gates cover handwritten files in `receiver-core` and `windows-app`.
Generated code is checked through regeneration and compilation when added.
`rustfmt.toml`, `clippy.toml`, and `Cargo.toml` define the Rust rules.
The Slint build treats compiler warnings as errors in CI.

### CI execution boundaries

| Event or changed paths | Format | Portable core | Windows | Service contracts |
| --- | --- | --- | --- | --- |
| Documentation or unrelated files | Yes | Yes | Skip | Skip |
| `scripts/release/`, auto-release workflow | + release tests | Yes | Skip | Skip |
| Receiver crates and Slint sources, generated crates, Clippy/Rustfmt config | Yes | Yes | Debug lint, link, tests | Generated crates only |
| Packaging, `windows-app/build.rs`, UI assets, development/production packaging workflows | Yes | Yes | + installer, policy and publication tests | Skip |
| Cargo manifests, lockfile, toolchain, `.cargo/` config | + advisories | Yes | + installer, policy and publication tests | Lockfile or generated manifests only |
| Quality scripts, CI workflow, shared local actions | + release tests and advisories | Yes | + installer, policy and publication tests | Skip |
| `deny.toml` | + advisories | Yes | Skip | Skip |
| Contract scripts or contracts workflow | Yes | Yes | Skip | Yes |
| Push to main | + release tests and advisories | Yes | Full baseline; installer when applicable | Same path filter as PRs |
| Weekly advisory schedule | Advisories only | Skip | Skip | Skip |

Format runs the source and dependency gates, quality script tests, and
Windows version check on every PR and main push. It compares the PR's base
with the checked-out merge commit, or the previous main revision with the
new revision. The NUL-delimited Git diff disables rename detection so both
paths participate in classification, including removed files. An unavailable
comparison opts into all checks. Service contracts uses GitHub's native path
filter on `scripts/contracts/**`, `generated/**`, `Cargo.lock`, and its
workflow, subject to GitHub's changed-file filter limits.

Windows runs Clippy, Debug linking, default and development-transport tests
on one runner and in a single Cargo target directory. Main also links
Release unless the packaging checks already link the production-mode
Receiver. Packaging uses a synthetic HTTPS origin, unsigned
`packaging/windows/build.ps1` (including actual PE checks), and the isolated
`installer.test.ps1`. It discards the installer without publishing it.
Production signing and publication workflows are unchanged.

CI uses Rust Cache for Cargo downloads and compiled dependencies. The
cache distinguishes OS, Rust toolchain, manifests/lockfile, configuration
and compiler environment. Windows separates the Debug/default +
development-transport and optional normal Release cache from the packaging
cache (static CRT Release, including the rejected development-origin test).
Workspace crates are rebuilt; Cargo fingerprints distinguish profiles,
features, and flags within a cache. A miss builds normally. The installer
is removed before the cache save; the production pipelines use no CI cache.

For timing comparisons, record the Format, Portable core, Windows and
contracts durations plus the cache restore and post-job save step durations,
cache hit/miss and size, and Windows lint/build/test/packaging step durations
on equivalent cold and warm Actions runs. The baseline from PR #53 was
Format and lint 4m52s (3m32s dependency direction/Clippy),
Windows MSVC build 11m18s (7m49s check/Release link,
2m42s Service tests), Portable core 36s, and Service contracts 16s.
Hosted cold/warm measurements are pending; local build times are not
comparable with the GitHub-hosted baseline.

Local Windows verification for this change (November 25, 2026): Clippy
38s, Debug link 2m35s, unsigned Release installer 12m53s, isolated installer
lifecycle 2m09s. With the same local target directory already populated,
Clippy took 3s, Debug link 8s, and Windows/default plus development transport
tests 20s. These measurements demonstrate local output reuse only; they
exclude GitHub cache transfer and cleanup overhead.

Set `PINGLIDE_SERVICE_URL` to the installation's HTTPS origin before building
to enable live pairing. Use the same origin in the Sender build.

[functional]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6875824783
[time]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6879752448
[ui]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/3#issuecomment-7076999601
[service]: https://github.com/BuddyLabsCo/pin-glide-service/releases/tag/v1.0.2
