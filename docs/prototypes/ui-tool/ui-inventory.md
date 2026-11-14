# Windows UI inventory — compact utility direction

Status: discussion draft. Only the user's B direction, fixed non-maximizable
windows, reuse of the Android logo, and one light appearance are approved by
the November 14, 2026 UI checkpoint. The presentation policies below are
proposals for review, not a resolved UI specification.

Authority: [Windows functional resolution](https://github.com/BuddyLabsCo/pin-glide-windows/issues/2#issuecomment-6875824783).
UI decision: [Clarify Windows UI requirements after functional agreement](https://github.com/BuddyLabsCo/pin-glide-windows/issues/3).

## Selected direction and implementation boundary

- A compact tool window; no sidebar, dashboard, onboarding carousel, or large
  ornamental status illustration.
- One fixed light appearance. No theme preference, dark appearance, or
  system-theme-following behavior in this scope. This supersedes W11's old
  light/dark language; it does not create a future dark-mode task.
- The original Android app icon, copied byte-for-byte from
  `app/src/main/res/mipmap-nodpi/ic_launcher.png`, observed at Android source
  `4ee4dd42f522bb56cd54c20e98247f93e276192c`; blob
  `a6ae2d7b6ee4327462a6b91f0ce4448a86384cf3`.
- Rust + Slint with Fluent standard widgets for app surfaces. Windows APIs
  through the selected `windows` crate for tray/menu, window styles, placement,
  clipboard, shortcut, startup, and session integration. No WebView/Electron,
  browser CSS effects, Acrylic/Mica requirement, custom titlebar, or animation.
- `ui.slint` is a real, compilable Slint prototype. All app screenshots in the
  gallery are rendered by `slint-viewer 1.18.1 --style fluent`. That is the
  preview tool version, not a production dependency pin.
- Main/settings: proposed 440 x 600 logical content pixels; pairing:
  440 x 560; SMS: 380 x 320; action feedback: 340 x 144; new-registration
  confirmation: 440 x 280. Native Windows chrome adds its own frame.
  Exact dimensions remain reviewable.
- Fixed size means fixed logical dimensions with normal Windows DPI scaling,
  not a physical-pixel lock. No drag-resize, maximize button, double-click
  maximize, snap-resize, or fullscreen entry. Native window styles must
  enforce the same restriction as Slint layout constraints.
- Main has native minimize and close; close hides to tray. Pairing/settings
  use ordinary native titlebars without maximize; close dismisses their UI.
  SMS/feedback are separate borderless tool windows with a Close button,
  omitted from the taskbar; new arrivals do not activate them.
- Content overflow uses ScrollView/ListView and wrapped text, not a growing
  window. Native screenshots omit OS-drawn titlebars; gallery frames show
  the proposed minimize/close-only contract.
- The preview uses English UI copy, the product name Pin Glide / 鹊信, and
  unchanged Chinese fixed-test SMS. Final UI language is not decided here.

## Surface catalogue

The discussion demo uses separate HTML files by functional boundary.
`index.html` is only the overview/navigation entry. Use `main.html`,
`pairing.html`, `settings.html`, `messages.html`, `tray.html`, `lifecycle.html`
and `installation.html` for the corresponding states. State search/cycling
does not cross these page boundaries. Shared preview CSS and scripts are
separate assets; the HTML files can be edited independently.

| Surface | Contents and entry | Closing / lifetime |
| --- | --- | --- |
| Main utility window | Logo/name; Ready or Not Ready; one concise reason; paired-phone rows; Pair a phone; startup state; shortcut summary; popup-duration summary; Settings | Close hides to tray; reception continues. Explicit launch/tray Open reuses it. |
| Pairing window | Instructions; QR/progress/error area; remaining validity; explicit Refresh only when safe; existing-reception note for add-Sender | Close only hides UI. No Cancel pairing button in this proposal. Reopen returns to the same operation. |
| Settings view | Startup checkbox; shortcut capture/change/clear; popup-duration input/apply; field errors; version | Reuses the main-sized utility surface; Back returns. Closing does not stop reception. |
| SMS window | Logo/name; full original OTP when present; Copy code; complete original SMS in scroll area; source label; Close | Independent window per message. Success copies its own OTP and closes only itself. Timeout/manual close releases its body. |
| Action feedback | Brief no-code/clipboard-failure explanation for explicit latest-code actions when no app surface is focused | Non-activating app window; no success popup. No-code proposed 3 seconds; retryable failure proposed 6 seconds or Close. |
| New-registration confirmation | Old identity cannot be restored; each Android phone must remove old association and add new Receiver; Cancel/New registration | Only appears after the explicit new-registration command. Cancel changes nothing. |
| Tray icon + standard tooltip | Unchanged original logo; tooltip contains app name, Ready/Not Ready and a short safe reason | Lives while process runs. No code, SMS, badge count, animation, or synthesized Sender presence. |
| Native tray context menu | Disabled status line; Open; Copy latest code; Pair a phone; Settings; separator; Exit | Windows draws/dismisses it; keyboard navigation, Enter and Escape use native behavior. |
| Safe error details | One latest safe error/reason, optional expandable details inside the current utility view | No SMS/OTP/token/key/ciphertext dump or message history. Not a new diagnostic dashboard. |
| Installer/uninstaller | Standard NSIS pages and native Windows trust/UAC/error surfaces | Outside Slint app UI; no additional custom setup framework. |

Phone names must not depend on a new Sender-inventory API or Windows Account
profile. The current service context does not approve those features.
Examples use `Phone ...7C2A`, a display fallback derived from known Sender
identity. Android names may be shown only if an already-approved contract
actually supplies them. No rename workflow is introduced.

## Tray design and full menu

The app registers its real logo as the tray icon, with appropriately sized icon
resources for Windows DPI. Keep it the same for Ready and Not Ready; use the
tooltip and the menu status line to explain state. No flash or recolored-logo
variant is required. Windows and the user control whether it sits in the
visible notification area or its overflow; the app does not promise pinning.

Left-click or keyboard activation opens/activates the existing utility window.
Right-click opens the native menu near the tray icon. Reopening never creates
a second instance, delivery connection, or shortcut.

```text
Pin Glide — Ready                    [disabled status line]
----------------------------------
Open Pin Glide
Copy latest code                     [disabled when cache is empty]
Pair a phone                         [existing operation: reopen it]
Settings
----------------------------------
Exit
```

Menu variations:

| Situation | Status/reason | Copy latest code | Pair a phone | Settings |
| --- | --- | --- | --- | --- |
| Ready, nonempty latest OTP | Ready | Enabled; success silent | Ordinary add-Sender | Enabled |
| Ready, empty cache | Ready | Disabled; never writes an empty string | Ordinary add-Sender | Enabled |
| Reconnecting/conflict | Not Ready — Reconnecting / another connection active | Enabled if an OTP remains; readiness does not expire memory | Do not start a second enrollment; reopen existing operation or explain current restriction | Enabled |
| First-use unpaired | Not Ready — Pair a phone | Disabled | First Receiver pairing | Proposed disabled until the initial pairing-first step is completed |
| All Pairings removed, valid Receiver credential remains | Not Ready — No paired phones | Disabled after applicable cleanup | Ordinary add-Sender, not automatic new Receiver | Enabled |
| Receiver credential unreadable | Not Ready — Local receiver data unavailable | Reflect actual surviving cache policy, not an inferred bulk reset | Open main with explicit New registration; no silent reset | Enabled when setup had completed |
| Receiver revoked | Not Ready — Receiver removed | Disabled after mandatory cleanup | Explicit New registration first | Enabled; ordinary settings preserved |
| Pairing active while main hidden | Main readiness still follows existing Receiver | Reflect cache | Reopen the same QR/progress | Existing-receiver settings remain accessible |

Do not put pause receiving, SMS history, account login, remote removal,
Recover receiver, Switch account, Send test SMS, or theme controls in this menu.
Copy latest code is a proposed additional entry to the existing latest-code
action, with the same cache semantics as the global shortcut.

If tray registration fails, keep receiving when otherwise possible. On an
explicitly open main window show an inline integration warning explaining that
Start menu launch can reopen the existing app. Do not open a window at automatic
startup. Explorer restart should restore the icon quietly; repeated failures
remain inspectable, not repeated notifications. No "hidden to tray" success
message may claim an icon exists when registration actually failed.

## Launch, main-window and lifecycle states

| ID | Trigger / condition | Visible result and action |
| --- | --- | --- |
| L01 | Explicit first launch | Pairing surface first; no login/profile/setup wizard before it. |
| L02 | Automatic startup, paired | Tray only; connect quietly. No main window, onboarding, or success message. |
| L03 | Automatic startup, unpaired | Tray only, Not Ready reason. Explicit Open leads to pairing. |
| L04 | Explicit later launch or tray Open | Open/activate the existing fixed-size main surface. |
| L05 | Another explicit executable launch | Activate existing UI; no duplicate-process alert in the ordinary case. |
| L06 | Main/settings close | Hide utility UI; no "still running" toast every time. |
| L07 | Pairing close/reopen | Hidden pairing continues; reopen identical live QR/progress/error. No cancellation. |
| L08 | Tray Exit | Disconnect and end process. No confirmation proposed; volatile SMS/OTP is released, settings and keys retained. |
| L09 | Windows session lock | Existing SMS windows close; no new SMS/action-feedback windows while locked. No retained bodies or replay on unlock. |
| L10 | Unlock | Remain tray-only unless UI was explicitly requested; no old-popup replay. Latest nonempty OTP can still be copied. |
| L11 | Suspend/hibernate | No UI; retry activity pauses. |
| L12 | Resume | Quiet connection check/reconnect; no activation or "welcome back" popup. |
| L13 | Ready | Main shows Ready and a short connection description; no activity feed or OTP history. |
| L14 | Connecting / transport retry | Main shows Not Ready with Connecting/Reconnecting reason. No third top-level readiness label. No retry alert loop. |
| L15 | Network unavailable / TLS / server error | Same reason area with safe explanation; automatic reconnect policy remains unchanged. No insecure-TLS workaround. |
| L16 | Exclusive connection conflict | Not Ready; "Another connection is active. Retrying automatically." No Take over/reset-pairings button. |
| L17 | Receiver credential absent/unreadable | Not Ready + persistent local-data explanation + explicit New registration. Stop connection attempts as already required. |
| L18 | One Windows-local Pairing key unreadable | Mark only that phone row Needs attention; explain remove-on-that-Android + add again. Other Pairings continue. |
| L19 | All Pairings removed, valid Receiver retained | Empty phone list + ordinary Pair a phone. Do not invent a new registration. |
| L20 | Authoritative Receiver revocation | Not Ready + revoked reason; clear app volatile/key/identity state as required; preserve settings; explicit new registration. |
| L21 | Unsupported protocol/type / unexpected closure | Concise ordinary connection/protocol error in status/details; no mandatory-update takeover screen or altered retry policy. |
| L22 | Invalid/authentication-failed SMS | No SMS/OTP display. One safe inline error/detail if inspected; preserve latest valid OTP and continue later deliveries. No per-message alert spam. |
| L23 | Authoritative Pairing removal | That row disappears; only its popups close; cache clears only if owned by it. Other reception continues; no fabricated Receiver revocation. |
| L24 | Phone logout / Account switch without removal | No inferred Windows row deletion or security modal. |
| L25 | Tray icon registration unavailable | Integration warning is separate from reception readiness. Explicit Start menu launch still reopens the existing app. |

## Pairing: every meaningful progress and error

The UI shows human descriptions, never B/R/M, private material, operation IDs,
cryptographic challenges, or credential strings.

| ID | Internal condition | UI proposal / allowed action |
| --- | --- | --- |
| P01 | Creating ordinary Pairing Session | "Preparing a secure pairing code..." with no QR until session/material exist. |
| P02 | QR waiting | QR, Android instructions, visible five-minute validity/countdown. No automatic rotation. |
| P03 | QR merely scanned, no authoritative event | Do not claim "scanned" or "confirmed"; keep waiting. Phone-side feedback belongs to Android. |
| P04 | Authorized Sender / handshake relay begins | "Phone connected. Establishing secure pairing..." based on actual observed state, not a scan guess. |
| P05 | Local key confirmation in progress | "Verifying the secure connection..." No Ready label yet for a new Receiver. |
| P06 | Saving protected local key / pending state | "Saving secure pairing data..." Only advance after the protected write succeeds. |
| P07 | Waiting for other completion report / activation | "Waiting for pairing confirmation..." No fake percentage or guessed step completion. |
| P08 | credential_ready | "Preparing this receiver..." Do not say complete or show a copyable credential. |
| P09 | Redemption request outstanding / uncertain response | "Checking registration result..." Keep the same operation; no unrestricted Retry redemption or destructive restart. |
| P10 | credential_issued, R persisted, first delivery connection pending | "Connecting this receiver..." Still Not Ready until the real delivery readiness condition is met. |
| P11 | First delivery upgrade completes new enrollment | Ready. If pairing UI is visible, show completion with Done/Back. If hidden, update state quietly. |
| P12 | add-Sender succeeds | "Phone paired"; existing Receiver readiness/connection remains independent. No new-R phase or interruption of other Pairings. |
| P13 | add-Sender fails | Inline error inside pairing surface; say existing phones keep working; preserve their keys and connection. |
| P14 | Five minutes elapsed | Expired text replaces the QR so stale material is not offered as usable. Explicit Refresh creates a fresh session/material. |
| P15 | Session creation network/service failure | Inline error + Try again creating the session; no broken/empty QR. |
| P16 | Pairing channel transport interrupted | Inline reconnect/checking reason; retain the live operation until its actual terminal state/expiry. |
| P17 | Local key confirmation/authentication failure | Inline security failure, never Ready. Offer fresh pairing only after safe terminal cleanup; preserve active Pairings. |
| P18 | Protected write fails | Inline "Could not save secure pairing data." No success claim; follow safe operation recovery, not silent reset. |
| P19 | Authoritative canceled / superseded | Explain this code is no longer usable. Fresh code only through an explicit new session; closing the window did not cause cancellation. |
| P20 | Expected terminal state then policy Close 1008 | Keep its completed/canceled/expired outcome; no "connection failed" replacement error. |
| P21 | Policy Close 1008 without a known terminal state | Ordinary safe connection/session error; do not assume success. |
| P22 | Phone/account/capacity authorization rejection | If Windows receives an authoritative reason, show its safe human explanation. Otherwise Android shows the reason and Windows remains waiting/expired. No Windows login or owner-proof screen. |
| P23 | Already-paired target scan | Do not replace keys, alias, Pairing or preferences. Only show Already paired if the Windows-side contract actually reports it; otherwise leave the session unchanged and rely on Android's message. |
| P24 | Pairing completes/fails while window hidden | No forced reopening or success toast. Reopen shows the resulting status. |
| P25 | Reopening a live session | Same QR and progress, no new session or timer reset. |
| P26 | Explicit new registration after unreadable R | First show the new-registration confirmation. Cancel does nothing; failure preserves remaining old local data. |

Refresh/Try again wording must route to protocol-safe actions. It cannot bypass
the approved one-time redemption operation/attempt rules. No generic retry
button is attached to every failure.

## SMS popup, full text and latest-code actions

| ID | Situation | UI / behavior |
| --- | --- | --- |
| M01 | Valid nonempty OTP | Exact OTP, Copy code button, full unchanged SMS, source fallback, Close. No code format assumptions. |
| M02 | Valid empty OTP | Full unchanged SMS under New message; hide Copy code rather than display a disabled primary action. This is a presentation proposal. |
| M03 | Long/multiline SMS | Fixed-size popup with wrapped original text and vertical scrolling; no truncation of the stored/displayable body or whitespace rewriting. |
| M04 | Long/non-numeric OTP | Scroll the code area horizontally as needed; copy the original string, never trim, synthesize or display only a normalized numeric guess. |
| M05 | Copy succeeds | Close only that message's popup immediately. No copied label, toast, sound, or success dialog. |
| M06 | Copy fails | Inline message next to the copy action; keep the body/OTP and action available for retry. |
| M07 | Default/configured timeout | Close only that popup; release its body. Each popup owns its lifetime; a settings change applies only to new popups. |
| M08 | Manual Close | Same body release; separately retained latest nonempty OTP remains. |
| M09 | Multiple arrivals | Independent app windows; no replacement of older messages and no history centre. New arrivals never take focus. Proposed placement/overflow below. |
| M10 | Older message finishes decrypting later | Its popup still appears, but latest-code action follows approved timestamp/arrival ordering, not visible topmost popup order. |
| M11 | No-OTP arrival after a previous OTP | Full new SMS is shown; latest-code action still copies the prior valid latest value. No empty-value feedback is emitted merely because the new SMS has no OTP. |
| M12 | Latest shortcut / tray action succeeds | Clipboard write only; no popup/confirmation and no automatic OS clipboard clearing. |
| M13 | Latest shortcut with no code | Clipboard unchanged; one brief non-activating No code available feedback window. A disabled tray Copy item does not invoke the action. |
| M14 | Latest shortcut/tray action clipboard failure | Brief non-activating failure feedback; cache retained; retry by invoking the action again. Coalesce repeated failures, not multiple stacked error windows. |
| M15 | Pairing-scoped removal | Close its popups and clear latest cache only if owned by it, without fallback to older history. |
| M16 | Receiver revocation / process exit / lock | Apply the already-approved scoped privacy cleanup; no old-popup replay afterward. Lock does not clear the latest OTP under the approved functional policy. |
| M17 | Invalid decrypted shape/authentication | No SMS window, code or copyable error payload. Safe details remain in the utility surface if inspected. |
| M18 | Fixed Android test SMS | Exactly the ordinary SMS path and UI; no Windows test tab, test action, extraction claim or E2E success screen. |

Proposed placement: primary-monitor work area, bottom right, 12 logical pixels
from the edge and 8 between independent windows; reserve actual taskbar space.
Older popups move upward, then into a column to the left. Do not move keyboard
focus when placing/repositioning. The initial monitor policy intentionally
avoids a cursor-following or monitor-picker setting.

Finite-screen overflow is **not resolved**. Once all fitting cells are used,
the least additional UI is a small cascade overlap of still-independent live
windows, without replacing, queueing, silently discarding, or adding history.
Whether that overlap is acceptable needs the user's live reaction. Stacking
limits, timer-on-hover behavior and whether copying an older popup should
also close related feedback are not silently adopted.

Fixed logical sizes still require clamping/repositioning after DPI/display
changes and keeping Close/copy controls reachable. If a work area is smaller
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
| S08 | Startup registration succeeds | Checkbox reflects actual saved/registered state; no success toast. |
| S09 | Startup registration fails | Restore actual state; inline error and retry action. Pairing/reception continue. Automatic startup never opens Settings to report it. |
| S10 | Duration input invalid | Field error, positive whole seconds required; keep the last saved value. Exact accepted bounds are not decided here. |
| S11 | Duration save succeeds | Input shows saved value; existing popups retain their lifetime. |
| S12 | Any settings save fails | Inline persistent failure; retain the last successfully saved setting and actual system state. |
| S13 | Startup config/system registration disagree | Show actual integration state and safe repair/retry explanation; no repeated notification loop. |
| S14 | Native tray registration/re-registration fails | Integration warning in explicitly opened utility UI; no new connection readiness state or automatic window. |
| S15 | Settings/persistent-data read or version-upgrade failure | Safe error inside the available utility surface; preserve existing files rather than offer a blind reset. Exact compatibility/recovery actions belong to the open architecture decision. |

Settings use explicit Change/Clear for shortcuts and Apply for duration.
Startup changes on checkbox interaction after a confirmed OS/save outcome.
No app-wide Save/Cancel transaction is introduced. There are no password,
Account profile, language, theme, endpoint, proxy, automatic OTP expiry,
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
| D08 | Uninstall confirmation / progress | Standard uninstaller confirmation; explain local data/integration removal and that old Android associations are removed on Android. Do not promise remote revocation or clipboard clearing. |
| D09 | Uninstall fails / files locked | Standard safe error, without a false complete-removal claim. |
| D10 | Reinstall | New Receiver registration and new QR; no Restore old Receiver action. |
| D11 | Slint/renderer cannot create any UI | For explicit launch, a minimal native Windows error and Close is the fallback proposal. Automatic startup still does not open UI; no SMS/key/token details and no destructive data cleanup. |

Installer page copy/details are not fully designed by a Slint screenshot.
They belong to NSIS/Windows, not another app framework.

## Error and feedback policy

| Mechanism | Use | Avoid |
| --- | --- | --- |
| Inline field error | Shortcut conflict, invalid duration, settings/OS-integration failure initiated in Settings | Generic blocking modal for each failure |
| Persistent status reason / banner | Disconnection, conflict, unreadable local data, revocation, pairing errors | Repeated alerts during automatic reconnect |
| Safe expandable detail | Protocol/service errors and unreadable/invalid messages, when explicitly inspected | Secret values, raw decrypted bodies, ciphertext, or a log/history screen |
| Brief app feedback tool window | User explicitly invokes latest-code copy with empty cache or clipboard failure and no current surface for inline feedback | Success toast or opening the main window |
| Confirmation dialog | Explicit replacement by a new Receiver registration | Confirming every Close/Exit/copy or pretending Close cancels pairing |
| OS-owned error UI | Installer, elevation/trust, other Windows-owned operations | HTML/Slint replicas of UAC or SmartScreen |
| Nothing visible | Normal ACK, reconnect success, hidden pairing success, lock/suspend, invalid-message background discard, automatic startup | Invented intermediate success states or privacy leaks |

Safe wording separates the scope of failures:
"Could not read one message" does not mean "Phone key lost";
"One phone needs attention" does not mean "Receiver revoked";
"Pairing added" does not prove receipt/copy of the Android test SMS.
Only a known local key-read failure activates the specific re-pair guidance.

## Accessibility and ordinary Windows behavior

Proposed baseline: standard Fluent controls with Tab/Shift+Tab, visible focus,
Enter/Space activation and labels; Escape dismisses a local capture/menu/dialog
without canceling a remote pairing. Status/error information is text, not only
color. Native titlebars/system menus stay standard except removed resize and
maximize commands. Long phone identifiers and safe errors wrap or scroll inside
the fixed surface. A popup is non-activating on arrival; explicit keyboard
access/activation policy for individual SMS windows is still to be agreed.

## Confirmed versus remaining decisions

Already decided by the user: B, simple utility, fixed sizes/no maximize or
fullscreen, no theme/dark, reuse actual Android logo.

Proposed here: exact dimensions; native tray menu commands and activation;
no-OTP Copy hidden; status/inline/transient feedback hierarchy; brief-feedback
durations; surface reuse; primary-monitor placement; overflow handling;
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
