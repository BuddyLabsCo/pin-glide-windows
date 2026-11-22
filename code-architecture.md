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
  before first use. A terminal pairing state determines the outcome when
  followed by Close `1008`.
- The delivery adapter captures arrival time and order, then attempts the
  Receipt before decryption. It owns reconnect, heartbeat, and Close handling.
  Core `delivery/` selects the key by Pairing ID and orders OTPs by `receivedAt`
  with arrival time as fallback and arrival order as the tiebreaker. An empty
  OTP still allows the full SMS popup and leaves the latest OTP alone.
- Slint renders state and emits actions. Rust owns popup copy and shortcut
  copy decisions. Send worker results through the Slint event loop. Keep
  clipboard writes and system events in `platform/`.

## 4. Persistent state

- Store settings and Receiver registration in separate versioned files under
  per-user Local AppData. Use current-user DPAPI for credentials and keys.
  Commit key references and protected blobs together.
- Use one writer. Stage and flush the new file before replacing the current
  one. Keep the previous committed data on failure. Read supported older
  formats and leave unreadable or newer unsupported files intact.
- Scope key loss and Pairing removal to the affected Pairing. Receiver
  revocation clears registration and Pairing state while keeping settings.
  Missing R requires explicit new registration. Installation upgrades
  preserve data. Uninstall removes app-owned data and registrations.
- Keep SMS bodies in their popups and the latest OTP in memory. Keep both
  out of persistent storage and logs. Leave the OS clipboard alone on cleanup.

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

The app owns WSS transport and checks message type and version. Preserve
optional wire fields and the required empty-OTP value. The Service adapter
creates `new_receiver` sessions using the installation's HTTPS origin and
native certificate and hostname verification.

## 6. Tests

Place Rust unit tests next to their modules. Put integration tests and
synthetic fixtures in the owning crate's `tests/` directory. Cover pairing
transitions, receipt ordering, empty OTP, and storage upgrades at their
interfaces. Keep script tests in `scripts/quality/*.test.mjs`.

## 7. Local development and CI

| Command | Checks |
| --- | --- |
| `node scripts/quality/install-tools.mjs` | Download pinned tools. Add the printed paths to local `PATH`. |
| `node scripts/quality/check.mjs` | Rust and Slint formatting, 500-line limit, crate dependencies, and Clippy with warnings denied |
| `node --test scripts/quality/check.test.mjs` | Quality script tests |
| `node scripts/quality/check.mjs core` | Portable core lint and tests |
| `node scripts/quality/check.mjs build` | Windows x64 MSVC check and release link |
| `node scripts/quality/check.mjs advisories` | RustSec advisories |
| `cargo test --locked -p windows-app --lib` | Service adapter, WSS wire models, and local HTTPS certificate verification |
| `cargo run --locked -p windows-app` | Open Welcome on Windows or reactivate the tray instance |
| `cargo run --locked -p windows-app -- --startup` | Start in the tray without opening Welcome |
| `cargo test --locked -p windows-app --test lifecycle -- --ignored` | Check single-instance activation, window size, and hide-to-tray on an interactive Windows desktop with Pin Glide stopped |

The source gates cover handwritten files in `receiver-core` and `windows-app`.
Generated code is checked through regeneration and compilation when added.
`rustfmt.toml`, `clippy.toml`, and `Cargo.toml` define the Rust rules.
The Slint build treats compiler warnings as errors in CI.

[functional]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6875824783
[time]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6879752448
[ui]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/3#issuecomment-7076999601
[service]: https://github.com/BuddyLabsCo/pin-glide-service/releases/tag/v1.0.2
