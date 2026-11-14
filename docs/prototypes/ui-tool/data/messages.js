window.UI_CASES = [
  {
    "key": "sms-otp",
    "group": "Messages",
    "title": "SMS with an OTP",
    "component": "SmsWindow",
    "behavior": "Full unchanged fixed test SMS; Copy uses this popup's exact OTP.",
    "properties": {},
    "image": "shots/sms-otp.png",
    "size": [
      380,
      320
    ],
    "feature": "messages"
  },
  {
    "key": "sms-no-otp",
    "group": "Messages",
    "title": "SMS without an OTP",
    "component": "SmsWindow",
    "behavior": "Full SMS. Copy action hidden as a proposal; previous latest OTP is not cleared.",
    "properties": {
      "otp": "",
      "sms": "[Pin Glide] Your parcel is ready at the front desk.\nPlease collect it before 20:00."
    },
    "image": "shots/sms-no-otp.png",
    "size": [
      380,
      320
    ],
    "feature": "messages"
  },
  {
    "key": "sms-long",
    "group": "Messages",
    "title": "Long original SMS",
    "component": "SmsWindow",
    "behavior": "Fixed window; wrapped, scrollable body. Copy/Close remain reachable.",
    "properties": {
      "sms": "[Pin Glide] Verification code: 482913.\n\nOriginal message paragraph 1. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 2. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 3. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 4. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 5. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 6. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 7. Keep this text exactly as received; do not shorten or rewrite it.\n\nOriginal message paragraph 8. Keep this text exactly as received; do not shorten or rewrite it."
    },
    "image": "shots/sms-long.png",
    "size": [
      380,
      320
    ],
    "feature": "messages"
  },
  {
    "key": "sms-long-code",
    "group": "Messages",
    "title": "Non-numeric / long OTP",
    "component": "SmsWindow",
    "behavior": "Code area scrolls horizontally; never normalize or trim the original copy value.",
    "properties": {
      "otp": "ABC-482913-xyz-LONG-ORIGINAL-VALUE",
      "sms": "Your original code is ABC-482913-xyz-LONG-ORIGINAL-VALUE."
    },
    "image": "shots/sms-long-code.png",
    "size": [
      380,
      320
    ],
    "feature": "messages"
  },
  {
    "key": "sms-copy-failed",
    "group": "Messages",
    "title": "Popup clipboard failure",
    "component": "SmsWindow",
    "behavior": "Keep popup/code/body for retry. No success UI.",
    "properties": {
      "error": "Could not copy. Try again."
    },
    "image": "shots/sms-copy-failed.png",
    "size": [
      380,
      320
    ],
    "feature": "messages"
  },
  {
    "key": "feedback-no-code",
    "group": "Messages",
    "title": "Shortcut / no code",
    "component": "FeedbackWindow",
    "behavior": "Brief non-activating feedback. Clipboard unchanged. No main-window activation.",
    "properties": {},
    "image": "shots/feedback-no-code.png",
    "size": [
      340,
      144
    ],
    "feature": "messages"
  },
  {
    "key": "feedback-copy-failed",
    "group": "Messages",
    "title": "Latest-code clipboard failure",
    "component": "FeedbackWindow",
    "behavior": "Retain cache. Retry by invoking latest-code copy again; coalesce repeated feedback.",
    "properties": {
      "heading": "Could not copy",
      "detail": "The clipboard could not be updated. Your code is still in memory. Try the shortcut again."
    },
    "image": "shots/feedback-copy-failed.png",
    "size": [
      340,
      144
    ],
    "feature": "messages"
  },
  {
    "key": "quiet-copy-success",
    "group": "Quiet outcomes",
    "title": "Copy succeeded",
    "component": null,
    "behavior": "No success prompt. The clicked SMS popup closes; latest-code shortcut copying remains silent.",
    "properties": {},
    "feature": "messages"
  },
  {
    "key": "quiet-ack",
    "group": "Quiet outcomes",
    "title": "Arrival receipt / ACK",
    "component": null,
    "behavior": "No UI. ACK is not successful decryption, popup display or copy.",
    "properties": {},
    "feature": "messages"
  }
];
