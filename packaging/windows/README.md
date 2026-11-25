# Windows Receiver distribution

The NSIS package installs Pin Glide for the current user on x64 Windows 10
22H2 and Windows 11. It adds a Start menu shortcut and an Apps & features
uninstall entry. Setup leaves the Receiver closed. A clean installation
registers `--startup` for tray-only sign-in. Open **Pin Glide** explicitly to
start Receiver registration.

## Build a signed release

Use a Windows PowerShell session with the Rust MSVC toolchain, NSIS 3.13
(`makensis.exe` on `PATH`), and Windows SDK SignTool. The release certificate
and its private key must be accessible through the signing user's Personal
(`CurrentUser\My`) certificate store.

```powershell
$env:PINGLIDE_SERVICE_URL = 'https://service.example.com'
$env:PINGLIDE_SIGNING_THUMBPRINT = '<release certificate thumbprint>'
$env:PINGLIDE_TIMESTAMP_URL = '<RFC 3161 timestamp endpoint>'
.\packaging\windows\build.ps1
```

Pass `-SignTool` with the full SDK `signtool.exe` path if it is absent from
`PATH`. The build signs and verifies the Cargo Release binary, embedded
uninstaller, and outer installer with SHA-256 and an RFC 3161 timestamp.
The Receiver statically links the MSVC runtime for a standalone install.
Distribute `target/packaging/windows/Pin-Glide-Setup-x64.exe` as the release
download. For local or manual development packaging, `-Unsigned` emits a
separately named `Pin-Glide-Setup-x64-unsigned.exe`.

Both installers take the product and file version and the Apps & features
displayed version from the `windows-app` Cargo package version. The numeric
Windows version resource uses the major, minor and patch numbers followed by
zero; the displayed version retains the full Cargo version.

## Build a development package

Create a GitHub Environment named `development` in the repository settings.
Set its environment variable `PINGLIDE_SERVICE_URL` to the development Service
origin (HTTP or HTTPS, with no credentials, non-root path, query or fragment).
Define this variable at the Environment level only, not at the repository or
organization level. The workflow checks the variable outside any Environment
first and fails if a broader-scope value is configured, even if an Environment
value also exists. This prevents silently falling back to another Service.
In Actions, select **Development packaging**, choose **Run workflow**, and
select the branch to package. Download the `Pin-Glide-Setup-x64-unsigned`
artifact containing `Pin-Glide-Setup-x64-unsigned.exe`. This workflow does not
use signing configuration or publish a Release.

For a local equivalent, use:

```powershell
$env:PINGLIDE_SERVICE_URL = 'http://localhost:49354'
.\packaging\windows\build.ps1 -Unsigned -DevelopmentTransport
```

`-DevelopmentTransport` explicitly enables the compile-time
`development-transport` Cargo feature. Unsigned packaging alone does not
enable it, and signed packaging always requires HTTPS. There is no runtime
override. The Cargo build validates the configured origin with the same URL
parser and transport policy as the Receiver. HTTPS development origins still
use TLS with native certificate and hostname verification; redirects remain
disabled.

HTTP and WS send Service credentials and requests in cleartext. Anyone able
to observe or modify that traffic may capture credentials or tamper with
requests and events. Use only development credentials and data on an isolated
network or in a VM. Development and production packages share the same
application identity, installation and persistent state; use separate
Windows profiles or VMs to avoid replacing a production installation or
reusing its credentials.

## Production release

Create a GitHub Environment named `production`, restricted to `main` with
no additional required reviewer gate. Set its Environment Variable
`PINGLIDE_SERVICE_URL` to the production HTTPS origin. Optionally configure
both `PINGLIDE_SIGNING_THUMBPRINT` and `PINGLIDE_TIMESTAMP_URL` for signing.
Define all three variables only at the Environment level. A partial or invalid
signing configuration fails; only two absent signing variables select unsigned
packaging. Signed builds also require SignTool on PATH and the matching
certificate with private-key access in the runner user's Personal store.
The workflow does not provision signing credentials or fall back to unsigned
on signing failure.

Merging a same-repository `auto-release` PR into `main` authorizes the
**Production release** workflow. It checks out the exact merge SHA,
builds with development transport explicitly disabled, and verifies the
distributable's version and signature status and the isolated installer
lifecycle. The lifecycle test builds isolated installers from the Receiver;
it does not install the actual distributable or contact the live Service.
The signed build additionally verifies the signed Receiver, embedded
uninstaller and outer installer during packaging.

