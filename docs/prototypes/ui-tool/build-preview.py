"""Render the throwaway native Slint state gallery. Standard library only."""

import json
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
CASES = []
STATE_ALIASES = {
    "pair-waiting": ("pair-rejected",),
    "pair-key-check": ("pair-authorized", "pair-saving", "pair-wait-other", "pair-disconnected"),
    "pair-failed": ("pair-auth-failed", "pair-save-failed"),
    "settings-default": ("settings-cleared",),
}


def case(key, group, title, component, behavior, **properties):
    CASES.append(dict(key=key, group=group, title=title, component=component,
                      behavior=behavior, properties=properties))


case("welcome-idle", "欢迎", "欢迎", "WelcomeWindow",
     "独立欢迎页；只有点击开始使用才创建配对会话。",
     **{"motion-enabled": False, "preview-seconds": 0})
case("welcome-arrived", "欢迎", "短信抵达电脑", "WelcomeWindow",
     "展示短信弹窗和验证码的静态画面，用于动效结尾及关闭动效时。",
     **{"motion-enabled": False})

case("main-ready", "主窗口", "就绪／已配对一台设备", "MainWindow",
     "首次配对成功后的主窗口。", **{"device-names": ["我的设备"]})
case("main-two-phones", "主窗口", "就绪／已配对两台设备", "MainWindow",
     "添加第二台设备后的主窗口。")
case("main-many-devices", "主窗口", "就绪／已配对多台设备", "MainWindow",
     "设备列表在固定窗口内滚动，配对和设置入口保持可见。",
     **{"device-names": ["我的设备", "备用设备", "工作设备", "书房设备", "旅行设备", "家用设备", "客厅设备"]})
case("main-no-phones", "主窗口", "未就绪／没有已配对的设备", "MainWindow",
     "接收凭据仍有效时添加设备，沿用现有接收端，不创建新身份。",
     reason="还没有已配对的设备。", **{"device-names": []})
case("main-local-r", "主窗口", "本地接收凭据不可用", "MainWindow",
     "停止重连；选择“配对新设备”后进入扫码流程，完成后创建新接收端。",
     reason="无法读取本地接收凭据。", **{"device-names": []})
case("main-one-key", "主窗口", "一台设备需要重新配对", "MainWindow",
     "仅用于已确认的本地配对密钥读取失败；其他设备和接收连接继续工作。",
     attention=True)
case("main-revoked", "主窗口", "接收端已被移除", "MainWindow",
     "收到权威撤销通知后清理身份、密钥、待完成任务、短信和验证码，保留普通设置。",
     reason="此接收端已被移除。", **{"device-names": []})

case("pair-preparing", "配对流程", "正在准备二维码", "PairingWindow",
     "会话和握手材料准备好之前，不显示二维码。",
     progress="正在准备二维码…",
     **{"qr-stage": "loading", "motion-enabled": False})
case("pair-waiting", "配对流程", "等待扫码／二维码有效", "PairingWindow",
     "二维码五分钟有效；关闭窗口后会话继续，不自动轮换二维码。",
     progress="等待扫码，剩余 04:32")
case("add-waiting", "配对流程", "添加设备／等待扫码", "PairingWindow",
     "通过主窗口添加第二台设备，已有连接继续工作。",
     progress="等待扫码，剩余 04:32", **{"adding-device": True})
case("add-preparing", "配对流程", "正在准备二维码", "PairingWindow",
     "刷新添加设备的二维码，已有连接继续工作。",
     progress="正在准备二维码…",
     **{"adding-device": True, "qr-stage": "loading", "motion-enabled": False})
case("add-confirming", "配对流程", "添加设备／正在配对", "PairingWindow",
     "目标设备确认后，同一个窗口等待新配对生效。",
     progress="正在配对…", **{"adding-device": True, "qr-stage": "loading", "motion-enabled": False})
case("pair-key-check", "配对流程", "正在配对", "PairingWindow",
     "目标设备确认后，在验证、保存和等待配对结果时共用此画面。",
     progress="正在配对…", **{"qr-stage": "loading", "motion-enabled": False})
case("pair-credential-ready", "配对流程", "正在准备接收端", "PairingWindow",
     "内部处于 credential_ready，新接收端此时仍未就绪。",
     progress="正在准备接收端…", **{"qr-stage": "loading", "motion-enabled": False})
