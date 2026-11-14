window.UI_CASES = [
  {
    "key": "tray-ready",
    "group": "Tray",
    "title": "Ready / latest code available",
    "component": "TrayContentPreview",
    "behavior": "OS-native context menu in production; this image shows content only, not an actual Win32 capture.",
    "properties": {},
    "image": "shots/tray-ready.png",
    "size": [
      360,
      392
    ],
    "feature": "tray"
  },
  {
    "key": "tray-empty",
    "group": "Tray",
    "title": "No cached OTP",
    "component": "TrayContentPreview",
    "behavior": "Copy latest code is disabled. It never writes an empty string.",
    "properties": {
      "has-code": false
    },
    "image": "shots/tray-empty.png",
    "size": [
      360,
      392
    ],
    "feature": "tray"
  },
  {
    "key": "tray-unpaired",
    "group": "Tray",
    "title": "First-use unpaired",
    "component": "TrayContentPreview",
    "behavior": "Tray only on automatic launch; explicit Pair a phone opens first pairing.",
    "properties": {
      "ready": false,
      "has-code": false,
      "can-configure": false
    },
    "image": "shots/tray-unpaired.png",
    "size": [
      360,
      392
    ],
    "feature": "tray"
  },
  {
    "key": "tray-revoked",
    "group": "Tray",
    "title": "Revoked / registration required",
    "component": "TrayContentPreview",
    "behavior": "Pair a phone is unavailable until explicit New registration from the main surface.",
    "properties": {
      "ready": false,
      "has-code": false,
      "can-pair": false
    },
    "image": "shots/tray-revoked.png",
    "size": [
      360,
      392
    ],
    "feature": "tray"
  }
];
