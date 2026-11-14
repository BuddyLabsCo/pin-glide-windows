"""Render the throwaway native Slint state gallery. Standard library only."""

import json
from pathlib import Path
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
CASES = []


def case(key, group, title, component, behavior, **properties):
    CASES.append(dict(key=key, group=group, title=title, component=component,
                      behavior=behavior, properties=properties))


case("main-ready", "Main", "Ready", "MainWindow",
     "Normal explicit Open. No SMS history, Sender-presence claim or code list.")
case("main-unpaired", "Main", "Not Ready / first setup", "MainWindow",
     "Explicit first launch opens pairing first; this main state is inspectable afterward.",
     status="Not Ready", reason="Pair a phone to start receiving messages.", **{"phone-count": 0})
case("main-no-phones", "Main", "Not Ready / no paired phones", "MainWindow",
     "If R remains valid, Pair a phone uses ordinary add-Sender, not a new Receiver.",
     status="Not Ready", reason="No paired phones. Add a phone to this receiver.", **{"phone-count": 0})
case("main-reconnect", "Main", "Not Ready / reconnecting", "MainWindow",
     "Automatic backoff continues; no repeated error popup or extra top-level state.",
     status="Not Ready", reason="Connection interrupted. Retrying automatically.")
case("main-network", "Main", "Not Ready / network unavailable", "MainWindow",
     "Wait for network improvement and normal backoff. Do not offer insecure TLS.",
     status="Not Ready", reason="The service cannot be reached. Retrying automatically.")
case("main-conflict", "Main", "Not Ready / another connection", "MainWindow",
     "No takeover/reset button. Follow the same approved automatic backoff.",
     status="Not Ready", reason="Another connection is active. Retrying automatically.")
case("main-local-r", "Main", "Local receiver data unavailable", "MainWindow",
     "Stop reconnecting. Only an explicit New registration begins the replacement flow.",
     status="Not Ready", reason="Local receiver data cannot be opened.",
     notice="Register as a new receiver. Remove the old pairing on every Android phone.",
     action="New registration", **{"phone-count": 0, "notice-error": True})
case("main-one-key", "Main", "One phone needs attention", "MainWindow",
     "Known Windows-local key failure only; other Pairings and R keep working.",
     notice="One phone needs to be paired again. Other phones keep working.", attention=True)
case("main-revoked", "Main", "Receiver removed", "MainWindow",
     "Authoritative revocation: identity/keys/pending work/SMS/OTP cleared; settings kept.",
     status="Not Ready", reason="This receiver was removed from the account.",
     notice="To use this computer again, explicitly register a new receiver.",
     action="New registration", **{"phone-count": 0, "notice-error": True})
case("main-protocol", "Main", "Ordinary protocol error", "MainWindow",
     "User-visible ordinary error. No forced-update or special retry policy.",
     status="Not Ready", reason="The connection could not be understood. Retrying automatically.",
     notice="The previous connection used an unsupported message format.", **{"notice-error": True})
case("main-invalid-message", "Main", "One message could not be read", "MainWindow",
     "No invalid SMS popup, no inferred key loss, no secret error details; continue later messages.",
     notice="One message could not be read. Earlier valid codes are unaffected.", **{"notice-error": True})
case("main-tray-failed", "Main", "Tray integration unavailable", "MainWindow",
     "Only visible after explicit Open; startup never opens a window to report it.",
     notice="The tray icon is unavailable. Reopen Pin Glide from the Start menu.", **{"notice-error": True})

case("pair-preparing", "Pairing", "Preparing a QR", "PairingWindow",
     "No QR before session/handshake material exists.",
     progress="Preparing a secure pairing code...", **{"show-qr": False})
case("pair-waiting", "Pairing", "Waiting / live QR", "PairingWindow",
     "QR lives five minutes. Closing the UI does not cancel. No auto-rotation.",
     progress="Waiting for your phone / 04:32 remaining")
case("pair-authorized", "Pairing", "Phone authorized", "PairingWindow",
     "Only authoritative session/relay state can advance this; a scan alone is not observable.",
     progress="Phone connected. Establishing secure pairing...", **{"show-qr": False})
case("pair-key-check", "Pairing", "Local key confirmation", "PairingWindow",
     "Human progress text; do not expose challenges or declare Ready early.",
     progress="Verifying the secure connection...", **{"show-qr": False})
case("pair-saving", "Pairing", "Protected local write", "PairingWindow",
     "A protected write failure is not a completed pairing.",
     progress="Saving secure pairing data...", **{"show-qr": False})
