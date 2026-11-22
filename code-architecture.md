# Code Architecture

Read this placement contract before adding, moving, or reviewing Windows
Receiver code. The [Windows functional resolution][functional], its
[message-time clarification][time], and the [Windows UI resolution][ui] govern
product behavior. The current Hello World exercises the workspace and quality
gates. The paths below also reserve homes for future Receiver implementation.

## Ownership and Placement

| Change | Home | Interface and ownership |
| --- | --- | --- |
| Pairing rules, QR payload and typed key confirmation | `crates/receiver-core/src/pairing/` | Own state transitions, framing, and provisional versus active key selection. App adapters execute IO and return typed outcomes. |
| Decryption, message validation, latest OTP selection | `crates/receiver-core/src/delivery/` | Take portable input and explicit arrival time/order. Return validated message data and cache outcomes. |
| HTTP and WSS | `crates/windows-app/src/service/` | Map generated wire types. Own HTTP authorization, connection ownership, frame IO, arrival Receipts, reconnect, heartbeat and Close handling. |
| Window actions and view state | `crates/windows-app/src/ui/`, `crates/windows-app/ui/` | Rust maps use cases to view state and actions. Slint renders and emits intents. Keep copy decisions and popup lifetime in Rust. |
| Native integration | `crates/windows-app/src/platform/` | Own clipboard, global shortcut, tray, window placement, startup, single-instance coordination and session/power notifications. |
| Settings and registration | `crates/windows-app/src/storage/` | Own persisted types, Windows key protection, versioned reads and writes, and migration. |
| Composition | `crates/windows-app/src/main.rs` | Wire the modules and run the UI event loop. Marshal worker results through the Slint event loop using weak handles. |
| Contract generation and release | `scripts/contracts/`, `generated/{http-client,wss-models}/`, `packaging/windows/` | Own generation configuration and output, and installer/signing configuration. Keep signing keys and download credentials in CI. |
| Tests | Colocated Rust tests, crate-local `tests/`, `scripts/quality/*.test.mjs` | Cover public transitions/interfaces and use synthetic protocol and storage fixtures. Put tests for quality tooling next to the scripts. |

`windows-app` depends on `receiver-core` and may consume the two generated
crates through its service adapters. The core takes no Slint, Win32, transport
runtime or generated wire types. Generated crates have no local dependencies.
Use private modules and small typed interfaces. Add a portable core dependency
by reviewing and updating the explicit allowlist in `scripts/quality/check.mjs`.
The compiler catches an undeclared `slint::SharedString` import in the core.
The metadata gate rejects declaring Slint there, including renamed or
target-specific declarations. Intra-app module ownership is a review rule.

## Receiver Invariants

- The Receiver owns ordinary `new_receiver` and `add_sender` enrollment.
  R authorizes add-Sender creation and delivery WSS. B authorizes the pairing
  WSS, Receiver completion, cancellation and first-R redemption where
  applicable. New-Receiver creation is anonymous. The `complete` request
  carries confirmation status. QR, challenge and key material stay in the
  approved client handshake framing.
- Keep provisional keys separate until the Service reports activation.
  Preserve existing Pairings during add-Sender, failure and duplicate-target
  handling. Keep `credential_ready`, `credential_issued` and first-delivery
  `completed` distinct. Persist redemption operation ID and attempt before the
  request and R before its first use. Allow attempt 1 only for an uncertain
  attempt 0 under the approved grant rules. Classify a terminal pairing state
  followed by Close `1008` by the terminal state. Leaving the pairing screen
  leaves enrollment running. Explicit refresh starts a fresh five-minute QR.
- The delivery connection owner captures arrival time/order, extracts usable
  correlation and attempts the arrival Receipt before decryption or business
  validation. Select the Pairing key by Pairing ID. Use connection generations
  to discard stale work. Handle duplicate-upgrade HTTP 409 without takeover
  and apply approved jittered reconnect delays of about 1, 2, 4, 8, 16 and
  30 seconds. Pause during suspend and check on resume. Follow Service
  Ping/Pong and Close semantics. Expose unsupported wire versions/types as
  ordinary receiver errors. Delivery is live only.
