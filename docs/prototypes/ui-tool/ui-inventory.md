# Windows UI inventory — compact utility direction

Status: discussion draft. The user's B direction, fixed non-maximizable
windows, reuse of the existing logo, one light appearance, and Chinese/English
selection defaulting to system language are approved by the November 14, 2026
UI checkpoints. The demo is Chinese only. The other presentation policies below are
proposals for review, not a resolved UI specification.

Authority: [Windows functional resolution](https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6875824783).
UI decision: [Clarify Windows UI requirements after functional agreement](https://github.com/BuddyLabsCo/pin-glide-windows/issues/3).

## Selected direction and implementation boundary

- A compact tool window and a single welcome page for first launch. The
  welcome animation shows an SMS reaching this computer and the OTP appearing.
- One fixed light appearance. No theme preference, dark appearance, or
  system-theme-following behavior in this scope. This supersedes W11's old
  light/dark language; it does not create a future dark-mode task.
- The original app icon, copied byte-for-byte from
  `app/src/main/res/mipmap-nodpi/ic_launcher.png`, observed at source
  `4ee4dd42f522bb56cd54c20e98247f93e276192c`; blob
  `a6ae2d7b6ee4327462a6b91f0ce4448a86384cf3`.
- Rust + Slint with Fluent standard widgets for app surfaces and a compact
  brand-colored primary action on Welcome. Windows APIs
  through the selected `windows` crate for tray/menu, window styles, placement,
  clipboard, shortcut, startup, and session integration. No WebView/Electron,
  browser CSS effects, Acrylic/Mica requirement, or custom titlebar. The
  welcome illustration uses Slint elements and property animation.
- `ui.slint` is a real, compilable Slint prototype. All app screenshots in the
  gallery are rendered by `slint-viewer 1.18.1 --style fluent`. That is the
  preview tool version, not a production dependency pin.
- Main: 400 x 576 logical content pixels; settings: 420 x 392; welcome/pairing:
  440 x 560; SMS: 380 x 320. Native Windows chrome
  adds its own frame.
  Exact dimensions remain reviewable.
- Fixed size means fixed logical dimensions with normal Windows DPI scaling,
  not a physical-pixel lock. No drag-resize, maximize button, double-click
  maximize, snap-resize, or fullscreen entry. Native window styles must
  enforce the same restriction as Slint layout constraints.
- Main has native minimize and close; close hides to tray. Welcome/pairing/settings
  use ordinary native titlebars without maximize; close dismisses their UI.
  SMS popups are separate borderless tool windows,
  omitted from the taskbar; new arrivals do not activate them.
- Content overflow uses ScrollView/ListView and wrapped text, not a growing
  window. Native screenshots omit OS-drawn titlebars; gallery frames show
  the proposed minimize/close-only contract.
- The application supports Chinese and English. Settings offers Follow system
  (default), Chinese and English. The saved preference represents system/zh/en;
  do not persist a guessed resolved system language as the user's override.
- The demo uses Chinese UI copy, state descriptions, tray text and errors.
  English is a language choice in the product. The gallery previews Follow
  system and Chinese. Keep the original fixed-test SMS unchanged.
- Language selection applies to app-owned controls, status/error text and
  native tray strings. Never translate received SMS, OTPs or device identifiers.
  A failed preference save retains the prior saved language choice.
  Product runtime OS-locale resolution and switching are not implemented by
  this snapshot-based demo. Unsupported-system-language fallback remains a
  detail to specify; Chinese and English system languages have their matching UI.

## Surface catalogue

The demo opens in `welcome.html`. Start opens `pairing.html`. Follow the
visible steps through first connection and completion into `main.html`, which
starts with one paired device. [Add a device](add-phone.html) previews pairing
a second device and returns to the main view with both devices. Settings, SMS and the tray context
menu have separate pages. The inventory retains lifecycle and installer rules.

| Surface | Contents and entry | Closing / lifetime |
| --- | --- | --- |
| Welcome window | First open on a new installation. A notification slides onto the sending device, travels along an arc, and expands into a code card on the computer. Show a settled frame when motion is disabled. Start opens pairing immediately, including during the animation. | Closing before Start leaves setup untouched. Once pairing starts, reopening returns to its current screen. |
| Main utility window | The pairing card and device list heading stay above the scrolling rows. Two rows and one inline name editor fit together. | Close hides to tray; reception continues. Explicit launch/tray Open reuses it. |
| Pairing window | A page title and one fixed square for the QR, waiting animation, success checkmark or error icon. Status and actions sit below it. Show scan instructions and remaining validity while the QR is visible. | The Add device header has a Back button that returns to the device list. Close hides the window. Reopen returns to the same operation. |
| Settings view | Language picker; startup switch; shortcut capture/change/clear; popup duration saved when the field loses focus; field errors | The arrow to the left of the title returns to the main window. Closing does not stop reception. |
| SMS window | Logo/name; OTP with end ellipsis on overflow; Copy code; vertically scrollable SMS body; source label | Clicking the popup dismisses it. Copy code copies the original value and dismisses that popup. Scrolling leaves it open. Timeout or dismissal releases its body. |
| Tray icon + standard tooltip | Unchanged original logo; tooltip contains app name, Ready/Not Ready and a short safe reason | Lives while process runs. No code, SMS, badge count, animation, or synthesized Sender presence. |
| Native tray context menu | Open Pin Glide, Settings, separator, Exit | Windows draws and dismisses it. Keyboard navigation, Enter and Escape use native behavior. |
| Installer/uninstaller | Standard NSIS pages and native Windows trust/UAC/error surfaces | Outside Slint app UI; no additional custom setup framework. |

Any device that can receive SMS and forward it through Pin Glide can act as a
Sender. Use “device” in lists and “target device” in scan instructions.
Keep window lifecycle details in this inventory.

After pairing succeeds, Windows asks for a local display name for the sending
device. The first device starts with “My device”, and the add-device example
starts with “Spare device”. Trim surrounding whitespace. Require 1–40 Unicode
code points, reject unpaired surrogates, and compare against the names of other
locally paired devices without regard to case. Validate on Done or Save.
On error, focus the field and show the reason below it. Rename pre-fills the
current name and excludes the selected device from duplicate checks. Store
the name locally with the sending device ID and use it in the device list and
SMS source label. The successful pairing and SMS reception are independent
of the name. The service authorizes single-pairing removal from the Sender side.

## Tray interactions

The app registers its real logo as the tray icon, with appropriately sized icon
resources for Windows DPI. Keep it the same for Ready and Not Ready. Use the
tooltip to explain state. No flash or recolored-logo
variant is required. Windows and the user control whether it sits in the
visible notification area or its overflow; the app does not promise pinning.

Left single-click does nothing. Left double-click or keyboard activation
opens or activates the main entry. First use opens Welcome. An existing
installation opens the main window. Right single-click opens the native
Windows menu at the tray icon. Open Pin Glide runs the same action as a
double-click. Reopening reuses the existing instance and connection. Settings
opens the settings view.

```text
Open Pin Glide
Settings
----------------------------------
Exit
```

If tray registration fails, keep receiving when otherwise possible. Start menu
launch can reopen the existing app. Explorer restart should restore the icon
quietly. Tray registration failures have no dedicated UI.

## Launch, main-window and lifecycle states

| ID | Trigger / condition | Visible result and action |
| --- | --- | --- |
| L01 | Explicit first launch | Show Welcome. Start begins session preparation and shows the QR when ready. No session or countdown starts on Welcome. |
| L02 | Automatic startup, paired | Tray only; connect quietly. No main window, onboarding, or success message. |
| L03 | Automatic startup, unpaired | Tray only, Not Ready reason. Explicit Open shows Welcome before setup starts or resumes the current pairing flow. |
| L04 | Explicit later launch or tray Open | Open/activate the existing fixed-size main surface. |
| L05 | Another explicit executable launch | Activate existing UI; no duplicate-process alert in the ordinary case. |
| L06 | Main/settings close | Hide utility UI; no "still running" toast every time. |
| L07 | Pairing close/reopen | Hidden pairing continues; reopen identical live QR/progress/error. No cancellation. |
| L08 | Tray Exit | Disconnect and end process. No confirmation proposed; volatile SMS/OTP is released, settings and keys retained. |
| L09 | Windows session lock | Existing SMS windows close. No new SMS windows, retained bodies, or replay on unlock. |
| L10 | Unlock | Remain tray-only unless UI was explicitly requested; no old-popup replay. Latest nonempty OTP can still be copied. |
| L11 | Suspend/hibernate | No UI; retry activity pauses. |
| L12 | Resume | Quiet connection check/reconnect; no activation or "welcome back" popup. |
| L13 | Ready | Show the paired device list and the action to pair a new device. |
| L14 | Connecting / transport retry | Retry quietly. The tray continues to report readiness. |
| L15 | Network unavailable / TLS / server error | Follow the automatic reconnect policy in the background. |
| L16 | Exclusive connection conflict | Continue retrying in the background. |
| L17 | Receiver credential absent/unreadable | Show the local-data explanation and stop connection attempts. Pair a new device opens the QR flow that creates a new Receiver. |
| L18 | One Windows-local Pairing key unreadable | Mark that device as needing to pair again. Its expanded row offers Delete to remove its local record. Remove the old association on the sending device before pairing again. Other Pairings continue. |
| L19 | All Pairings removed, valid Receiver retained | Show an empty device list and the usual Pair action. Reuse the existing Receiver. |
| L20 | Authoritative Receiver revocation | Show the revoked reason, clear credentials and volatile state as required, and preserve settings. Pair a new device opens the QR flow that creates a new Receiver. |
| L21 | Unsupported protocol/type / unexpected closure | Follow the existing connection retry policy in the background. |
| L22 | Invalid/authentication-failed SMS | Discard the invalid message quietly. Preserve the latest valid OTP and continue later deliveries. |
| L23 | Authoritative Pairing removal | That row disappears; only its popups close; cache clears only if owned by it. Other reception continues; no fabricated Receiver revocation. |
| L24 | Sender logout / Account switch without removal | No inferred Windows row deletion or security modal. |
| L25 | Tray icon registration unavailable | Explicit Start menu launch reopens the existing app. |

## Pairing: every meaningful progress and error

The UI shows human descriptions, never B/R/M, private material, operation IDs,
cryptographic challenges, or credential strings.

| ID | Internal condition | UI proposal / allowed action |
| --- | --- | --- |
| P01 | Creating ordinary Pairing Session | A soft pulse in the QR frame and “Preparing QR code…” below. Use a still frame when motion is disabled. The QR replaces the animation in place when ready. |
| P02 | QR waiting | QR with instructions to open Pin Glide on the target device, scan, and confirm. Show the remaining time of the five-minute validity window. No automatic rotation. |
| P03 | QR merely scanned, no authoritative event | Keep waiting. Any feedback from the scan itself appears on the target device. |
| P04 | Authorized Sender / handshake relay begins | Replace the QR in place with the waiting animation and show “Pairing…” below. |
| P05 | Local key confirmation in progress | Keep the animation and “Pairing…”. |
| P06 | Saving protected local key / pending state | Keep “Pairing…”. Advance after the protected write succeeds. |
| P07 | Waiting for other completion report / activation | Keep the animation and “Pairing…” until the pairing result is known. |
| P08 | credential_ready | "Preparing this receiver..." Do not say complete or show a copyable credential. |
| P09 | Redemption request outstanding / uncertain response | "Checking registration result..." Keep the same operation; no unrestricted Retry redemption or destructive restart. |
| P10 | credential_issued, R persisted, first delivery connection pending | "Connecting this receiver..." Still Not Ready until the real delivery readiness condition is met. |
| P11 | First delivery upgrade completes new enrollment | Ready. If visible, replace the animation with a checkmark, “Pairing complete”, and the device name field. Done saves the valid name and opens the main window. If hidden, update state quietly. |
| P12 | add-Sender succeeds | Show the same name field for the new device. Done saves the name and returns to the device list. Existing Receiver readiness and connection remain independent of this operation. |
| P13 | add-Sender fails | Show the shared pairing failure screen. Preserve existing keys and connections. |
| P14 | Five minutes elapsed | Show “QR code expired” and Refresh QR code. Refresh starts a fresh session. |
| P15 | Session creation network/service failure | Show “Could not prepare the pairing QR code” and Refresh QR code. |
| P16 | Pairing channel transport interrupted | Keep the current scan or waiting screen and check the existing session in the background. Retain its original expiry. Change the screen when the session advances, expires, or is confirmed to have ended. |
| P17 | Local key confirmation/authentication failure | Show the shared failure screen once the attempt has ended. Preserve active Pairings. |
| P18 | Protected write fails | Show the shared failure screen once the attempt has ended. Preserve existing local data and follow the operation recovery rules. |
| P19 | Authoritative canceled / superseded | Show “Pairing canceled” for confirmed cancellation and the shared failure screen for a superseded session. Both offer Refresh QR code. |
| P20 | Expected terminal state then policy Close 1008 | Keep its completed/canceled/expired outcome; no "connection failed" replacement error. |
| P21 | Policy Close 1008 without a known terminal state | Check the session. Show the shared failure screen if it cannot continue. |
| P22 | Sender authorization request rejected | The device making the request handles the error. Windows follows the session state it receives and keeps the QR visible while the session awaits authorization. |
| P23 | Already-paired target scan | Preserve keys, alias, Pairing and preferences. Show Already paired only if the Receiver gets that result. Otherwise keep the session waiting for a result and let the target device show the message. |
| P24 | Pairing completes/fails while window hidden | No forced reopening or success toast. Reopen shows the resulting status. |
| P25 | Reopening a live session | Same QR and progress, no new session or timer reset. |
| P26 | Pairing after unreadable or revoked Receiver credentials | Pair a new device opens the QR flow for a new Receiver. Previously paired devices must pair again with the new Receiver. Failure preserves remaining old local data. |

The shared failure screen says “Pairing failed. Please try again.” and offers
“Refresh QR code”. Refresh starts a new session after resolving the previous
attempt under the existing redemption rules. An attempt with an uncertain
result stays on the progress screen while its outcome is checked.

## SMS popup, full text and latest-code actions

| ID | Situation | UI / behavior |
| --- | --- | --- |
| M01 | Valid nonempty OTP | OTP, Copy code button, full unchanged SMS, source fallback. |
| M02 | Valid empty OTP | Full unchanged SMS under New message; hide Copy code rather than display a disabled primary action. This is a presentation proposal. |
| M03 | Long/multiline SMS | Fixed-size popup with wrapped original text and vertical scrolling; no truncation of the stored/displayable body or whitespace rewriting. |
| M04 | Long/non-numeric OTP | Show an ellipsis at the end when the code exceeds the available width. Copy the complete original string. |
| M05 | Copy code clicked | Attempt to copy the original value and dismiss that message's popup. |
| M07 | Default/configured timeout | Close only that popup; release its body. Each popup owns its lifetime; a settings change applies only to new popups. |
| M08 | Click on popup | Dismiss that popup and release its body. The latest nonempty OTP remains available for the copy shortcut. |
| M09 | Multiple arrivals | Independent app windows; no replacement of older messages and no history centre. New arrivals never take focus. Proposed placement/overflow below. |
| M10 | Older message finishes decrypting later | Its popup still appears, but latest-code action follows approved timestamp/arrival ordering, not visible topmost popup order. |
| M11 | No-OTP arrival after a previous OTP | Full new SMS is shown; latest-code action still copies the prior valid latest value. No empty-value feedback is emitted merely because the new SMS has no OTP. |
| M12 | Latest shortcut succeeds | Clipboard write only; no popup/confirmation and no automatic OS clipboard clearing. |
| M13 | Latest shortcut with no code | Clipboard unchanged. |
| M14 | Latest shortcut clipboard failure | Code remains available for another attempt. |
| M15 | Pairing-scoped removal | Close its popups and clear latest cache only if owned by it, without fallback to older history. |
| M16 | Receiver revocation / process exit / lock | Apply the already-approved scoped privacy cleanup; no old-popup replay afterward. Lock does not clear the latest OTP under the approved functional policy. |
| M17 | Invalid decrypted shape/authentication | Discard the message quietly. |
| M18 | Fixed test SMS from the Sender | Use the ordinary SMS path and UI. |

Proposed placement: primary-monitor work area, bottom right, 12 logical pixels
from the edge and 8 between independent windows; reserve actual taskbar space.
Older popups move upward, then into a column to the left. Do not move keyboard
focus when placing/repositioning. The initial monitor policy intentionally
avoids a cursor-following or monitor-picker setting.

Finite-screen overflow is **not resolved**. Once all fitting cells are used,
the least additional UI is a small cascade overlap of still-independent live
windows, without replacing, queueing, silently discarding, or adding history.
Whether that overlap is acceptable needs the user's live reaction. Stacking
limits and timer-on-hover behavior remain open.

Fixed logical sizes still require clamping/repositioning after DPI/display
changes and keeping the copy control reachable. If a work area is smaller
than a proposed window at the chosen DPI, do not silently resize against the
user's fixed-size requirement. Exact size selection for the minimum supported
work area remains a UI decision.

## Settings and integration operation states

| ID | Condition | UI proposal |
| --- | --- | --- |
| S01 | Default settings after first pairing | Startup ON; shortcut Unassigned; popup duration 30 seconds. No theme/cache-duration controls. |
| S02 | Shortcut capture | Read-only capture field + instructions. Escape cancels; Enter applies a complete supported combination. This local input must not replace the old registration early. |
| S03 | Incomplete/unsupported combination | Field error; previous working shortcut remains. No popup outside settings. |
| S04 | OS shortcut conflict | Field error "This shortcut is already in use." Restore/retain old displayed working value. |
| S05 | New shortcut registration fails | Field error; preserve old registration and saved configuration. |
| S06 | Clear shortcut succeeds | Unassigned; registration removed; reception and popup copying remain available. |
| S07 | Clear/unregister or setting save fails | Show the actual integration state and failure. Do not falsely display Unassigned; implementation must reconcile partial integration/save failure. |
| S08 | Startup registration succeeds | Switch reflects actual saved/registered state; no success toast. |
| S09 | Startup registration fails | Restore actual state; inline error and retry action. Pairing/reception continue. Automatic startup never opens Settings to report it. |
| S10 | Duration input invalid | Field error, positive whole seconds required; keep the last saved value. Exact accepted bounds are not decided here. |
| S11 | Duration save succeeds | Input shows saved value; existing popups retain their lifetime. |
| S12 | Any settings save fails | Inline persistent failure; retain the last successfully saved setting and actual system state. |
| S13 | Startup config/system registration disagree | Show actual integration state and safe repair/retry explanation; no repeated notification loop. |
| S15 | Settings/persistent-data read or version-upgrade failure | Safe error inside the available utility surface; preserve existing files rather than offer a blind reset. Exact compatibility/recovery actions belong to the open architecture decision. |
| S16 | Default UI language | Follow system selected; resolve Chinese/English from the Windows UI language. The demo remains Chinese. |
| S17 | Manual Chinese / English choice | Override the system-language preference. The gallery shows the manual Chinese selection. |
| S18 | Language preference save fails | Inline failure; retain the previously saved selection and existing reception/Pairing state. |

Settings use explicit Change/Clear for shortcuts. Leaving the duration field
validates the draft and saves a valid value.
The startup switch reflects the confirmed OS/save outcome.
No app-wide Save/Cancel transaction is introduced. There are no password,
Account profile, theme, endpoint, proxy, automatic OTP expiry,
clipboard-clearing, history or test settings in this proposal.

## Installation, upgrade, uninstall and OS-owned UI

| ID | Condition | Expected visible surface |
| --- | --- | --- |
| D01 | Installer entry | Standard NSIS Welcome / installation directory / progress / completion pages as needed by the later installer task. App identity uses the same logo/name. |
| D02 | OS requests elevation or warns about trust | Native Windows UAC/SmartScreen surface if Windows policy/installer privilege requires it. No app-designed replica, bypass instruction or guarantee that signing removes all warnings. |
| D03 | Unsupported Windows/architecture | Installer error before unsupported execution; Windows 10 22H2 / Windows 11 x64 are the approved initial scope. |
| D04 | Install permission / disk / file-in-use failure | Standard installer error with safe retry/cancel instructions. No product pairing-success implication. |
| D05 | Explicit launch after successful installation | Pairing first for a new installation. Installer completion alone does not claim delivery readiness. |
| D06 | Upgrade | Standard progress/completion; preserve existing settings, Receiver credentials/identity and Pairing keys. |
| D07 | Upgrade failure | Standard installer error. No preemptive clearing of existing app state. |
| D08 | Uninstall confirmation / progress | Standard uninstaller confirmation. Explain removal of local data and integrations and how to remove old pairings on the sending devices. |
| D09 | Uninstall fails / files locked | Standard safe error, without a false complete-removal claim. |
| D10 | Reinstall | New Receiver registration and new QR; no Restore old Receiver action. |
| D11 | Slint/renderer cannot create any UI | For explicit launch, a minimal native Windows error and Close is the fallback proposal. Automatic startup still does not open UI; no SMS/key/token details and no destructive data cleanup. |

Installer page copy/details are not fully designed by a Slint screenshot.
They belong to NSIS/Windows, not another app framework.

## Error and feedback policy

| Mechanism | Use | Avoid |
| --- | --- | --- |
| Inline field error | Shortcut conflict, invalid duration, settings/OS-integration failure initiated in Settings | Generic blocking modal for each failure |
| Persistent status reason / banner | Unreadable local data, revocation, pairing errors | Repeated alerts during automatic reconnect |
| OS-owned error UI | Installer, elevation/trust, other Windows-owned operations | HTML/Slint replicas of UAC or SmartScreen |
| Nothing visible | Normal ACK, reconnect success, hidden pairing success, lock/suspend, invalid-message background discard, automatic startup | Invented intermediate success states or privacy leaks |

Safe wording separates the scope of failures:
An unreadable message, an unreadable device key, and Receiver revocation are
separate states. A completed pairing does not verify receipt or copying of the test SMS.
Only a known local key-read failure activates the specific re-pair guidance.

## Accessibility and ordinary Windows behavior

Proposed baseline: standard Fluent controls with Tab/Shift+Tab, visible focus,
Enter/Space activation and labels; Escape dismisses a local capture/menu/dialog
without canceling a remote pairing. Status/error information is text, not only
color. Native titlebars/system menus stay standard except removed resize and
maximize commands. Long device identifiers and safe errors wrap or scroll inside
the fixed surface. A popup is non-activating on arrival; explicit keyboard
access/activation policy for individual SMS windows is still to be agreed.

## Confirmed versus remaining decisions

Already decided by the user: B, simple utility, fixed sizes/no maximize or
fullscreen, no theme/dark, reuse the existing logo, Chinese/English choice
defaulting to system language, Chinese-only demo screens, and tray single-click
no-op, double-click open and right-click native menu.

Proposed here: exact dimensions;
no-OTP Copy hidden; status and inline feedback; surface reuse;
primary-monitor placement; overflow handling;
shortcut-capture UI; per-field settings operations; initial settings gating;
keyboard access to non-activating SMS windows.

No new functional policy is adopted. The UI child stays open. No product code,
Rust crate selection, native tray/no-focus/maximize integration test,
performance project, acceptance plan, or end-to-end exercise is supplied.

## Technical sources inspected

- Slint Window size constraints:
  https://docs.slint.dev/latest/docs/slint/reference/window/window/
- Slint standard controls and ScrollView:
  https://docs.slint.dev/latest/docs/slint/reference/std-widgets/
- Slint Rust native-window access:
  https://docs.slint.dev/latest/docs/rust/slint/winit_030/
  (version/backend feature dependent; not an approved dependency pin).
- Microsoft notification-area API, tooltip and context-menu guidance:
  https://learn.microsoft.com/en-us/windows/win32/shell/notification-area
- Microsoft non-activating window/show semantics:
  https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
  and https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindow

The Windows adapter must arrange non-activating creation/show before an SMS
window can flash into focus. A Slint `PopupWindow`/auto-dismiss popover alone is
not a substitute for independent, non-activating SMS windows. Likewise,
Slint's in-window ContextMenuArea is not the system-tray integration.