case("pair-wait-other", "Pairing", "Waiting for completion", "PairingWindow",
     "Do not guess the other side's confirmation or show fabricated percentage.",
     progress="Waiting for pairing confirmation...", **{"show-qr": False})
case("pair-credential-ready", "Pairing", "Credential ready", "PairingWindow",
     "Internal credential_ready; still Not Ready for a new Receiver.",
     progress="Preparing this receiver...", **{"show-qr": False})
case("pair-uncertain", "Pairing", "Registration result uncertain", "PairingWindow",
     "Same persisted operation; never attach unrestricted redemption retries to the UI.",
     progress="Checking registration result...",
     notice="Keep this operation open. Closing the window does not interrupt it.", **{"show-qr": False})
case("pair-issued", "Pairing", "Credential issued / first connection", "PairingWindow",
     "R has been stored; first delivery upgrade has not completed. Do not claim Ready.",
     progress="Connecting this receiver...", **{"show-qr": False})
case("pair-done", "Pairing", "New receiver Ready", "PairingWindow",
     "Only after the first successful delivery connection. Hidden completion stays quiet.",
     heading="Ready", detail="Your phone is paired with this computer.",
     progress="Ready to receive new messages.", action="Done", complete=True, **{"show-qr": False})
case("pair-added", "Pairing", "Additional phone paired", "PairingWindow",
     "No R redemption. Preserve the existing delivery connection and all other Pairings.",
     heading="Phone paired", detail="The new phone is now paired with this receiver.",
     progress="Other phones were not interrupted.", action="Done", complete=True,
     **{"show-qr": False, "completion-detail": "The existing receiver connection is unchanged."})
case("pair-expired", "Pairing", "QR expired", "PairingWindow",
     "Stale QR is no longer shown. Refresh explicitly creates new session/material.",
     progress="This code has expired.", action="Refresh code", **{"show-qr": False})
case("pair-create-failed", "Pairing", "Could not create session", "PairingWindow",
     "Try again creates a session; there is no broken QR or false success.",
     progress="Could not prepare a pairing code.", notice="The service could not be reached.",
     action="Try again", error=True, **{"show-qr": False})
case("pair-disconnected", "Pairing", "Pairing channel interrupted", "PairingWindow",
     "Preserve the live session; reconcile or wait for its real terminal state.",
     progress="Checking pairing connection...", notice="The pairing connection was interrupted.",
     **{"show-qr": False})
case("pair-auth-failed", "Pairing", "Secure confirmation failed", "PairingWindow",
     "Never Ready. Fresh pairing is offered only after safe terminal cleanup.",
     progress="Secure pairing could not be confirmed.",
     notice="Existing paired phones and their keys are unchanged.", error=True, **{"show-qr": False})
case("pair-save-failed", "Pairing", "Could not save secure data", "PairingWindow",
     "No false completion. Preserve existing data and route through safe operation recovery.",
     progress="Could not save secure pairing data.",
     notice="Check local storage access. Existing phones are unchanged.", error=True, **{"show-qr": False})
case("pair-canceled", "Pairing", "Session canceled / superseded", "PairingWindow",
     "Closing a window did not cause this. Expected following Close 1008 keeps this outcome.",
     progress="This pairing code is no longer active.",
     notice="The session ended. Start a fresh pairing when ready.", action="Create new code",
     **{"show-qr": False})
case("pair-rejected", "Pairing", "Authoritative authorization error", "PairingWindow",
     "Only show a reason Windows actually receives. Otherwise Android owns the rejection UI.",
     progress="The phone could not join this receiver.",
     notice="Check the pairing message shown in the Android app.", error=True, **{"show-qr": False})
case("new-registration-confirm", "Pairing", "Confirm a new Receiver", "ConfirmationWindow",
     "Only explicit New registration opens this. Cancel does nothing; failure preserves old local data.")

case("settings-default", "Settings", "Default settings", "SettingsWindow",
     "Startup ON, shortcut Unassigned, 30 seconds. No theme, cache-expiry or history controls.")
case("settings-capture", "Settings", "Capture shortcut", "SettingsWindow",
     "Escape cancels. Enter applies a complete supported chord; the old shortcut remains registered.",
     capturing=True, shortcut="Ctrl + Alt + V")
case("settings-shortcut-invalid", "Settings", "Incomplete shortcut", "SettingsWindow",
     "Field error. Keep old valid configuration/registration.",
     shortcut="Ctrl + Alt + V", **{"shortcut-error": "Choose a complete supported key combination."})