The authorized commit must contain a stable `windows-app` Cargo package
version and a matching nonempty `## <version>` section in `CHANGELOG.md`
(the Herdr convention). The workflow publishes that section as Release Notes,
creates `v<version>` at the authorized merge SHA, and uploads the verified
standalone installer to a draft Release of the same name. It makes the
Release public only after checking the uploaded asset's SHA-256 digest.
The Release Notes also carry a hidden verified-candidate identity for recovery;
do not edit this record or move the tag.

The configuration job needs `contents: read`; the production job needs
`contents: write` for tags and Releases and `actions: read` for downloading
the original candidate. It uses the scoped `GITHUB_TOKEN`, not a separate
PAT or GitHub App token. Repository tag rules must permit the authorized job
to create the release tag. Production jobs share a concurrency queue, so
publication runs one at a time without cancelling an active release.

### Recover a failed production release

1. In Actions, open the original **Production release** run and rerun its
   failed jobs. Do not merge another Release PR just to retry. The rerun
   uses the original merge commit and run ID, not the current `main` head.
2. The `production-release-candidate` artifact contains the installer and
   `release-candidate.json` with the authorized commit, Cargo version,
   signing mode, filename and SHA-256 digest. Retain it until publication
   succeeds. A retry validates and reuses this artifact, without rebuilding
   or resigning the installer.
3. A matching lightweight or annotated tag, draft and uploaded installer
   can be resumed. Compare an uploaded draft with the original verified
   candidate whenever that artifact is available; otherwise its recorded
   identity and uploaded digest must agree. An empty failed-upload placeholder
   in a matching draft can be retried. An already complete Release is a no-op
   without accessing Actions artifacts, even after the artifact expires.
4. If the candidate is unavailable before any draft exists, the original
   run can build and verify again. If an incomplete draft records a candidate
   but lacks the uploaded installer and the artifact is unavailable, stop:
   recover the original verified artifact or resolve the draft deliberately.
   Conflicting tags, Release names, notes, manifests or asset names/digests
   fail without overwriting them. Investigate the conflict before retrying;
   never move a published tag or replace its published installer.

Run the release policy and publication tests locally:

```powershell
.\packaging\windows\production.test.ps1
node --test packaging/windows/production-publication.test.mjs
```

## Maintenance and verification

Choose **Exit** in the tray before upgrading or uninstalling. Run the new
installer over the existing installation to upgrade. Setup replaces the app
in `%LOCALAPPDATA%\Programs\Pin Glide` and leaves the settings and Receiver
registration under `%LOCALAPPDATA%\Pin Glide` intact, including after a
failed upgrade. Setup updates an existing Pin Glide login entry to the
installed executable and preserves disabled startup and Windows Startup Apps
approval. Uninstall removes the app-owned flat data directory, Start menu
shortcut, and Windows registrations. Clipboard contents and Android
associations stay in place. Open Pin Glide after reinstalling to begin a
new registration.

Run the isolated installer lifecycle test after building:

```powershell
.\packaging\windows\installer.test.ps1 `
  -Receiver .\target\x86_64-pc-windows-msvc\release\windows-app.exe `
  -Installer .\target\packaging\windows\Pin-Glide-Setup-x64-unsigned.exe
```

The test verifies the actual installer's version resources against Cargo
metadata, then uses synthetic state and unique file paths and registry names.
It exercises rejected packaging origins, version metadata, first install, a
running Receiver's instance guard, rollback after a blocked binary replacement
or unreadable registry metadata, startup migration, uninstall retry, and clean
reinstall.

### Packaged first use

Run this desktop test in a disposable Windows profile with an unlocked
desktop and no existing Pin Glide installation or data. The test uses
the production installer paths and real current-user startup registration.
Trust the loopback fixture CA only in that profile, then discard the profile
after testing.

```powershell
certutil -user -addstore Root .\crates\windows-app\tests\fixtures\service\ca.pem
$env:PINGLIDE_SERVICE_URL = 'https://localhost:49354'
.\packaging\windows\build.ps1 -Unsigned
$env:PINGLIDE_DISPOSABLE_PROFILE = '1'
cargo test --locked -p windows-app --test lifecycle --target x86_64-pc-windows-msvc `
  packaged_reinstall_enrolls_only_after_explicit_start -- --ignored
```

The test uninstalls seeded local state, reinstalls the packaged Release
Receiver, and verifies tray-only startup and a fresh anonymous `new_receiver`
request to the local HTTPS fixture after selecting **Get started**.