case("pair-uncertain", "配对流程", "正在核实注册结果", "PairingWindow",
     "沿用同一持久化操作，不能用通用重试按钮绕过一次性凭据领取规则。",
     progress="正在核实注册结果…", **{"qr-stage": "loading", "motion-enabled": False})
case("pair-issued", "配对流程", "凭据已保存／首次连接中", "PairingWindow",
     "接收凭据已保存，但首次接收连接尚未成功，不提前显示就绪。",
     progress="正在连接接收服务…", **{"qr-stage": "loading", "motion-enabled": False})
case("pair-done", "配对流程", "配对完成", "PairingWindow",
     "首次接收连接成功后才显示此状态；窗口隐藏时安静完成。",
     **{"qr-stage": "complete"})
case("pair-added", "配对流程", "设备已添加", "PairingWindow",
     "不重新领取接收凭据，不中断原有连接，不影响其他配对。",
     **{"adding-device": True, "qr-stage": "complete", "device-name": "备用设备"})
PAIRING_PROBLEMS = (
    ("create-failed", "二维码准备失败", "配对二维码未能准备好，可以刷新二维码。",
     {"progress": "无法准备配对二维码。"}),
    ("failed", "配对失败", "配对确定失败后，统一提示并允许刷新二维码。", {}),
    ("expired", "二维码已过期", "不再显示可扫描的旧二维码；主动刷新才创建新会话和材料。",
     {"progress": "二维码已过期。"}),
    ("canceled", "配对已取消", "服务端确认此次配对已取消。",
     {"progress": "配对已取消。"}),
)
for suffix, title, behavior, properties in PAIRING_PROBLEMS:
    case(f"pair-{suffix}", "配对流程", title, "PairingWindow", behavior,
         **properties, **{"qr-stage": "problem"})
    case(f"add-{suffix}", "配对流程", title, "PairingWindow", behavior,
         **properties, **{"adding-device": True, "qr-stage": "problem"})

case("settings-default", "设置", "默认设置", "SettingsWindow",
     "开机启动开启，快捷键未设置，弹窗三十秒，语言默认跟随系统；没有主题、缓存到期或历史设置。")
case("settings-capture", "设置", "录入快捷键", "SettingsWindow",
     "Esc 取消，Enter 应用有效组合；新组合注册成功前保留旧快捷键。",
     capturing=True, shortcut="Ctrl + Alt + V")
case("settings-shortcut-invalid", "设置", "快捷键组合无效", "SettingsWindow",
     "在字段旁提示，保留原有有效设置和注册。",
     shortcut="Ctrl + Alt + V", **{"shortcut-error": "请输入完整且受支持的快捷键组合。"})
case("settings-shortcut-conflict", "设置", "快捷键已被占用", "SettingsWindow",
     "新注册失败，继续保留原有可用快捷键。",
     shortcut="Ctrl + Alt + V", **{"shortcut-error": "此快捷键已被占用，原有快捷键保持不变。"})
case("settings-startup-failed", "设置", "开机启动设置失败", "SettingsWindow",
     "显示实际关闭状态；接收继续工作；自动启动不主动打开此页报错。",
     startup=False, **{"startup-error": "无法启用开机启动，可再次打开开关重试。"})
case("settings-duration-invalid", "设置", "弹窗时长无效", "SettingsWindow",
     "保留上次保存的值，输入范围的具体上限仍待确定。",
     **{"duration-draft": "0", "duration-error": "请输入大于零的整数秒数。"})
case("settings-duration-save-failed", "设置", "弹窗时长保存失败", "SettingsWindow",
     "保存失败时恢复上次保存的时长，并在字段旁提示重试。",
     **{"duration": "45", "duration-draft": "45", "duration-error": "无法保存弹窗显示时长，请重试。"})
case("settings-language-zh", "设置", "手动选择中文", "SettingsWindow",
     "正式应用手动使用中文，不再跟随系统语言；此演示始终只展示中文。",
     **{"language-index": 1})

TEST_OTP = "482913"
case("sms-otp", "短信与复制", "含验证码的短信", "SmsWindow",
     "完整保留固定测试短信原文；按钮复制完整验证码后关闭弹窗。",
     sms="【鹊信】这是一条测试短信，验证码 482913。请勿用于真实登录。")
case("sms-no-otp", "短信与复制", "不含验证码的短信", "SmsWindow",
     "标识下直接显示完整短信正文，保留之前的最新有效验证码。",
     otp="", sms="【鹊信】您的包裹已送达前台。\n请于 20:00 前领取。")
