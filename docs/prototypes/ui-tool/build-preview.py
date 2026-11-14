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


case("main-ready", "主窗口", "就绪", "MainWindow",
     "主动打开主窗口；不显示短信历史、手机在线状态或验证码列表。")
case("main-unpaired", "主窗口", "未就绪／首次设置", "MainWindow",
     "首次主动启动先显示配对；之后可查看此主窗口状态。",
     status="未就绪", reason="配对手机后，即可接收新短信。", **{"phone-count": 0})
case("main-no-phones", "主窗口", "未就绪／没有已配对手机", "MainWindow",
     "接收凭据仍有效时添加手机，沿用现有接收端，不创建新身份。",
     status="未就绪", reason="还没有已配对的手机，请添加手机。", **{"phone-count": 0})
case("main-reconnect", "主窗口", "未就绪／正在重连", "MainWindow",
     "按既定退避规则自动重连，不反复弹出报错，也不增加第三种就绪状态。",
     status="未就绪", reason="连接已中断，正在自动重试。")
case("main-network", "主窗口", "未就绪／网络不可用", "MainWindow",
     "等待网络恢复并按规则重连，不提供关闭安全校验的选项。",
     status="未就绪", reason="暂时无法连接服务，正在自动重试。")
case("main-conflict", "主窗口", "未就绪／已有其他连接", "MainWindow",
     "继续自动重试，不提供抢占连接或重置配对按钮。",
     status="未就绪", reason="已有其他连接正在使用，稍后自动重试。")
case("main-local-r", "主窗口", "本地接收凭据不可用", "MainWindow",
     "停止重连；只有主动选择“重新注册”才开始创建新接收端。",
     status="未就绪", reason="无法读取本地接收凭据。",
     notice="请重新注册接收端，并在各台 Android 手机上移除旧配对。",
     action="重新注册", **{"phone-count": 0, "notice-error": True})
case("main-one-key", "主窗口", "一台手机需要处理", "MainWindow",
     "仅用于已确认的本地配对密钥读取失败；其他手机和接收连接继续工作。",
     notice="一台手机需要重新配对，其他手机仍可正常接收。", attention=True)
case("main-revoked", "主窗口", "接收端已被移除", "MainWindow",
     "收到权威撤销通知后清理身份、密钥、待完成任务、短信和验证码，保留普通设置。",
     status="未就绪", reason="此接收端已被移除。",
     notice="如需继续使用，请主动重新注册此接收端。",
     action="重新注册", **{"phone-count": 0, "notice-error": True})
case("main-protocol", "主窗口", "连接协议异常", "MainWindow",
     "显示普通连接错误，不强制进入更新页面，也不改变重试规则。",
     status="未就绪", reason="连接数据格式异常，正在自动重试。",
     notice="上次连接使用了不支持的消息格式。", **{"notice-error": True})
case("main-invalid-message", "主窗口", "一条短信无法读取", "MainWindow",
     "不弹出无效短信，不推断密钥丢失，不暴露敏感诊断，继续接收后续短信。",
     notice="一条短信无法读取，之前的有效验证码不受影响。", **{"notice-error": True})
case("main-tray-failed", "主窗口", "托盘图标暂不可用", "MainWindow",
     "仅在主动打开窗口时显示；自动启动不会为此弹出窗口。",
     notice="托盘图标暂不可用，可从开始菜单重新打开鹊信。", **{"notice-error": True})

case("pair-preparing", "配对流程", "正在准备二维码", "PairingWindow",
     "会话和握手材料准备好之前，不显示二维码。",
     progress="正在准备安全配对二维码…", **{"show-qr": False})
case("pair-waiting", "配对流程", "等待扫码／二维码有效", "PairingWindow",
     "二维码五分钟有效；关闭窗口不取消配对，不自动轮换二维码。",
     progress="等待手机扫码，剩余 04:32")
case("pair-authorized", "配对流程", "手机已获授权", "PairingWindow",
     "只有实际会话或握手进展才能推进状态，不能仅凭扫码猜测已经确认。",
     progress="手机已连接，正在建立安全配对…", **{"show-qr": False})
case("pair-key-check", "配对流程", "正在验证安全连接", "PairingWindow",
     "显示可理解的进度，不暴露挑战数据，也不提前宣告就绪。",
     progress="正在验证安全连接…", **{"show-qr": False})
case("pair-saving", "配对流程", "正在安全保存配对数据", "PairingWindow",
     "只有受保护的数据保存成功，才能推进配对。",
     progress="正在安全保存配对数据…", **{"show-qr": False})
case("pair-wait-other", "配对流程", "等待双方完成确认", "PairingWindow",
     "不猜测对端进度，不显示虚构的百分比。",
     progress="正在等待配对确认…", **{"show-qr": False})
case("pair-credential-ready", "配对流程", "正在准备接收端", "PairingWindow",
     "内部处于 credential_ready，新接收端此时仍未就绪。",
     progress="正在准备接收端…", **{"show-qr": False})
