# Pin Glide Windows UI prototype

The approved design uses compact windows with fixed logical sizes, a single
light appearance, and the original Pin Glide logo. The product supports
Chinese and English and follows the system language by default. The browser
prototype uses Chinese.

Open [index.html](index.html) directly in a browser. It opens the animated
welcome screen. Choose Start to enter pairing. Each scenario has its own HTML
page.

The [screenshot archive](../shots/README.md) shows every gallery state and
screens that appear after interacting with the prototype.

| Page | Boundary |
| --- | --- |
| [welcome.html](welcome.html) | First launch: SMS delivery animation and Start |
| [pairing.html](pairing.html) | First pairing: QR, confirmation, first connection, completion |
| [main.html](main.html) | After pairing: device list, pairing management and registration recovery |
| [Add a device](add-phone.html) | Add a device: QR, expiry and completion |
| [settings.html](settings.html) | Language, startup, shortcut, popup duration, field feedback |
| [messages.html](messages.html) | SMS popups with or without a code, scrolling, and copying |
| [tray.html](tray.html) | Native tray menu with Open Pin Glide, Settings, and Exit |

Use the state selector and the links between workflow screens. A state is
shareable as `pairing.html?state=pair-issued`, for example. The HTML welcome
preview plays a 6.4-second sequence rendered from the Slint illustration
and shows the arrival frame when the browser requests reduced motion.
The preview advances from preparation to
the QR after a short delay. The real flow advances when session preparation
completes. The completion screens accept a device name and show the saved
name in the device list and SMS source. The browser preview keeps these names
for the current tab session.

The [UI inventory](ui-inventory.md) describes the approved screens and interactions.

## Native source and regeneration

Regeneration requires `slint-viewer` (preview version: 1.18.1), Python and
FFmpeg with `libwebp_anim` to encode the welcome preview. Opening the
generated HTML requires only a browser.
No Python package or frontend dependency is needed.

```powershell
python docs/prototypes/ui-tool/build-preview.py
```

The command compiles the source with `--style fluent`, renders every state and
asserts each output matches the prototype's fixed logical dimensions. It writes
`shots/` and a separate state-data file for each feature under `data/`.
The HTML files are edited directly.

Edit each feature's HTML for its layout. Shared preview
styles live in `assets/preview.css`; shared state navigation/rendering lives
in `assets/gallery.js`. Product-surface artwork comes from `ui.slint`, with
scenario inputs defined in `build-preview.py`. No file-loader server, bundler
or frontend dependency is required. The pages work via `file://`.

To inspect a real fixed-size Slint window interactively:

```powershell
slint-viewer --style fluent --component MainWindow docs/prototypes/ui-tool/ui.slint
```

Use `--component WelcomeWindow`, `PairingWindow`, `SettingsWindow` or `SmsWindow`
for the other surfaces. The browser SMS preview supports scrolling, click to
dismiss and copying the sample code. The Slint callbacks are ready for a host
to wire to the application.

## Sources and limits

The logo is the original Pin Glide launcher asset, copied byte for byte from
`4ee4dd42f522bb56cd54c20e98247f93e276192c`, blob
`a6ae2d7b6ee4327462a6b91f0ce4448a86384cf3`. It is not a generated replacement.
`qr-placeholder.svg` is deliberately non-scannable.

App controls use Fluent standard widgets except for the welcome action.
That action uses a compact, left-aligned Slint control with the app's accent
color, a 4 px radius and visible hover, press and focus states. The welcome
illustration is drawn in `welcome.slint`. Its timeline drives both the live
Slint preview and the generated animation. The production window host should
set `motion-enabled` to match Windows Animation effects. The prototype uses
one light appearance and standard window chrome.

Native OS caption buttons are absent from headless component screenshots;
the browser gallery adds a schematic caption with no maximize button. The
production Windows adapter must still enforce non-resize/non-maximize,
hide-to-tray, independent non-activating SMS windows and taskbar policy.
The tray page sketches the native context menu. A left single-click is a
no-op, a left double-click opens the main entry, and a right-click opens the
Windows context menu. For a new installation, the main entry opens the welcome
screen. Start creates the first pairing session.

Some screenshots deliberately show progress text rather than a fabricated
percentage. The original
fixed test SMS remains unchanged. Demo UI, state descriptions and errors are
Chinese. The Settings language choices are Follow system (default), Chinese
and English; the latter does not introduce English demo screens. Real SMS/OTP
must never be translated when the application language changes.

Compilation, native render dimensions and browser-catalogue navigation were
checked across the separate scenario pages.
Focus/maximize/tray/clipboard/hotkeys, real pairing/crypto/reception,
installer behavior, accessibility automation, product E2E, resource measurement
and acceptance were not validated.