case("sms-long", "短信与复制", "较长的短信原文", "SmsWindow",
     "弹窗随短信正文增高，达到上限后只滚动正文。",
     sms="【鹊信】验证码 482913。\n\n" +
     "\n\n".join(f"短信原文第 {i} 段。这里用于展示较长短信，所有内容按收到的原文显示，不省略、不改写。"
                 for i in range(1, 9)))
case("sms-long-code", "短信与复制", "较长或非数字验证码", "SmsWindow",
     "长验证码末尾显示省略号；复制完整原始值。",
     otp="ABC-482913-xyz-LONG-ORIGINAL-VALUE", sms="您的原始验证码为 ABC-482913-xyz-LONG-ORIGINAL-VALUE。")

def render(source, component, properties, props, output):
    props.write_text(json.dumps(properties, ensure_ascii=True), encoding="utf-8")
    subprocess.run(["slint-viewer", "--style", "fluent", "--component", component,
                    "--load-data", str(props), "--screenshot", str(output), str(source)],
                   check=True, capture_output=True, text=True)


def encode_motion(frames, output, filters="format=bgra"):
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
                    "-framerate", "20", "-i", str(frames / "frame-%03d.png"),
                    "-vf", filters, "-c:v", "libwebp_anim", "-lossless", "1",
                    "-loop", "0", str(output)], check=True)


def render_welcome_motion(source, shots):
    with tempfile.TemporaryDirectory(prefix="pin-glide-motion-") as scratch:
        frames = Path(scratch)
        props = frames / "props.json"
        for frame in range(128):
            seconds = frame / 20
            output = frames / f"frame-{frame:03}.png"
            if seconds < .24:
                shutil.copyfile(shots / "welcome-idle.png", output)
            elif 2.80 <= seconds <= 5.52:
                shutil.copyfile(shots / "welcome-arrived.png", output)
            else:
                render(source, "WelcomeWindow",
                       {"motion-enabled": False, "preview-seconds": seconds}, props, output)
        encode_motion(frames, shots / "welcome-motion.webp")


def render_pairing_motion(source, shots):
    preparing = next(item for item in CASES if item["key"] == "pair-preparing")
    with tempfile.TemporaryDirectory(prefix="pin-glide-qr-motion-") as scratch:
        frames = Path(scratch)
        props = frames / "props.json"
        for frame in range(36):
            render(source, preparing["component"],
                   {**preparing["properties"], "preview-seconds": frame / 20},
                   props, frames / f"frame-{frame:03}.png")
        # Crop the fixed QR card so every loading state uses the same animation.
        encode_motion(frames, shots / "pair-preparing.webp", "crop=176:176:132:88,format=bgra")
    for item in CASES:
        if item["properties"].get("qr-stage") == "loading":
            item["motion"] = "shots/pair-preparing.webp"