case("pair-uncertain", "配对流程", "正在核实注册结果", "PairingWindow",
     "沿用同一持久化操作，不能用通用重试按钮绕过一次性凭据领取规则。",
     progress="正在核实注册结果…",
     notice="操作仍在进行，关闭窗口不会中断。", **{"show-qr": False})
case("pair-issued", "配对流程", "凭据已保存／首次连接中", "PairingWindow",
     "接收凭据已保存，但首次接收连接尚未成功，不提前显示就绪。",
     progress="正在连接接收服务…", **{"show-qr": False})
case("pair-done", "配对流程", "新接收端已就绪", "PairingWindow",
     "首次接收连接成功后才显示此状态；窗口隐藏时安静完成。",
     heading="就绪", detail="手机已与这台电脑配对。",
     progress="现在可以接收新短信。", action="完成", complete=True, **{"show-qr": False})
case("pair-added", "配对流程", "已添加另一台手机", "PairingWindow",
     "不重新领取接收凭据，不中断原有连接，不影响其他配对。",
     heading="手机已配对", detail="新手机已与此接收端配对。",
     progress="其他手机的接收未被中断。", action="完成", complete=True,
     **{"show-qr": False, "completion-detail": "原有接收连接保持不变。"})
case("pair-expired", "配对流程", "二维码已过期", "PairingWindow",
     "不再显示可扫描的旧二维码；主动刷新才创建新会话和材料。",
     progress="二维码已过期。", action="刷新二维码", **{"show-qr": False})
case("pair-create-failed", "配对流程", "二维码准备失败", "PairingWindow",
     "重试创建会话，不显示空白或损坏的二维码，不误报成功。",
     progress="无法准备配对二维码。", notice="暂时无法连接服务。",
     action="重试", error=True, **{"show-qr": False})
case("pair-disconnected", "配对流程", "配对连接中断", "PairingWindow",
     "保留有效会话，核实实际进度或等待会话终态。",
     progress="正在检查配对连接…", notice="配对连接已中断。",
     **{"show-qr": False})
case("pair-auth-failed", "配对流程", "安全确认失败", "PairingWindow",
     "不显示就绪；安全结束旧操作后才能重新配对。",
     progress="无法完成安全配对确认。",
     notice="已有手机的配对和密钥保持不变。", error=True, **{"show-qr": False})
case("pair-save-failed", "配对流程", "安全保存失败", "PairingWindow",
     "不误报完成；保留已有数据，按安全规则处理未完成操作。",
     progress="无法安全保存配对数据。",
     notice="请检查本地存储权限，已有手机不受影响。", error=True, **{"show-qr": False})
case("pair-canceled", "配对流程", "配对会话已结束或失效", "PairingWindow",
     "关闭窗口不是取消原因；后续预期的策略关闭不会覆盖此结果。",
     progress="此配对二维码已失效。",
     notice="此次配对已结束，可主动创建新的二维码。", action="创建新二维码",
     **{"show-qr": False})
case("pair-rejected", "配对流程", "配对授权未通过", "PairingWindow",
     "仅显示 Windows 实际收到的原因；未收到的拒绝原因由 Android 显示。",
     progress="手机未能与此接收端配对。",
     notice="请查看 Android 应用中的配对提示。", error=True, **{"show-qr": False})
case("new-registration-confirm", "配对流程", "确认重新注册", "ConfirmationWindow",
     "仅由主动“重新注册”打开；取消不改变数据，注册失败保留旧本地数据。")

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
case("settings-shortcut-failed", "设置", "快捷键修改或清除失败", "SettingsWindow",
     "显示真实系统状态，不能在注销失败时声称已经清除。",
     shortcut="Ctrl + Alt + V", **{"shortcut-error": "快捷键修改失败，继续使用原有设置。"})
case("settings-cleared", "设置", "快捷键已清除", "SettingsWindow",
     "显示未设置，不弹成功提示；接收和弹窗复制不受影响。")
case("settings-startup-failed", "设置", "开机启动设置失败", "SettingsWindow",
     "显示实际关闭状态；接收继续工作；自动启动不主动打开此页报错。",
     startup=False, notice="无法启用开机启动，可重新勾选重试。")
case("settings-duration-invalid", "设置", "弹窗时长无效", "SettingsWindow",
     "保留上次保存的值，输入范围的具体上限仍待确定。",
     **{"duration-error": "请输入大于零的整数秒数。"})
case("settings-save-failed", "设置", "设置保存失败", "SettingsWindow",
     "保留最后保存成功的配置和实际系统状态，不显示虚假的成功结果。",
     notice="设置保存失败，继续使用上次保存的设置。")
case("settings-language-zh", "设置", "手动选择中文", "SettingsWindow",
     "正式应用手动使用中文，不再跟随系统语言；此演示始终只展示中文。",
     **{"language-index": 1})
case("settings-language-en", "设置", "手动选择 English", "SettingsWindow",
     "正式应用应切换为英文；此演示仅展示选项已选中，不另做英文界面。",
     **{"language-index": 2})
