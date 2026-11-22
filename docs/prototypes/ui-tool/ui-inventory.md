# Pin Glide Windows UI inventory

This inventory describes the approved Windows UI prototype. The
[screenshot archive](../shots/README.md) includes the interaction states as
well as the gallery presets.

| Surface | Fixed logical content size | Entry and layout |
| --- | --- | --- |
| [Welcome](welcome.html) | 440 × 560 | First explicit open before pairing. The sample SMS travels to the computer. Show the settled frame when motion is reduced. Start begins QR preparation. |
| [First pairing](pairing.html) | 440 × 560 | Page title above a fixed square that holds the QR, progress animation, success check mark, or error icon. Status and actions appear underneath. |
| [Add a device](add-phone.html) | 440 × 560 | Shares the pairing layout and QR error treatments. The arrow to the left of the title goes back to the main window. |
| [Main window](main.html) | 400 × 576 | Pair a new device above the list. The Settings icon sits at the top right. The device list scrolls separately from its heading. |
| [Settings](settings.html) | 420 × 392 | The arrow to the left of the title returns to the main window. |
| [SMS popup](messages.html) | 380 × 320 | Separate borderless window with the app name, message, and source. A code and Copy code appear when the SMS contains a code. |
| [Tray context menu](tray.html) | Native menu | Open Pin Glide, Settings, then a separator and Exit. |

The windows use one light appearance, the original Pin Glide logo, and
standard Windows chrome. They have fixed sizes in logical pixels and scale
with Windows DPI. The main window offers minimize and close. Closing it hides
it to the tray and keeps reception running. SMS popups do not take focus on
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

## SMS and tray

Long SMS bodies scroll vertically inside the popup. Long codes end in an
ellipsis, and Copy code copies the original full value. Clicking anywhere
in the popup dismisses it. Clicking Copy code also dismisses it. Scrolling
keeps it open. The popup shows the device name as the source.

A tray single-click does nothing. A double-click opens Pin Glide. The native
right-click menu can open Pin Glide, open Settings directly, or exit.