def build():
    keys = [item["key"] for item in CASES]
    assert len(keys) == len(set(keys)), "Duplicate state key"
    assert {"pair-credential-ready", "pair-issued", "pair-uncertain",
            "sms-no-otp", "main-one-key", "add-waiting", "main-two-phones"} <= set(keys)
    dimensions = dict(WelcomeWindow=(440, 560), MainWindow=(400, 576), PairingWindow=(440, 560),
                      SettingsWindow=(420, 392))
    source = ROOT / "ui.slint"
    subprocess.run(["slint-viewer", "--check", "--style", "fluent", str(source)], check=True)
    shots = ROOT / "shots"
    shots.mkdir(exist_ok=True)
    for item in CASES:
        with tempfile.TemporaryDirectory(prefix="pin-glide-ui-") as scratch:
            props = Path(scratch) / "props.json"
            output = shots / f"{item['key']}.png"
            if item["component"] == "SmsWindow":
                item["properties"].setdefault("otp", TEST_OTP)
                item["sms"] = item["properties"]["sms"]
                item["otp"] = item["properties"]["otp"]
            render(source, item["component"], item["properties"], props, output)
            header = output.read_bytes()[:24]
            assert header[:8] == b"\x89PNG\r\n\x1a\n", output
            size = struct.unpack(">II", header[16:24])
            if item["component"] == "SmsWindow":
                width, height = size
                assert width == 380 and 89 <= height <= 240, output
            else:
                assert size == dimensions[item["component"]], output
            item["image"] = f"shots/{item['key']}.png"
            item["size"] = size
            if item["component"] == "MainWindow":
                props = item["properties"]
                item["devices"] = props.get("device-names", ["我的设备", "备用设备"])
                if len(item["devices"]) > 3:
                    # Browser slices need the rows below the native scroll viewport.
                    item["scrollHeight"] = max(dimensions["MainWindow"][1], 304 + 49 * len(item["devices"]))
                    item["scrollImage"] = f"shots/{item['key']}-scroll.png"
                    render(source, "MainWindow", {**props, "preview-height": item["scrollHeight"]},
                           Path(scratch) / "props.json", ROOT / item["scrollImage"])
            if item["properties"].get("qr-stage") == "problem":
                item["refresh"] = True
            if item["properties"].get("adding-device"):
                item["back"] = True
            if item["component"] == "SettingsWindow":
                props = item["properties"]
                item["back"] = True
                # shortcut: the HTML duration control overlays a Slint screenshot; remove offsets for live rendering.
                item["durationTop"] = 244 + (26 if any(
                    props.get(key) for key in ("capturing", "shortcut-error", "startup-error")
                ) else 0)
                if "duration-error" in props:
                    item["durationDraft"] = props["duration-draft"]
                    item["durationError"] = props["duration-error"]
            if item["key"] in STATE_ALIASES:
                item["aliases"] = STATE_ALIASES[item["key"]]
    render_welcome_motion(source, shots)
    render_pairing_motion(source, shots)
    features = {"欢迎": "welcome", "主窗口": "main", "配对流程": "pairing", "设置": "settings",
                "短信与复制": "messages"}
    add_flow = ("add-preparing", "add-waiting", "add-confirming", "pair-added")
    first_problems = tuple(f"pair-{suffix}" for suffix, *_ in PAIRING_PROBLEMS)
    add_problems = tuple(f"add-{suffix}" for suffix, *_ in PAIRING_PROBLEMS)
    overrides = dict.fromkeys(add_flow + add_problems, "add-phone")
    groups = {
        "main": {
            "正常使用": ("main-ready", "main-two-phones", "main-many-devices"),
            "管理配对或恢复使用": ("main-no-phones", "main-one-key", "main-local-r",
                                  "main-revoked"),
        },
        "pairing": {
            "首次扫码与连接": ("pair-preparing", "pair-waiting", "pair-key-check",
                          "pair-credential-ready", "pair-uncertain", "pair-issued", "pair-done"),
            "配对遇到问题": first_problems,
        },
        "add-phone": {
            "扫码与完成": add_flow,
            "配对遇到问题": add_problems,
        },
    }
    data_dir = ROOT / "data"
    data_dir.mkdir(exist_ok=True)
    for item in CASES:
        item["feature"] = overrides.get(item["key"], features[item["group"]])
    pages = (*features.values(), "add-phone")
    for feature in pages:
        assert (ROOT / f"{feature}.html").is_file(), feature
        subset = [item for item in CASES if item["feature"] == feature]
        assert subset, feature
        if feature == "welcome":
            continue
        if feature in groups:
            by_key = {item["key"]: item for item in subset}
            ordered = [(key, group) for group, keys in groups[feature].items() for key in keys]
            assert len(ordered) == len(by_key) and {key for key, _ in ordered} == by_key.keys()
            subset = [dict(by_key[key], group=group) for key, group in ordered]
        data = json.dumps([{key: item[key] for key in ("key", "group", "title", "component", "image", "size", "devices",
                                                      "scrollImage", "motion", "aliases", "refresh", "back",
                                                      "durationTop", "durationDraft", "durationError", "sms", "otp")
                            if key in item}
                           for item in subset], indent=2, ensure_ascii=True)
        (data_dir / f"{feature}.js").write_text(f"window.UI_CASES = {data};\n", encoding="utf-8")
    assert sum(len([item for item in CASES if item["feature"] == feature])
               for feature in pages) == len(CASES)
    version = subprocess.check_output(["slint-viewer", "--version"], text=True).strip()
    rendered = sum(bool(item["component"]) for item in CASES)
    print(json.dumps(dict(renderer=version, states=len(CASES), native_renders=rendered,
                          pages={feature: sum(item["feature"] == feature for item in CASES)
                                 for feature in pages})))


if __name__ == "__main__":
    build()
