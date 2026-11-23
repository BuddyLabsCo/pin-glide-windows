# Pin Glide Windows UI inventory

This inventory describes the approved Windows UI prototype. The
[screenshot archive](../shots/README.md) includes the interaction states as
well as the gallery presets.

| Surface | Logical content size | Entry and layout |
| --- | --- | --- |
| [Welcome](welcome.html) | 440 × 560 | First explicit open before pairing. The sample SMS travels to the computer. Show the settled frame when motion is reduced. Start begins QR preparation. |
| [First pairing](pairing.html) | 440 × 560 | Page title above a fixed square that holds the QR, progress animation, success check mark, or error icon. Status and actions appear underneath. |
| [Add a device](add-phone.html) | 440 × 560 | Shares the pairing layout and QR error treatments. The arrow to the left of the title goes back to the main window. |
| [Main window](main.html) | 400 × 576 | Pair a new device above the list. The Settings icon sits at the top right. The device list scrolls separately from its heading. |
| [Settings](settings.html) | 420 × 392 | The arrow to the left of the title returns to the main window. |
| [SMS popup](messages.html) | 380 wide, up to 240 high | Separate borderless window that grows with the message. The code and Copy code share a row when a code is available. |
| [Tray context menu](tray.html) | Native menu | Open Pin Glide, Settings, then a separator and Exit. |

The windows use one light appearance, the original Pin Glide logo, and
standard Windows chrome. Sizes use logical pixels and scale with Windows DPI.
The main, setup, and settings windows have fixed sizes. The main window
offers minimize and close. Closing it hides it to the tray and keeps reception
running. SMS popups do not take focus on
arrival or appear in the taskbar. The archive shows the window chrome as
represented by the browser prototype.

## Pairing

- Preparing shows a pulse in the QR area. Once ready, the QR takes its place
  and shows scan instructions and remaining validity time.
- While the devices pair, the QR area shows progress. First pairing also shows
  receiver preparation, registration outcome checking, and first connection.
- QR preparation failure, pairing failure, expiration, and cancellation share
  the same layout in both flows. Each provides an action to prepare a new QR.
- On completion, the QR area shows a check mark. The device name field and
  Done appear below it. Done with a valid name saves it and opens the main
  window. Adding a device leaves the existing devices available.

## Device list and names

The list shows the names assigned on Windows. Each row expands or collapses
when clicked anywhere. An expanded healthy device shows an inline name editor
with Cancel and Save. Two rows and one expanded editor fit together. Additional
rows scroll within the list while the heading remains visible. Dividers span
the list. A device that needs to pair again instead shows a red Delete button
with white text. Delete removes that device's local entry.

Pairing and renaming use the same checks as the sender. Trim surrounding
whitespace, require 1–40 Unicode code points, reject unpaired surrogates,
and reject a duplicate name without regard to case. During renaming, exclude
the current device from the duplicate check. Done or Save focuses the input
if validation fails and displays the reason beneath it on its own line. The
rename form leaves no empty error row. The saved name appears in the list and
on SMS popups from that device.

The main window also previews an empty list, unavailable local receiver
credentials, and receiver removal. All use the usual **Pair a new device**
action. An isolated device that needs pairing again does not make the other
paired devices unusable.

## Settings

- **Interface language** follows the system by default, with Chinese and
  English choices in the product. The gallery previews the Chinese interface.
- **Start at login** uses a switch. The gallery shows the startup failure
  beside the switch.
- **Copy verification code shortcut** shows the current shortcut, with Change
  and Clear actions. The gallery shows capture, invalid combinations, and
  shortcuts already in use. An invalid or conflicting shortcut leaves the
  previous shortcut in place.
- **Popup duration** defaults to 30 seconds. Leaving the field applies a
  positive whole number of seconds. Invalid input shows an inline error.
  A save failure restores the last saved duration and shows an inline
  error asking the user to retry.

## SMS and tray

The popup grows with the wrapped message. At the height limit, the message
body scrolls while the code, copy action and device name stay in place. When
there is no code, the message starts directly below the app name. Long codes
end in an ellipsis. Copy code attempts to copy the original full value and
dismisses the popup. Clicking anywhere else in the popup also dismisses it.
The popup shows the device name as the source.

Popups stack upward from the lower right, newest at the top. The stack shows
at most five popups. When a new popup cannot fit within the work area or the
five-popup limit, expire the oldest visible popup until the new one fits.
Measure the space using the rendered popup heights and gaps.

Review [popup motion](messages-motion.html) in a browser. A new popup fades in
with a short slide from the right. A closing popup fades out with a short slide
to the right. Popups above the gap slide down to fill it. Popups below the
gap keep their positions. With reduced motion, the demo applies the final
positions immediately.

A tray single-click does nothing. A double-click opens Pin Glide. The native
right-click menu can open Pin Glide, open Settings directly, or exit.
