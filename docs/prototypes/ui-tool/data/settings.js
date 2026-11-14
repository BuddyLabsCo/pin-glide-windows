window.UI_CASES = [
  {
    "key": "settings-default",
    "group": "Settings",
    "title": "Default settings",
    "component": "SettingsWindow",
    "behavior": "Startup ON, shortcut Unassigned, 30 seconds. No theme, cache-expiry or history controls.",
    "properties": {},
    "image": "shots/settings-default.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-capture",
    "group": "Settings",
    "title": "Capture shortcut",
    "component": "SettingsWindow",
    "behavior": "Escape cancels. Enter applies a complete supported chord; the old shortcut remains registered.",
    "properties": {
      "capturing": true,
      "shortcut": "Ctrl + Alt + V"
    },
    "image": "shots/settings-capture.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-shortcut-invalid",
    "group": "Settings",
    "title": "Incomplete shortcut",
    "component": "SettingsWindow",
    "behavior": "Field error. Keep old valid configuration/registration.",
    "properties": {
      "shortcut": "Ctrl + Alt + V",
      "shortcut-error": "Choose a complete supported key combination."
    },
    "image": "shots/settings-shortcut-invalid.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-shortcut-conflict",
    "group": "Settings",
    "title": "Shortcut already in use",
    "component": "SettingsWindow",
    "behavior": "Registration failed; the previous working shortcut remains intact.",
    "properties": {
      "shortcut": "Ctrl + Alt + V",
      "shortcut-error": "This shortcut is already in use. The previous shortcut is unchanged."
    },
    "image": "shots/settings-shortcut-conflict.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-shortcut-failed",
    "group": "Settings",
    "title": "Shortcut registration/clear failed",
    "component": "SettingsWindow",
    "behavior": "Show actual OS state; do not claim Unassigned after a failed unregister.",
    "properties": {
      "shortcut": "Ctrl + Alt + V",
      "shortcut-error": "Could not change the shortcut. The previous setting remains active."
    },
    "image": "shots/settings-shortcut-failed.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-cleared",
    "group": "Settings",
    "title": "Shortcut cleared",
    "component": "SettingsWindow",
    "behavior": "Unassigned, no success toast; reception and popup copying unaffected.",
    "properties": {},
    "image": "shots/settings-cleared.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-startup-failed",
    "group": "Settings",
    "title": "Startup registration failed",
    "component": "SettingsWindow",
    "behavior": "Actual OFF state is shown. Reception still works. Automatic startup never opens this page.",
    "properties": {
      "startup": false,
      "notice": "Could not enable launch at login. Try the checkbox again."
    },
    "image": "shots/settings-startup-failed.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-duration-invalid",
    "group": "Settings",
    "title": "Invalid popup duration",
    "component": "SettingsWindow",
    "behavior": "Last saved value remains. Exact supported input bounds are not yet decided.",
    "properties": {
      "duration-error": "Enter a positive whole number of seconds."
    },
    "image": "shots/settings-duration-invalid.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-save-failed",
    "group": "Settings",
    "title": "Settings write failed",
    "component": "SettingsWindow",
    "behavior": "Keep last saved config and actual OS integration state; no false success.",
    "properties": {
      "notice": "Could not save the change. The last saved settings remain in use."
    },
    "image": "shots/settings-save-failed.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  }
];
