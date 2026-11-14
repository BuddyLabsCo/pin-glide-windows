window.UI_CASES = [
  {
    "key": "settings-default",
    "group": "\u8bbe\u7f6e",
    "title": "\u9ed8\u8ba4\u8bbe\u7f6e",
    "component": "SettingsWindow",
    "behavior": "\u5f00\u673a\u542f\u52a8\u5f00\u542f\uff0c\u5feb\u6377\u952e\u672a\u8bbe\u7f6e\uff0c\u5f39\u7a97\u4e09\u5341\u79d2\uff0c\u8bed\u8a00\u9ed8\u8ba4\u8ddf\u968f\u7cfb\u7edf\uff1b\u6ca1\u6709\u4e3b\u9898\u3001\u7f13\u5b58\u5230\u671f\u6216\u5386\u53f2\u8bbe\u7f6e\u3002",
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5f55\u5165\u5feb\u6377\u952e",
    "component": "SettingsWindow",
    "behavior": "Esc \u53d6\u6d88\uff0cEnter \u5e94\u7528\u6709\u6548\u7ec4\u5408\uff1b\u65b0\u7ec4\u5408\u6ce8\u518c\u6210\u529f\u524d\u4fdd\u7559\u65e7\u5feb\u6377\u952e\u3002",
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5feb\u6377\u952e\u7ec4\u5408\u65e0\u6548",
    "component": "SettingsWindow",
    "behavior": "\u5728\u5b57\u6bb5\u65c1\u63d0\u793a\uff0c\u4fdd\u7559\u539f\u6709\u6709\u6548\u8bbe\u7f6e\u548c\u6ce8\u518c\u3002",
    "properties": {
      "shortcut": "Ctrl + Alt + V",
      "shortcut-error": "\u8bf7\u8f93\u5165\u5b8c\u6574\u4e14\u53d7\u652f\u6301\u7684\u5feb\u6377\u952e\u7ec4\u5408\u3002"
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5feb\u6377\u952e\u5df2\u88ab\u5360\u7528",
    "component": "SettingsWindow",
    "behavior": "\u65b0\u6ce8\u518c\u5931\u8d25\uff0c\u7ee7\u7eed\u4fdd\u7559\u539f\u6709\u53ef\u7528\u5feb\u6377\u952e\u3002",
    "properties": {
      "shortcut": "Ctrl + Alt + V",
      "shortcut-error": "\u6b64\u5feb\u6377\u952e\u5df2\u88ab\u5360\u7528\uff0c\u539f\u6709\u5feb\u6377\u952e\u4fdd\u6301\u4e0d\u53d8\u3002"
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5feb\u6377\u952e\u4fee\u6539\u6216\u6e05\u9664\u5931\u8d25",
    "component": "SettingsWindow",
    "behavior": "\u663e\u793a\u771f\u5b9e\u7cfb\u7edf\u72b6\u6001\uff0c\u4e0d\u80fd\u5728\u6ce8\u9500\u5931\u8d25\u65f6\u58f0\u79f0\u5df2\u7ecf\u6e05\u9664\u3002",
    "properties": {
      "shortcut": "Ctrl + Alt + V",
      "shortcut-error": "\u5feb\u6377\u952e\u4fee\u6539\u5931\u8d25\uff0c\u7ee7\u7eed\u4f7f\u7528\u539f\u6709\u8bbe\u7f6e\u3002"
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5feb\u6377\u952e\u5df2\u6e05\u9664",
    "component": "SettingsWindow",
    "behavior": "\u663e\u793a\u672a\u8bbe\u7f6e\uff0c\u4e0d\u5f39\u6210\u529f\u63d0\u793a\uff1b\u63a5\u6536\u548c\u5f39\u7a97\u590d\u5236\u4e0d\u53d7\u5f71\u54cd\u3002",
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5f00\u673a\u542f\u52a8\u8bbe\u7f6e\u5931\u8d25",
    "component": "SettingsWindow",
    "behavior": "\u663e\u793a\u5b9e\u9645\u5173\u95ed\u72b6\u6001\uff1b\u63a5\u6536\u7ee7\u7eed\u5de5\u4f5c\uff1b\u81ea\u52a8\u542f\u52a8\u4e0d\u4e3b\u52a8\u6253\u5f00\u6b64\u9875\u62a5\u9519\u3002",
    "properties": {
      "startup": false,
      "notice": "\u65e0\u6cd5\u542f\u7528\u5f00\u673a\u542f\u52a8\uff0c\u53ef\u91cd\u65b0\u52fe\u9009\u91cd\u8bd5\u3002"
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
    "group": "\u8bbe\u7f6e",
    "title": "\u5f39\u7a97\u65f6\u957f\u65e0\u6548",
    "component": "SettingsWindow",
    "behavior": "\u4fdd\u7559\u4e0a\u6b21\u4fdd\u5b58\u7684\u503c\uff0c\u8f93\u5165\u8303\u56f4\u7684\u5177\u4f53\u4e0a\u9650\u4ecd\u5f85\u786e\u5b9a\u3002",
    "properties": {
      "duration-error": "\u8bf7\u8f93\u5165\u5927\u4e8e\u96f6\u7684\u6574\u6570\u79d2\u6570\u3002"
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
    "group": "\u8bbe\u7f6e",
    "title": "\u8bbe\u7f6e\u4fdd\u5b58\u5931\u8d25",
    "component": "SettingsWindow",
    "behavior": "\u4fdd\u7559\u6700\u540e\u4fdd\u5b58\u6210\u529f\u7684\u914d\u7f6e\u548c\u5b9e\u9645\u7cfb\u7edf\u72b6\u6001\uff0c\u4e0d\u663e\u793a\u865a\u5047\u7684\u6210\u529f\u7ed3\u679c\u3002",
    "properties": {
      "notice": "\u8bbe\u7f6e\u4fdd\u5b58\u5931\u8d25\uff0c\u7ee7\u7eed\u4f7f\u7528\u4e0a\u6b21\u4fdd\u5b58\u7684\u8bbe\u7f6e\u3002"
    },
    "image": "shots/settings-save-failed.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-language-zh",
    "group": "\u8bbe\u7f6e",
    "title": "\u624b\u52a8\u9009\u62e9\u4e2d\u6587",
    "component": "SettingsWindow",
    "behavior": "\u6b63\u5f0f\u5e94\u7528\u624b\u52a8\u4f7f\u7528\u4e2d\u6587\uff0c\u4e0d\u518d\u8ddf\u968f\u7cfb\u7edf\u8bed\u8a00\uff1b\u6b64\u6f14\u793a\u59cb\u7ec8\u53ea\u5c55\u793a\u4e2d\u6587\u3002",
    "properties": {
      "language-index": 1
    },
    "image": "shots/settings-language-zh.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-language-en",
    "group": "\u8bbe\u7f6e",
    "title": "\u624b\u52a8\u9009\u62e9 English",
    "component": "SettingsWindow",
    "behavior": "\u6b63\u5f0f\u5e94\u7528\u5e94\u5207\u6362\u4e3a\u82f1\u6587\uff1b\u6b64\u6f14\u793a\u4ec5\u5c55\u793a\u9009\u9879\u5df2\u9009\u4e2d\uff0c\u4e0d\u53e6\u505a\u82f1\u6587\u754c\u9762\u3002",
    "properties": {
      "language-index": 2
    },
    "image": "shots/settings-language-en.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  },
  {
    "key": "settings-language-save-failed",
    "group": "\u8bbe\u7f6e",
    "title": "\u8bed\u8a00\u504f\u597d\u4fdd\u5b58\u5931\u8d25",
    "component": "SettingsWindow",
    "behavior": "\u4fdd\u5b58\u5931\u8d25\u65f6\u4fdd\u7559\u539f\u6709\u8bed\u8a00\u504f\u597d\uff1b\u6b64\u5904\u4ecd\u4e3a\u9ed8\u8ba4\u7684\u8ddf\u968f\u7cfb\u7edf\uff0c\u4e0d\u6539\u53d8\u914d\u5bf9\u6216\u63a5\u6536\u3002",
    "properties": {
      "notice": "\u8bed\u8a00\u8bbe\u7f6e\u4fdd\u5b58\u5931\u8d25\uff0c\u7ee7\u7eed\u8ddf\u968f\u7cfb\u7edf\u8bed\u8a00\u3002"
    },
    "image": "shots/settings-language-save-failed.png",
    "size": [
      440,
      600
    ],
    "feature": "settings"
  }
];