- Use the shared HPKE PSK P-256/HKDF-SHA256/AES-256-GCM profile and typed
  confirmation, and the approved AES-GCM message framing with fresh 96-bit
  nonces, 128-bit tags, padded Base64 and empty AAD. `hpke-rs` with its
  RustCrypto backend and `aes-gcm` are candidates. Require protocol vectors
  for the chosen versions, including the exporter and message framing.
- The required `otp: ""` is valid and displays the original SMS. Only a
  nonempty OTP participates in the latest cache. Compare parsed `receivedAt`
  (or pre-decryption arrival time) and break ties by arrival order. Popup copy
  uses its own exact code and dismisses that popup. Shortcut copy uses the
  current latest code. Keep both outcomes silent.
- UI owns the approved layout and controls. The Windows adapter supplies
  per-monitor work area and no-focus window behavior. The popup owner enforces
  the five-popup and work-area limits. Lock closes popups and suppresses new
  ones while still allowing live processing. Tray startup stays quiet.
  `pairing_removed` clears only the affected Pairing, popups and latest OTP
  when applicable. `device_revoked` clears Receiver state and blocks stale
  callbacks from restoring it. OS clipboard contents follow their own lifetime.

## Service Contracts

Use the [Service `v1.0.2` release][service] at source commit
`fb134954164acfc360816c36beeaec04863958ab` as the initial integration pin.
SHA-256 for `openapi.yaml` is
`238d7f568370daf23250b38ea762d9d1dd8d299a457f1e51932ee85cf2943d19`.
For `asyncapi.yaml` it is
`ce5ac6ddc9cdd8a8a76fafc9e04c20830fc914b6c77a7882c4f3d9ede162a5d3`.
On first integration, put these values and generator pins in
`scripts/contracts/service.lock.json` and use clean staging.

1. Download both files with `gh release download v1.0.2 --repo BuddyLabsCo/pin-glide-service --pattern openapi.yaml --pattern asyncapi.yaml --dir <staging>`.
   Verify the pin, both digests, resolvable references and HTTP `/v1` and
   WSS `pin-glide.v1` coherence.
2. Run OpenAPI Generator `7.27.0` `validate -i <openapi.yaml>`, then
   `generate -g rust --library reqwest -i <openapi.yaml> -c scripts/contracts/openapi-rust.json -o <staging>/http-client`.
   Configure async HTTP and `reqwestDefaultFeatures: native-tls`.
3. Install locked generator dependencies with `npm ci --prefix scripts/contracts`.
   Have `scripts/contracts/generate-wss.mjs` resolve AsyncAPI with
   `@asyncapi/parser@3.6.6` and pass each named payload to Modelina
   `5.10.1`'s Rust generator. The full AsyncAPI document produced empty model
   output in the probe. Keep the Serde configuration and any normalization
   in generator inputs. The app decoder validates type and version and owns
   the WSS runtime. Tokio with `tokio-tungstenite` and `native-tls` is the
   integration candidate.
4. Commit generated Rust crates and lock workspace dependencies. Upon adding
   them, add a required trusted regeneration job that checks file sets and
   bytes against committed output, compiles generated crates and adapters,
   and runs fixtures for optional fields, error mapping, and wire behavior.
   Ordinary builds use committed output. Only the trusted generation job
   needs read-only access to private download inputs. Validate HTTPS and WSS
   certificate and hostname behavior on MSVC with the actual feature sets.

Preserve omitted `senderId`, `pairingId` and `errors` as optional. Check
present-but-nullable `nickname` and `avatarUrl` in the generated contract.
Keep published recovery schemas without adding a recovery or account-profile
UI. Adapt compatible additions at the wire boundary, handle unfamiliar
business error codes without branching on free-form text, and preserve
protocol failure outcomes. Update the exact pin and both artifacts together
in a reviewed contract upgrade. Keep older Android senders that omit
`receivedAt` compatible. A codegen upgrade and a persisted-format upgrade
have independent versioning and tests.

## Persistent State

