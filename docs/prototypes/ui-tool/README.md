# Compact native utility UI study

This continues the B direction selected by the user on November 14, 2026.
The user also selected fixed-size, non-maximizable windows and no theme/dark
appearance in the current scope. The other presentation details remain draft.

Open [index.html](index.html) directly in a browser. It contains overview and
navigation only. Each functional boundary has its own independently editable
HTML page; each page searches and cycles only its own states:

| Page | Boundary |
| --- | --- |
| [main.html](main.html) | Receiver readiness, phones, credential/key errors |
| [pairing.html](pairing.html) | QR, enrollment progress/errors, new registration |
| [settings.html](settings.html) | Startup, shortcut, popup duration, save errors |
| [messages.html](messages.html) | Full SMS, OTP/no-OTP, clipboard and latest-code feedback |
| [tray.html](tray.html) | System-tray icon/tooltip/menu proposals |
| [lifecycle.html](lifecycle.html) | Automatic launch, lock, suspend, Exit |
| [installation.html](installation.html) | NSIS and Windows-owned installation/trust UI |

Use the page's state list, search or arrow keys. A state is shareable as
`pairing.html?state=pair-issued`, for example. Native screenshots are static
Slint renders, not a proposed browser-based application.

Read [ui-inventory.md](ui-inventory.md) for the complete trigger/action/error
inventory, including tray, installer and deliberately invisible outcomes.
There are 62 catalogue states, of which 51 have native Slint screenshots.
The inventory also enumerates subcases not requiring distinct artwork.

## Native source and regeneration

Requires the already-installed `slint-viewer` (preview version: 1.18.1) and Python.
No Python package or frontend dependency is needed.

```powershell
python docs/prototypes/ui-tool/build-preview.py
```

The command compiles the source with `--style fluent`, renders every state and
asserts each output has the proposed fixed logical dimensions. It writes
`shots/` and a separate state-data file for each feature under `data/`.
It does not overwrite the feature HTML pages or overview.

Edit each feature's HTML for its layout and discussion notes. Shared preview
styles live in `assets/preview.css`; shared state navigation/rendering lives
in `assets/gallery.js`. Product-surface artwork comes from `ui.slint`, with
scenario inputs defined in `build-preview.py`. No file-loader server, bundler
or frontend dependency is required; all eight HTML files work via `file://`.

To inspect a real fixed-size Slint window interactively:

```powershell
slint-viewer --style fluent --component MainWindow docs/prototypes/ui-tool/ui.slint
```

Use `--component PairingWindow`, `SettingsWindow`, `SmsWindow`,
`FeedbackWindow`, `ConfirmationWindow` or `TrayContentPreview` for the other
surfaces. Their callbacks intentionally do not call product actions. This is a
visual/state prototype, not functional receiver implementation.

## Sources and limits

The logo is the byte-for-byte Android launcher asset at source
`4ee4dd42f522bb56cd54c20e98247f93e276192c`, blob
`a6ae2d7b6ee4327462a6b91f0ce4448a86384cf3`. It is not a generated replacement.
`qr-placeholder.svg` is deliberately non-scannable.

App controls use real Fluent standard widgets; status, labels, wrapped text
and scroll regions are ordinary Slint elements. There is no Mica, Acrylic,
custom native caption, WebView or required animation. Light appearance is
forced for these prototype windows; no theme preference is added.

Native OS caption buttons are absent from headless component screenshots;
the browser gallery adds a schematic caption with no maximize button. The
production Windows adapter must still enforce non-resize/non-maximize,
hide-to-tray, independent non-activating SMS windows and taskbar policy.
The tray component is only a content sketch: real tray icons/tooltips/menus
must be implemented with Windows APIs, and the OS controls their appearance.

Some screenshots deliberately show progress text rather than a fabricated
percentage. Quiet states render no invented application dialog. The original
fixed test SMS remains unchanged; UI chrome is English pending a language
decision.

Compilation, native render dimensions and browser-catalogue navigation were
checked across the separate feature pages (263 assertions across all seven
pages; this is not a product test suite).
Focus/maximize/tray/clipboard/hotkeys, real pairing/crypto/reception,
installer behavior, accessibility automation, product E2E, resource measurement
and acceptance were not validated. The user has excluded the last three
projects from the Wayfinder map.