case("settings-shortcut-conflict", "Settings", "Shortcut already in use", "SettingsWindow",
     "Registration failed; the previous working shortcut remains intact.",
     shortcut="Ctrl + Alt + V", **{"shortcut-error": "This shortcut is already in use. The previous shortcut is unchanged."})
case("settings-shortcut-failed", "Settings", "Shortcut registration/clear failed", "SettingsWindow",
     "Show actual OS state; do not claim Unassigned after a failed unregister.",
     shortcut="Ctrl + Alt + V", **{"shortcut-error": "Could not change the shortcut. The previous setting remains active."})
case("settings-cleared", "Settings", "Shortcut cleared", "SettingsWindow",
     "Unassigned, no success toast; reception and popup copying unaffected.")
case("settings-startup-failed", "Settings", "Startup registration failed", "SettingsWindow",
     "Actual OFF state is shown. Reception still works. Automatic startup never opens this page.",
     startup=False, notice="Could not enable launch at login. Try the checkbox again.")
case("settings-duration-invalid", "Settings", "Invalid popup duration", "SettingsWindow",
     "Last saved value remains. Exact supported input bounds are not yet decided.",
     **{"duration-error": "Enter a positive whole number of seconds."})
case("settings-save-failed", "Settings", "Settings write failed", "SettingsWindow",
     "Keep last saved config and actual OS integration state; no false success.",
     notice="Could not save the change. The last saved settings remain in use.")

case("sms-otp", "Messages", "SMS with an OTP", "SmsWindow",
     "Full unchanged fixed test SMS; Copy uses this popup's exact OTP.")
case("sms-no-otp", "Messages", "SMS without an OTP", "SmsWindow",
     "Full SMS. Copy action hidden as a proposal; previous latest OTP is not cleared.",
     otp="", sms="[Pin Glide] Your parcel is ready at the front desk.\nPlease collect it before 20:00.")
case("sms-long", "Messages", "Long original SMS", "SmsWindow",
     "Fixed window; wrapped, scrollable body. Copy/Close remain reachable.",
     sms="[Pin Glide] Verification code: 482913.\n\n" +
     "\n\n".join(f"Original message paragraph {i}. Keep this text exactly as received; do not shorten or rewrite it."
                 for i in range(1, 9)))
case("sms-long-code", "Messages", "Non-numeric / long OTP", "SmsWindow",
     "Code area scrolls horizontally; never normalize or trim the original copy value.",
     otp="ABC-482913-xyz-LONG-ORIGINAL-VALUE", sms="Your original code is ABC-482913-xyz-LONG-ORIGINAL-VALUE.")
case("sms-copy-failed", "Messages", "Popup clipboard failure", "SmsWindow",
     "Keep popup/code/body for retry. No success UI.",
     error="Could not copy. Try again.")
case("feedback-no-code", "Messages", "Shortcut / no code", "FeedbackWindow",
     "Brief non-activating feedback. Clipboard unchanged. No main-window activation.")
case("feedback-copy-failed", "Messages", "Latest-code clipboard failure", "FeedbackWindow",
     "Retain cache. Retry by invoking latest-code copy again; coalesce repeated feedback.",
     heading="Could not copy", detail="The clipboard could not be updated. Your code is still in memory. Try the shortcut again.")

case("tray-ready", "Tray", "Ready / latest code available", "TrayContentPreview",
     "OS-native context menu in production; this image shows content only, not an actual Win32 capture.")
case("tray-empty", "Tray", "No cached OTP", "TrayContentPreview",
     "Copy latest code is disabled. It never writes an empty string.", **{"has-code": False})
case("tray-unpaired", "Tray", "First-use unpaired", "TrayContentPreview",
     "Tray only on automatic launch; explicit Pair a phone opens first pairing.",
     ready=False, **{"has-code": False, "can-configure": False})
case("tray-revoked", "Tray", "Revoked / registration required", "TrayContentPreview",
     "Pair a phone is unavailable until explicit New registration from the main surface.",
     ready=False, **{"has-code": False, "can-pair": False})

case("quiet-auto-start", "Quiet outcomes", "Automatic startup", None,
     "No application window. Tray only, even when unpaired or startup integration has an error.")
case("quiet-copy-success", "Quiet outcomes", "Copy succeeded", None,
     "No success prompt. The clicked SMS popup closes; latest-code shortcut copying remains silent.")
case("quiet-hidden-pair", "Quiet outcomes", "Pairing completes while hidden", None,
     "No forced reopening or toast. Reopen shows the real resulting state.")