Use two versioned files in per-user Local AppData. Settings include language,
launch at login, shortcut and popup duration. Keep the last committed
preferences on save failure and report actual shortcut/autostart state.
Store Receiver ID, Pairing metadata, key references and pending
operation/attempt/phase in the registration record. Protect R, each
active/provisional key and any required restart credential/private material
with current-user DPAPI. Write references and their protected blobs together.

Use one writer, stage a same-directory file, flush, then use a checked
Windows replace operation for existing files. A failed upgrade preserves the
prior committed data. Refuse to overwrite unreadable or unsupported newer
formats and retain the original on migration failure. Read supported older
versions even after skipped releases. For example, an older settings record
without `language` reads as Follow system. A registration reshape that adds a
key reference commits the reference and protected blob together. Downgrade
requires a reader that supports the stored version. Reopen only current,
committed state and never restore an older snapshot that revives cleared
credentials or keys.

Missing R requires explicit new registration. An unreadable Pairing key
affects only that Pairing. Installer upgrades preserve settings and identity.
Uninstall removes app-owned data and native registrations, and reinstall
starts a new Receiver registration. SMS bodies live for the popup lifetime,
and one latest nonempty OTP lives in memory. Log allowlisted outcomes and
keep SMS, OTP, credentials, keys and raw DTO dumps out of diagnostics.

## Static Quality and Verification

Run `node scripts/quality/check.mjs` and
`node --test scripts/quality/check.test.mjs`. Install the Rust toolchain from
`rust-toolchain.toml`. On Windows, `node scripts/quality/install-tools.mjs`
downloads the checksum-pinned `slint-lsp` and `cargo-deny` and prints their
directories for the local `PATH`.
The workflow also runs `node scripts/quality/check.mjs build` on Windows x64
MSVC. Run `cargo run --locked -p windows-app` for the scaffold window.

| Gate | Required behavior |
| --- | --- |
| Rust format | `cargo fmt -p receiver-core -p windows-app --check`. `rustfmt.toml` sets UTF-8 source layout with LF, four spaces and a 100-column formatting target. Run the command without `--check` to fix it. |
| Slint format | Compare `slint-lsp format` stdout with handwritten `.slint` source after CRLF normalization. Run `slint-lsp format --inline <path>` to fix it. |
| File size | Limit handwritten `.rs` and `.slint` to 500 physical lines, including Rust tests and `build.rs`. |
| Rust lint | Default Clippy and Rust warnings fail. `Cargo.toml` additionally denies `dbg_macro`, `await_holding_lock`, `undocumented_unsafe_blocks` and `cognitive_complexity`. `clippy.toml` sets 25. Review narrowly scoped exceptions with a reason. |
| Dependency direction | Inspect Cargo metadata for declared dependency edges, including optional, renamed, build, test and target-specific dependencies. |
| UI diagnostics | Compile the real app Slint tree in `build.rs` with `SLINT_COMPILER_DENY_WARNINGS=1` in quality/build scripts. |
| Dependency advisories | Run `cargo deny --locked check advisories` in CI. Review any time-limited advisory exception in `deny.toml`. |
| Build and tests | On Windows, check all workspace targets with `--locked` and link the MSVC release binary. On Linux, check and test the portable core without the app. Generated code is built and checked for drift when present. |

Handwritten source gates cover `crates/receiver-core/` and
`crates/windows-app/`. Keep generated and vendored sources outside those
trees. The `docs/prototypes/` archive is reference material. CI runs the same
script entry points as local development and keeps source gates, tests, and
linking visible as separate jobs or steps.

When adding a message rule or a pairing transition, test through the core
interface. For network changes, test arrival Receipt on decrypt failure,
duplicate upgrade, stale Close, revocation, credential-specific requests and
terminal Close ordering. For storage changes, test against old, interrupted
and unreadable data. Review that UI changes do not duplicate domain state,
transport does not own clipboard behavior, and contract or storage changes
update their fixtures. Update this document when ownership, crate edges,
quality commands or persistence policy change.

[functional]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6875824783
[time]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6879752448
[ui]: https://github.com/BuddyLabsCo/pin-glide-windows/issues/3#issuecomment-7076999601
[service]: https://github.com/BuddyLabsCo/pin-glide-service/releases/tag/v1.0.2