case("settings-language-save-failed", "设置", "语言偏好保存失败", "SettingsWindow",
     "保存失败时保留原有语言偏好；此处仍为默认的跟随系统，不改变配对或接收。",
     notice="语言设置保存失败，继续跟随系统语言。")

case("sms-otp", "短信与复制", "含验证码的短信", "SmsWindow",
     "完整保留固定测试短信原文；按钮只复制当前弹窗的原始验证码。")
case("sms-no-otp", "短信与复制", "不含验证码的短信", "SmsWindow",
     "显示完整原文；提案是隐藏复制按钮；保留之前的最新有效验证码。",
     otp="", sms="【鹊信】您的包裹已送达前台。\n请于 20:00 前领取。")
case("sms-long", "短信与复制", "较长的短信原文", "SmsWindow",
     "窗口尺寸固定，短信换行并可滚动，复制和关闭按钮始终可用。",
     sms="【鹊信】验证码 482913。\n\n" +
     "\n\n".join(f"短信原文第 {i} 段。这里用于展示较长短信，所有内容按收到的原文显示，不省略、不改写。"
                 for i in range(1, 9)))
case("sms-long-code", "短信与复制", "较长或非数字验证码", "SmsWindow",
     "验证码区域可横向滚动；复制原始值，不规范化或截短。",
     otp="ABC-482913-xyz-LONG-ORIGINAL-VALUE", sms="您的原始验证码为 ABC-482913-xyz-LONG-ORIGINAL-VALUE。")
case("sms-copy-failed", "短信与复制", "弹窗复制失败", "SmsWindow",
     "保留弹窗、原文和验证码，允许重试，不显示成功提示。",
     error="复制失败，请重试。")
case("feedback-no-code", "短信与复制", "快捷复制／暂无验证码", "FeedbackWindow",
     "显示短暂且不抢焦点的提示，剪贴板不变，不打开主窗口。")
case("feedback-copy-failed", "短信与复制", "最新验证码复制失败", "FeedbackWindow",
     "保留缓存，可再次触发复制；连续失败合并提示，不堆叠报错。",
     heading="复制失败", detail="无法写入剪贴板，验证码仍然保留，可再次按快捷键重试。")

case("tray-ready", "系统托盘", "就绪／有可用验证码", "TrayContentPreview",
     "正式应用使用系统原生菜单；此图仅示意菜单内容，不是实际托盘截图。")
case("tray-empty", "系统托盘", "没有缓存的验证码", "TrayContentPreview",
     "禁用“复制最新验证码”，不向剪贴板写入空字符串。", **{"has-code": False})
case("tray-unpaired", "系统托盘", "首次使用／尚未配对", "TrayContentPreview",
     "自动启动仅驻留托盘；主动配对才打开首次配对窗口。",
     ready=False, **{"has-code": False, "can-configure": False})
case("tray-revoked", "系统托盘", "已被移除／需要重新注册", "TrayContentPreview",
     "先从主窗口主动重新注册，再允许添加手机。",
     ready=False, **{"has-code": False, "can-pair": False})

case("quiet-auto-start", "安静处理", "自动启动", None,
     "不打开应用窗口；未配对或启动集成有错误时，也只驻留托盘。")
case("quiet-copy-success", "安静处理", "复制成功", None,
     "不弹成功提示；当前短信弹窗关闭，快捷键复制保持安静。")
case("quiet-hidden-pair", "安静处理", "窗口隐藏时完成配对", None,
     "不强制打开窗口，不弹成功提示；重新打开时显示实际结果。")
case("quiet-lock", "安静处理", "锁定与解锁", None,
     "锁定时关闭短信弹窗，不显示或保留锁定期间的短信原文；解锁不重放旧弹窗。")
case("quiet-suspend", "安静处理", "休眠与恢复", None,
     "不激活窗口；休眠时暂停重试，恢复后安静地检查连接。")
case("quiet-ack", "安静处理", "收到消息／到达回执", None,
     "不显示界面；到达回执不代表解密、显示或复制成功。")
case("quiet-scanned", "安静处理", "Windows 尚未观察到扫码结果", None,
     "继续等待，不根据仅手机可见的事件虚构“已扫码”步骤。")
case("quiet-removed", "安静处理", "单个配对被移除", None,
     "只移除对应手机、弹窗和必要的最新验证码，不显示整个接收端被撤销。")
case("quiet-exit", "安静处理", "退出应用", None,
     "断开连接并释放短信和验证码内存；保留设置与配对，不清空系统剪贴板。")
case("os-installer", "安装与系统", "安装、升级与卸载", None,
     "使用标准 NSIS 与 Windows 界面，不制作仿系统弹窗；完整分支见场景清单 D01—D11。")
case("os-trust", "安装与系统", "Windows 信任与权限提示", None,
     "仅在 Windows 要求时显示 UAC 或 SmartScreen，不仿制这些界面，也不承诺消除系统警告。")


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
    features = {"主窗口": "main", "配对流程": "pairing", "设置": "settings",
                "短信与复制": "messages", "系统托盘": "tray",
                "安静处理": "lifecycle", "安装与系统": "installation"}
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
