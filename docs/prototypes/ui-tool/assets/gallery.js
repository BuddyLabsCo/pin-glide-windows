// Shared presentation only. Each HTML page loads its own functional state data.
const cases = window.UI_CASES;
const feature = document.body.dataset.feature;
const query = new URLSearchParams(location.search);
let current = cases.some(item => item.key === query.get("state"))
  ? query.get("state") : cases[0].key;

document.querySelector("#count").textContent = `${cases.length} 个状态`;
const surfaceNames = {
  MainWindow: "主窗口", PairingWindow: "配对窗口", SettingsWindow: "设置",
  SmsWindow: "短信弹窗", FeedbackWindow: "快捷复制提示",
  ConfirmationWindow: "重新注册确认", TrayContentPreview: "托盘菜单内容示意"
};
document.querySelectorAll(".feature-links a").forEach(link => {
  if (link.getAttribute("href") === `${feature}.html`) {
    link.setAttribute("aria-current", "page");
  }
});

function filtered() {
  const text = document.querySelector("#search").value.toLowerCase();
  return cases.filter(item =>
    `${item.title} ${item.behavior} ${item.key}`.toLowerCase().includes(text));
}

function list() {
  const container = document.querySelector("#list");
  container.replaceChildren();
  let previous = "";
  for (const item of filtered()) {
    if (previous !== item.group) {
      const heading = document.createElement("div");
      heading.className = "group";
      heading.textContent = item.group;
      container.append(heading);
      previous = item.group;
    }
    const button = document.createElement("button");
    button.textContent = item.title;
    button.dataset.state = item.key;
    button.className = item.key === current ? "selected" : "";
    button.onclick = () => show(item.key);
    container.append(button);
  }
}

function show(key) {
  const item = cases.find(candidate => candidate.key === key);
  if (!item) return;
  current = key;
  document.querySelector("#kicker").textContent = item.group;
  document.querySelector("#title").textContent = item.title;
  document.querySelector("#behavior").textContent = item.behavior;
  document.querySelector("#meta").textContent = item.image
    ? `${surfaceNames[item.component]}／内容区 ${item.size[0]} × ${item.size[1]} 逻辑像素／Slint Fluent`
    : "此情况不显示应用界面，或由系统呈现";

  const target = document.querySelector("#render");
  target.replaceChildren();
  if (item.image) {
    const frame = document.createElement("div");
    frame.className = "native-frame";
    if (!["SmsWindow", "FeedbackWindow"].includes(item.component)) {
      const caption = document.createElement("div");
      caption.className = "caption";
      const title = document.createElement("span");
      title.textContent = "鹊信";
      const commands = document.createElement("span");
      commands.textContent = item.component === "MainWindow" ? "− ×" : "×";
      caption.append(title, commands);
      frame.append(caption);
    }
    const image = document.createElement("img");
    image.src = item.image;
    image.alt = item.title;
    [image.width, image.height] = item.size;
    frame.append(image);
    target.append(frame);
  } else {
    const box = document.createElement("div");
    box.className = "no-surface";
    const title = document.createElement("strong");
    title.textContent = item.group === "安装与系统"
      ? "由 Windows 或 NSIS 呈现" : "不显示应用弹窗";
    const detail = document.createElement("p");
    detail.textContent = item.behavior;
    box.append(title, detail);
    target.append(box);
  }
  document.querySelector("#annotation").textContent =
    item.component === "TrayContentPreview"
      ? "仅示意菜单内容，正式应用使用 Windows 原生托盘图标与菜单。"
      : item.image
        ? "此图由真实 Slint 源码编译渲染，原型尚未接入 Windows 系统能力。"
        : "不为此新增通知，也不强制打开主窗口。";
  const url = new URL(location.href);
  url.searchParams.set("state", key);
  url.searchParams.delete("view");
  history.replaceState(null, "", url);
  list();
}

function move(delta) {
  const options = filtered();
  if (!options.length) return;
  const index = options.findIndex(item => item.key === current);
  const next = index < 0 ? (delta > 0 ? 0 : options.length - 1)
    : (index + delta + options.length) % options.length;
  show(options[next].key);
}

document.querySelector("#search").oninput = list;
document.querySelector("#previous").onclick = () => move(-1);
document.querySelector("#next").onclick = () => move(1);
document.addEventListener("keydown", event => {
  if (event.target.closest("input,select,textarea,[contenteditable]")) return;
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    event.preventDefault();
    move(event.key === "ArrowRight" ? 1 : -1);
  }
});
show(current);
