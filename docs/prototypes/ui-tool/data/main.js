window.UI_CASES = [
  {
    "key": "main-ready",
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u5c31\u7eea",
    "component": "MainWindow",
    "behavior": "\u4e3b\u52a8\u6253\u5f00\u4e3b\u7a97\u53e3\uff1b\u4e0d\u663e\u793a\u77ed\u4fe1\u5386\u53f2\u3001\u624b\u673a\u5728\u7ebf\u72b6\u6001\u6216\u9a8c\u8bc1\u7801\u5217\u8868\u3002",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u672a\u5c31\u7eea\uff0f\u9996\u6b21\u8bbe\u7f6e",
    "component": "MainWindow",
    "behavior": "\u9996\u6b21\u4e3b\u52a8\u542f\u52a8\u5148\u663e\u793a\u914d\u5bf9\uff1b\u4e4b\u540e\u53ef\u67e5\u770b\u6b64\u4e3b\u7a97\u53e3\u72b6\u6001\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u914d\u5bf9\u624b\u673a\u540e\uff0c\u5373\u53ef\u63a5\u6536\u65b0\u77ed\u4fe1\u3002",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u672a\u5c31\u7eea\uff0f\u6ca1\u6709\u5df2\u914d\u5bf9\u624b\u673a",
    "component": "MainWindow",
    "behavior": "\u63a5\u6536\u51ed\u636e\u4ecd\u6709\u6548\u65f6\u6dfb\u52a0\u624b\u673a\uff0c\u6cbf\u7528\u73b0\u6709\u63a5\u6536\u7aef\uff0c\u4e0d\u521b\u5efa\u65b0\u8eab\u4efd\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u8fd8\u6ca1\u6709\u5df2\u914d\u5bf9\u7684\u624b\u673a\uff0c\u8bf7\u6dfb\u52a0\u624b\u673a\u3002",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u672a\u5c31\u7eea\uff0f\u6b63\u5728\u91cd\u8fde",
    "component": "MainWindow",
    "behavior": "\u6309\u65e2\u5b9a\u9000\u907f\u89c4\u5219\u81ea\u52a8\u91cd\u8fde\uff0c\u4e0d\u53cd\u590d\u5f39\u51fa\u62a5\u9519\uff0c\u4e5f\u4e0d\u589e\u52a0\u7b2c\u4e09\u79cd\u5c31\u7eea\u72b6\u6001\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u8fde\u63a5\u5df2\u4e2d\u65ad\uff0c\u6b63\u5728\u81ea\u52a8\u91cd\u8bd5\u3002"
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u672a\u5c31\u7eea\uff0f\u7f51\u7edc\u4e0d\u53ef\u7528",
    "component": "MainWindow",
    "behavior": "\u7b49\u5f85\u7f51\u7edc\u6062\u590d\u5e76\u6309\u89c4\u5219\u91cd\u8fde\uff0c\u4e0d\u63d0\u4f9b\u5173\u95ed\u5b89\u5168\u6821\u9a8c\u7684\u9009\u9879\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u6682\u65f6\u65e0\u6cd5\u8fde\u63a5\u670d\u52a1\uff0c\u6b63\u5728\u81ea\u52a8\u91cd\u8bd5\u3002"
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u672a\u5c31\u7eea\uff0f\u5df2\u6709\u5176\u4ed6\u8fde\u63a5",
    "component": "MainWindow",
    "behavior": "\u7ee7\u7eed\u81ea\u52a8\u91cd\u8bd5\uff0c\u4e0d\u63d0\u4f9b\u62a2\u5360\u8fde\u63a5\u6216\u91cd\u7f6e\u914d\u5bf9\u6309\u94ae\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u5df2\u6709\u5176\u4ed6\u8fde\u63a5\u6b63\u5728\u4f7f\u7528\uff0c\u7a0d\u540e\u81ea\u52a8\u91cd\u8bd5\u3002"
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u672c\u5730\u63a5\u6536\u51ed\u636e\u4e0d\u53ef\u7528",
    "component": "MainWindow",
    "behavior": "\u505c\u6b62\u91cd\u8fde\uff1b\u53ea\u6709\u4e3b\u52a8\u9009\u62e9\u201c\u91cd\u65b0\u6ce8\u518c\u201d\u624d\u5f00\u59cb\u521b\u5efa\u65b0\u63a5\u6536\u7aef\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u65e0\u6cd5\u8bfb\u53d6\u672c\u5730\u63a5\u6536\u51ed\u636e\u3002",
      "notice": "\u8bf7\u91cd\u65b0\u6ce8\u518c\u63a5\u6536\u7aef\uff0c\u5e76\u5728\u5404\u53f0 Android \u624b\u673a\u4e0a\u79fb\u9664\u65e7\u914d\u5bf9\u3002",
      "action": "\u91cd\u65b0\u6ce8\u518c",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u4e00\u53f0\u624b\u673a\u9700\u8981\u5904\u7406",
    "component": "MainWindow",
    "behavior": "\u4ec5\u7528\u4e8e\u5df2\u786e\u8ba4\u7684\u672c\u5730\u914d\u5bf9\u5bc6\u94a5\u8bfb\u53d6\u5931\u8d25\uff1b\u5176\u4ed6\u624b\u673a\u548c\u63a5\u6536\u8fde\u63a5\u7ee7\u7eed\u5de5\u4f5c\u3002",
    "properties": {
      "notice": "\u4e00\u53f0\u624b\u673a\u9700\u8981\u91cd\u65b0\u914d\u5bf9\uff0c\u5176\u4ed6\u624b\u673a\u4ecd\u53ef\u6b63\u5e38\u63a5\u6536\u3002",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u63a5\u6536\u7aef\u5df2\u88ab\u79fb\u9664",
    "component": "MainWindow",
    "behavior": "\u6536\u5230\u6743\u5a01\u64a4\u9500\u901a\u77e5\u540e\u6e05\u7406\u8eab\u4efd\u3001\u5bc6\u94a5\u3001\u5f85\u5b8c\u6210\u4efb\u52a1\u3001\u77ed\u4fe1\u548c\u9a8c\u8bc1\u7801\uff0c\u4fdd\u7559\u666e\u901a\u8bbe\u7f6e\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u6b64\u63a5\u6536\u7aef\u5df2\u88ab\u79fb\u9664\u3002",
      "notice": "\u5982\u9700\u7ee7\u7eed\u4f7f\u7528\uff0c\u8bf7\u4e3b\u52a8\u91cd\u65b0\u6ce8\u518c\u6b64\u63a5\u6536\u7aef\u3002",
      "action": "\u91cd\u65b0\u6ce8\u518c",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u8fde\u63a5\u534f\u8bae\u5f02\u5e38",
    "component": "MainWindow",
    "behavior": "\u663e\u793a\u666e\u901a\u8fde\u63a5\u9519\u8bef\uff0c\u4e0d\u5f3a\u5236\u8fdb\u5165\u66f4\u65b0\u9875\u9762\uff0c\u4e5f\u4e0d\u6539\u53d8\u91cd\u8bd5\u89c4\u5219\u3002",
    "properties": {
      "status": "\u672a\u5c31\u7eea",
      "reason": "\u8fde\u63a5\u6570\u636e\u683c\u5f0f\u5f02\u5e38\uff0c\u6b63\u5728\u81ea\u52a8\u91cd\u8bd5\u3002",
      "notice": "\u4e0a\u6b21\u8fde\u63a5\u4f7f\u7528\u4e86\u4e0d\u652f\u6301\u7684\u6d88\u606f\u683c\u5f0f\u3002",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u4e00\u6761\u77ed\u4fe1\u65e0\u6cd5\u8bfb\u53d6",
    "component": "MainWindow",
    "behavior": "\u4e0d\u5f39\u51fa\u65e0\u6548\u77ed\u4fe1\uff0c\u4e0d\u63a8\u65ad\u5bc6\u94a5\u4e22\u5931\uff0c\u4e0d\u66b4\u9732\u654f\u611f\u8bca\u65ad\uff0c\u7ee7\u7eed\u63a5\u6536\u540e\u7eed\u77ed\u4fe1\u3002",
    "properties": {
      "notice": "\u4e00\u6761\u77ed\u4fe1\u65e0\u6cd5\u8bfb\u53d6\uff0c\u4e4b\u524d\u7684\u6709\u6548\u9a8c\u8bc1\u7801\u4e0d\u53d7\u5f71\u54cd\u3002",
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
    "group": "\u4e3b\u7a97\u53e3",
    "title": "\u6258\u76d8\u56fe\u6807\u6682\u4e0d\u53ef\u7528",
    "component": "MainWindow",
    "behavior": "\u4ec5\u5728\u4e3b\u52a8\u6253\u5f00\u7a97\u53e3\u65f6\u663e\u793a\uff1b\u81ea\u52a8\u542f\u52a8\u4e0d\u4f1a\u4e3a\u6b64\u5f39\u51fa\u7a97\u53e3\u3002",
    "properties": {
      "notice": "\u6258\u76d8\u56fe\u6807\u6682\u4e0d\u53ef\u7528\uff0c\u53ef\u4ece\u5f00\u59cb\u83dc\u5355\u91cd\u65b0\u6253\u5f00\u9e4a\u4fe1\u3002",
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
    "group": "\u5b89\u9759\u5904\u7406",
    "title": "\u5355\u4e2a\u914d\u5bf9\u88ab\u79fb\u9664",
    "component": null,
    "behavior": "\u53ea\u79fb\u9664\u5bf9\u5e94\u624b\u673a\u3001\u5f39\u7a97\u548c\u5fc5\u8981\u7684\u6700\u65b0\u9a8c\u8bc1\u7801\uff0c\u4e0d\u663e\u793a\u6574\u4e2a\u63a5\u6536\u7aef\u88ab\u64a4\u9500\u3002",
    "properties": {},
    "feature": "main"
  }
];
