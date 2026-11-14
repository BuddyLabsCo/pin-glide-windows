window.UI_CASES = [
  {
    "key": "main-ready",
    "group": "Main",
    "title": "Ready",
    "component": "MainWindow",
    "behavior": "Normal explicit Open. No SMS history, Sender-presence claim or code list.",
    "properties": {},
    "image": "shots/main-ready.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-unpaired",
    "group": "Main",
    "title": "Not Ready / first setup",
    "component": "MainWindow",
    "behavior": "Explicit first launch opens pairing first; this main state is inspectable afterward.",
    "properties": {
      "status": "Not Ready",
      "reason": "Pair a phone to start receiving messages.",
      "phone-count": 0
    },
    "image": "shots/main-unpaired.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-no-phones",
    "group": "Main",
    "title": "Not Ready / no paired phones",
    "component": "MainWindow",
    "behavior": "If R remains valid, Pair a phone uses ordinary add-Sender, not a new Receiver.",
    "properties": {
      "status": "Not Ready",
      "reason": "No paired phones. Add a phone to this receiver.",
      "phone-count": 0
    },
    "image": "shots/main-no-phones.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-reconnect",
    "group": "Main",
    "title": "Not Ready / reconnecting",
    "component": "MainWindow",
    "behavior": "Automatic backoff continues; no repeated error popup or extra top-level state.",
    "properties": {
      "status": "Not Ready",
      "reason": "Connection interrupted. Retrying automatically."
    },
    "image": "shots/main-reconnect.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-network",
    "group": "Main",
    "title": "Not Ready / network unavailable",
    "component": "MainWindow",
    "behavior": "Wait for network improvement and normal backoff. Do not offer insecure TLS.",
    "properties": {
      "status": "Not Ready",
      "reason": "The service cannot be reached. Retrying automatically."
    },
    "image": "shots/main-network.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-conflict",
    "group": "Main",
    "title": "Not Ready / another connection",
    "component": "MainWindow",
    "behavior": "No takeover/reset button. Follow the same approved automatic backoff.",
    "properties": {
      "status": "Not Ready",
      "reason": "Another connection is active. Retrying automatically."
    },
    "image": "shots/main-conflict.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-local-r",
    "group": "Main",
    "title": "Local receiver data unavailable",
    "component": "MainWindow",
    "behavior": "Stop reconnecting. Only an explicit New registration begins the replacement flow.",
    "properties": {
      "status": "Not Ready",
      "reason": "Local receiver data cannot be opened.",
      "notice": "Register as a new receiver. Remove the old pairing on every Android phone.",
      "action": "New registration",
      "phone-count": 0,
      "notice-error": true
    },
    "image": "shots/main-local-r.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-one-key",
    "group": "Main",
    "title": "One phone needs attention",
    "component": "MainWindow",
    "behavior": "Known Windows-local key failure only; other Pairings and R keep working.",
    "properties": {
      "notice": "One phone needs to be paired again. Other phones keep working.",
      "attention": true
    },
    "image": "shots/main-one-key.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-revoked",
    "group": "Main",
    "title": "Receiver removed",
    "component": "MainWindow",
    "behavior": "Authoritative revocation: identity/keys/pending work/SMS/OTP cleared; settings kept.",
    "properties": {
      "status": "Not Ready",
      "reason": "This receiver was removed from the account.",
      "notice": "To use this computer again, explicitly register a new receiver.",
      "action": "New registration",
      "phone-count": 0,
      "notice-error": true
    },
    "image": "shots/main-revoked.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-protocol",
    "group": "Main",
    "title": "Ordinary protocol error",
    "component": "MainWindow",
    "behavior": "User-visible ordinary error. No forced-update or special retry policy.",
    "properties": {
      "status": "Not Ready",
      "reason": "The connection could not be understood. Retrying automatically.",
      "notice": "The previous connection used an unsupported message format.",
      "notice-error": true
    },
    "image": "shots/main-protocol.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-invalid-message",
    "group": "Main",
    "title": "One message could not be read",
    "component": "MainWindow",
    "behavior": "No invalid SMS popup, no inferred key loss, no secret error details; continue later messages.",
    "properties": {
      "notice": "One message could not be read. Earlier valid codes are unaffected.",
      "notice-error": true
    },
    "image": "shots/main-invalid-message.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "main-tray-failed",
    "group": "Main",
    "title": "Tray integration unavailable",
    "component": "MainWindow",
    "behavior": "Only visible after explicit Open; startup never opens a window to report it.",
    "properties": {
      "notice": "The tray icon is unavailable. Reopen Pin Glide from the Start menu.",
      "notice-error": true
    },
    "image": "shots/main-tray-failed.png",
    "size": [
      440,
      600
    ],
    "feature": "main"
  },
  {
    "key": "quiet-removed",
    "group": "Quiet outcomes",
    "title": "One Pairing removed",
    "component": null,
    "behavior": "Remove only its row/popups and applicable latest cache. No Receiver-wide revocation modal.",
    "properties": {},
    "feature": "main"
  }
];
