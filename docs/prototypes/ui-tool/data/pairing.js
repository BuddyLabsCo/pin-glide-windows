window.UI_CASES = [
  {
    "key": "pair-preparing",
    "group": "Pairing",
    "title": "Preparing a QR",
    "component": "PairingWindow",
    "behavior": "No QR before session/handshake material exists.",
    "properties": {
      "progress": "Preparing a secure pairing code...",
      "show-qr": false
    },
    "image": "shots/pair-preparing.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-waiting",
    "group": "Pairing",
    "title": "Waiting / live QR",
    "component": "PairingWindow",
    "behavior": "QR lives five minutes. Closing the UI does not cancel. No auto-rotation.",
    "properties": {
      "progress": "Waiting for your phone / 04:32 remaining"
    },
    "image": "shots/pair-waiting.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-authorized",
    "group": "Pairing",
    "title": "Phone authorized",
    "component": "PairingWindow",
    "behavior": "Only authoritative session/relay state can advance this; a scan alone is not observable.",
    "properties": {
      "progress": "Phone connected. Establishing secure pairing...",
      "show-qr": false
    },
    "image": "shots/pair-authorized.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-key-check",
    "group": "Pairing",
    "title": "Local key confirmation",
    "component": "PairingWindow",
    "behavior": "Human progress text; do not expose challenges or declare Ready early.",
    "properties": {
      "progress": "Verifying the secure connection...",
      "show-qr": false
    },
    "image": "shots/pair-key-check.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-saving",
    "group": "Pairing",
    "title": "Protected local write",
    "component": "PairingWindow",
    "behavior": "A protected write failure is not a completed pairing.",
    "properties": {
      "progress": "Saving secure pairing data...",
      "show-qr": false
    },
    "image": "shots/pair-saving.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-wait-other",
    "group": "Pairing",
    "title": "Waiting for completion",
    "component": "PairingWindow",
    "behavior": "Do not guess the other side's confirmation or show fabricated percentage.",
    "properties": {
      "progress": "Waiting for pairing confirmation...",
      "show-qr": false
    },
    "image": "shots/pair-wait-other.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-credential-ready",
    "group": "Pairing",
    "title": "Credential ready",
    "component": "PairingWindow",
    "behavior": "Internal credential_ready; still Not Ready for a new Receiver.",
    "properties": {
      "progress": "Preparing this receiver...",
      "show-qr": false
    },
    "image": "shots/pair-credential-ready.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-uncertain",
    "group": "Pairing",
    "title": "Registration result uncertain",
    "component": "PairingWindow",
    "behavior": "Same persisted operation; never attach unrestricted redemption retries to the UI.",
    "properties": {
      "progress": "Checking registration result...",
      "notice": "Keep this operation open. Closing the window does not interrupt it.",
      "show-qr": false
    },
    "image": "shots/pair-uncertain.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-issued",
    "group": "Pairing",
    "title": "Credential issued / first connection",
    "component": "PairingWindow",
    "behavior": "R has been stored; first delivery upgrade has not completed. Do not claim Ready.",
    "properties": {
      "progress": "Connecting this receiver...",
      "show-qr": false
    },
    "image": "shots/pair-issued.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-done",
    "group": "Pairing",
    "title": "New receiver Ready",
    "component": "PairingWindow",
    "behavior": "Only after the first successful delivery connection. Hidden completion stays quiet.",
    "properties": {
      "heading": "Ready",
      "detail": "Your phone is paired with this computer.",
      "progress": "Ready to receive new messages.",
      "action": "Done",
      "complete": true,
      "show-qr": false
    },
    "image": "shots/pair-done.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-added",
    "group": "Pairing",
    "title": "Additional phone paired",
    "component": "PairingWindow",
    "behavior": "No R redemption. Preserve the existing delivery connection and all other Pairings.",
    "properties": {
      "heading": "Phone paired",
      "detail": "The new phone is now paired with this receiver.",
      "progress": "Other phones were not interrupted.",
      "action": "Done",
      "complete": true,
      "show-qr": false,
      "completion-detail": "The existing receiver connection is unchanged."
    },
    "image": "shots/pair-added.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-expired",
    "group": "Pairing",
    "title": "QR expired",
    "component": "PairingWindow",
    "behavior": "Stale QR is no longer shown. Refresh explicitly creates new session/material.",
    "properties": {
      "progress": "This code has expired.",
      "action": "Refresh code",
      "show-qr": false
    },
    "image": "shots/pair-expired.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-create-failed",
    "group": "Pairing",
    "title": "Could not create session",
    "component": "PairingWindow",
    "behavior": "Try again creates a session; there is no broken QR or false success.",
    "properties": {
      "progress": "Could not prepare a pairing code.",
      "notice": "The service could not be reached.",
      "action": "Try again",
      "error": true,
      "show-qr": false
    },
    "image": "shots/pair-create-failed.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-disconnected",
    "group": "Pairing",
    "title": "Pairing channel interrupted",
    "component": "PairingWindow",
    "behavior": "Preserve the live session; reconcile or wait for its real terminal state.",
    "properties": {
      "progress": "Checking pairing connection...",
      "notice": "The pairing connection was interrupted.",
      "show-qr": false
    },
    "image": "shots/pair-disconnected.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-auth-failed",
    "group": "Pairing",
    "title": "Secure confirmation failed",
    "component": "PairingWindow",
    "behavior": "Never Ready. Fresh pairing is offered only after safe terminal cleanup.",
    "properties": {
      "progress": "Secure pairing could not be confirmed.",
      "notice": "Existing paired phones and their keys are unchanged.",
      "error": true,
      "show-qr": false
    },
    "image": "shots/pair-auth-failed.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-save-failed",
    "group": "Pairing",
    "title": "Could not save secure data",
    "component": "PairingWindow",
    "behavior": "No false completion. Preserve existing data and route through safe operation recovery.",
    "properties": {
      "progress": "Could not save secure pairing data.",
      "notice": "Check local storage access. Existing phones are unchanged.",
      "error": true,
      "show-qr": false
    },
    "image": "shots/pair-save-failed.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-canceled",
    "group": "Pairing",
    "title": "Session canceled / superseded",
    "component": "PairingWindow",
    "behavior": "Closing a window did not cause this. Expected following Close 1008 keeps this outcome.",
    "properties": {
      "progress": "This pairing code is no longer active.",
      "notice": "The session ended. Start a fresh pairing when ready.",
      "action": "Create new code",
      "show-qr": false
    },
    "image": "shots/pair-canceled.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "pair-rejected",
    "group": "Pairing",
    "title": "Authoritative authorization error",
    "component": "PairingWindow",
    "behavior": "Only show a reason Windows actually receives. Otherwise Android owns the rejection UI.",
    "properties": {
      "progress": "The phone could not join this receiver.",
      "notice": "Check the pairing message shown in the Android app.",
      "error": true,
      "show-qr": false
    },
    "image": "shots/pair-rejected.png",
    "size": [
      440,
      560
    ],
    "feature": "pairing"
  },
  {
    "key": "new-registration-confirm",
    "group": "Pairing",
    "title": "Confirm a new Receiver",
    "component": "ConfirmationWindow",
    "behavior": "Only explicit New registration opens this. Cancel does nothing; failure preserves old local data.",
    "properties": {},
    "image": "shots/new-registration-confirm.png",
    "size": [
      440,
      280
    ],
    "feature": "pairing"
  },
  {
    "key": "quiet-hidden-pair",
    "group": "Quiet outcomes",
    "title": "Pairing completes while hidden",
    "component": null,
    "behavior": "No forced reopening or toast. Reopen shows the real resulting state.",
    "properties": {},
    "feature": "pairing"
  },
  {
    "key": "quiet-scanned",
    "group": "Quiet outcomes",
    "title": "QR scan not observed by Windows",
    "component": null,
    "behavior": "Keep waiting. Never fabricate a Scanned step from phone-side events unavailable to this client.",
    "properties": {},
    "feature": "pairing"
  }
];