case("quiet-lock", "Quiet outcomes", "Lock / unlock", None,
     "Close SMS windows on lock; no locked-session popups or body retention; no old-message replay on unlock.")
case("quiet-suspend", "Quiet outcomes", "Suspend / resume", None,
     "No activation. Pause retry while suspended; quietly check/reconnect on resume.")
case("quiet-ack", "Quiet outcomes", "Arrival receipt / ACK", None,
     "No UI. ACK is not successful decryption, popup display or copy.")
case("quiet-scanned", "Quiet outcomes", "QR scan not observed by Windows", None,
     "Keep waiting. Never fabricate a Scanned step from phone-side events unavailable to this client.")
case("quiet-removed", "Quiet outcomes", "One Pairing removed", None,
     "Remove only its row/popups and applicable latest cache. No Receiver-wide revocation modal.")
case("quiet-exit", "Quiet outcomes", "Exit process", None,
     "Disconnect; release SMS/OTP memory; keep ordinary settings and Pairing data. No OS clipboard clearing.")
case("os-installer", "OS / installer", "Install / upgrade / uninstall", None,
     "Standard NSIS and OS-owned pages, not Slint replicas. See inventory D01-D10 for every success/failure branch.")
case("os-trust", "OS / installer", "Windows trust / elevation", None,
     "UAC/SmartScreen only if Windows requests it. No app-designed substitute or promise to suppress it.")


def build():
    keys = [item["key"] for item in CASES]
    assert len(keys) == len(set(keys)), "Duplicate state key"
    assert {"pair-credential-ready", "pair-issued", "pair-uncertain",
            "sms-no-otp", "main-one-key", "tray-empty"} <= set(keys)
    dimensions = dict(MainWindow=(440, 600), PairingWindow=(440, 560),
                      SettingsWindow=(440, 600), SmsWindow=(380, 320),
                      FeedbackWindow=(340, 144), ConfirmationWindow=(440, 280),
                      TrayContentPreview=(360, 392))
    source = ROOT / "ui.slint"
    subprocess.run(["slint-viewer", "--check", "--style", "fluent", str(source)], check=True)
    shots = ROOT / "shots"
    shots.mkdir(exist_ok=True)
    for item in CASES:
        if not item["component"]:
            continue
        with tempfile.TemporaryDirectory(prefix="pin-glide-ui-") as scratch:
            props = Path(scratch) / "props.json"
            props.write_text(json.dumps(item["properties"], ensure_ascii=True), encoding="utf-8")
            output = shots / f"{item['key']}.png"
            subprocess.run(["slint-viewer", "--style", "fluent", "--component",
                            item["component"], "--load-data", str(props),
                            "--screenshot", str(output), str(source)], check=True,
                           capture_output=True, text=True)
            header = output.read_bytes()[:24]
            assert header[:8] == b"\x89PNG\r\n\x1a\n", output
            assert struct.unpack(">II", header[16:24]) == dimensions[item["component"]], output
            item["image"] = f"shots/{item['key']}.png"
            item["size"] = dimensions[item["component"]]
    features = {"Main": "main", "Pairing": "pairing", "Settings": "settings",
                "Messages": "messages", "Tray": "tray",
                "Quiet outcomes": "lifecycle", "OS / installer": "installation"}
    overrides = {"quiet-copy-success": "messages", "quiet-ack": "messages",
                 "quiet-hidden-pair": "pairing", "quiet-scanned": "pairing",
                 "quiet-removed": "main"}
    data_dir = ROOT / "data"
    data_dir.mkdir(exist_ok=True)
    for item in CASES:
        item["feature"] = overrides.get(item["key"], features[item["group"]])
    for feature in features.values():
        assert (ROOT / f"{feature}.html").is_file(), feature
        subset = [item for item in CASES if item["feature"] == feature]
        assert subset, feature
        data = json.dumps(subset, indent=2, ensure_ascii=True)
        (data_dir / f"{feature}.js").write_text(f"window.UI_CASES = {data};\n", encoding="utf-8")
    assert sum(len([item for item in CASES if item["feature"] == feature])
               for feature in features.values()) == len(CASES)
    version = subprocess.check_output(["slint-viewer", "--version"], text=True).strip()
    rendered = sum(bool(item["component"]) for item in CASES)
    print(json.dumps(dict(renderer=version, states=len(CASES), native_renders=rendered,
                          pages={feature: sum(item["feature"] == feature for item in CASES)
                                 for feature in features.values()})))


if __name__ == "__main__":
    build()
