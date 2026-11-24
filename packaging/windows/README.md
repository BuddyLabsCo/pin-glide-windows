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
download. For local packaging only, `-Unsigned` emits a separately named
`Pin-Glide-Setup-x64-unsigned.exe`.

## Maintenance and verification

Choose **Exit** in the tray before upgrading or uninstalling. Run the new
installer over the existing installation to upgrade. Setup replaces the app
in `%LOCALAPPDATA%\Programs\Pin Glide` and leaves the settings and Receiver
registration under `%LOCALAPPDATA%\Pin Glide` intact, including after a
failed upgrade. Uninstall removes the local Receiver data, Start menu shortcut,
and Windows registrations. Clipboard contents and Android associations stay
in place. Open Pin Glide after reinstalling to begin a new registration.

Run the isolated installer lifecycle test after building:

```powershell
.\packaging\windows\installer.test.ps1 `
  -Receiver .\target\x86_64-pc-windows-msvc\release\windows-app.exe
```

The test uses synthetic state and unique file paths and registry names. It
exercises first install, a running Receiver's instance guard, a blocked binary
replacement, an upgrade, uninstall, and clean reinstall.
