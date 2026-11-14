window.UI_CASES = [
  {
    "key": "quiet-auto-start",
    "group": "Quiet outcomes",
    "title": "Automatic startup",
    "component": null,
    "behavior": "No application window. Tray only, even when unpaired or startup integration has an error.",
    "properties": {},
    "feature": "lifecycle"
  },
  {
    "key": "quiet-lock",
    "group": "Quiet outcomes",
    "title": "Lock / unlock",
    "component": null,
    "behavior": "Close SMS windows on lock; no locked-session popups or body retention; no old-message replay on unlock.",
    "properties": {},
    "feature": "lifecycle"
  },
  {
    "key": "quiet-suspend",
    "group": "Quiet outcomes",
    "title": "Suspend / resume",
    "component": null,
    "behavior": "No activation. Pause retry while suspended; quietly check/reconnect on resume.",
    "properties": {},
    "feature": "lifecycle"
  },
  {
    "key": "quiet-exit",
    "group": "Quiet outcomes",
    "title": "Exit process",
    "component": null,
    "behavior": "Disconnect; release SMS/OTP memory; keep ordinary settings and Pairing data. No OS clipboard clearing.",
    "properties": {},
    "feature": "lifecycle"
  }
];
