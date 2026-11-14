# Windows UI style prototype

Historical A/B/C comparison. The user selected B on November 14, 2026 and
requested fixed-size, non-maximizable windows with one light appearance.
The current native Slint state study is [Compact native utility UI](../ui-tool/README.md).
The old dark previews below are historical alternatives, not current scope.

Throwaway visual directions for [Clarify Windows UI requirements after functional agreement](https://github.com/BuddyLabsCo/pin-glide-windows/issues/3).
No direction has been approved. This does not resolve the UI ticket.

## Run

Open `index.html` directly, or run from the repository root:

```powershell
python -m http.server 4317 --bind 127.0.0.1 --directory docs/prototypes/ui-style
```

Open `http://127.0.0.1:4317/?variant=A`. `B` and `C` select the other directions.
The bottom bar and left/right arrow keys switch directions. The theme button
previews both light and dark for every direction; `&theme=dark` is shareable.

## Directions

| Direction | Main window | SMS popup | Pairing |
| --- | --- | --- | --- |
| A / Native Fluent | Left navigation, standard settings density | Inline code and copy button | Centered QR |
| B / Quiet utility | Compact single column with inline preferences | Large code and full-width copy | Side-by-side instructions and QR |
| C / Status workspace | Horizontal navigation, split status and preferences | Vertical code/action/text hierarchy | Minimal centered pairing |

Each board shows a paired main window, an independent SMS popup, and a separate
add-Sender pairing window. First-use ordering remains pairing before optional
settings; the visible board is a comparison of separate surfaces, not a workflow
that shows all surfaces automatically.

The app name comes from the UI issue. Its approved icon attachment was
inaccessible during preview, so a temporary P monogram marks the icon position.
This is not a replacement icon proposal. UI chrome uses English preview copy;
the fixed test SMS preserves its exact approved Chinese text. Localization and
final wording remain undecided. The artifact needs no network access.

## Scope and behavior

The approved functional resolution of November 14, 2026 remains authoritative.
The variants deliberately share its behavior:

- Ready / Not Ready only; reconnecting is an explanatory reason.
- Two example paired phones; no assumed Sender presence or SMS history.
- Autostart initially on; copy shortcut initially unassigned.
- Popup duration initially 30 seconds, configurable for new popups.
- Success closes only the clicked popup, with no success confirmation.
- No-OTP SMS shows the full sample text and hides the copy action as a proposal.
- No-OTP SMS does not replace the latest nonempty OTP.
- Failed copy retains the popup and shows an error.
- QR expiry offers explicit refresh; QR does not rotate automatically.
- Pairing-window closure does not cancel pairing.
- Main-window closure hides to tray.

Controls change in-memory preview state only. The scenario selector previews
no-OTP, reconnecting, QR expiry, copy failure, and an isolated missing key.
The shortcut control toggles one sample chord and clear; this is not a proposed
shortcut-capture interaction or approved default.

The QR is deliberately non-scannable placeholder artwork. Timers are static.
No connection, real SMS, credentials, clipboard, disk data, system settings,
Slint runtime, tray integration, or real pairing is involved.

## Still open

User selection, exact sizes and placement, stacking and overflow, focus policy
implementation, first-use/progress/error screens, tray menu, theme policy,
localization, keyboard/accessibility requirements, and final UI specification.
The snapshots describe a visual direction, not complete screen coverage.

The map requests Impeccable, but that skill was not present in the available
catalog or searched local skill directories during this session. Prototype and
Slint/Fluent constraints were used without claiming an Impeccable review.

## Preview verification

The three light directions and the dark palette were rendered and visually
inspected at 1440 x 1180. `self-check.js` passed 50 assertions in the rendered
prototype, covering variant URLs/wrapping, defaults, fixed SMS preservation,
no-OTP controls, copy success/failure, QR refresh, readiness, duration changes,
theme switching and hiding/reopening the main window. These are prototype
checks, not product E2E or Windows integration validation.
