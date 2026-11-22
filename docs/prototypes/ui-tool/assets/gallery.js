const cases = window.UI_CASES;
const state = document.querySelector("#state");
const render = document.querySelector("#render");

let group;
let prepareTimer;
for (const item of cases) {
  if (item.group !== group?.label) {
    group = document.createElement("optgroup");
    group.label = item.group;
    state.append(group);
  }
  group.append(new Option(item.title, item.key));
}

function showImage(item) {
  const image = document.createElement("img");
  image.src = item.image;
  image.alt = item.title;
  [image.width, image.height] = item.size;
  const currentMotion = render.querySelector(".qr-motion:not(.qr-motion-leaving)");
  if (item.motion && currentMotion?.getAttribute("src") === item.motion) {
    currentMotion.previousElementSibling.replaceWith(image);
    return;
  }

  const frame = document.createElement("div");
  frame.className = "native-frame";
  if (item.component !== "SmsWindow") {
    const caption = document.createElement("div");
    caption.className = "caption";
    const title = document.createElement("span");
    title.textContent = "鹊信";
    const controls = document.createElement("span");
    controls.textContent = item.component === "MainWindow" ? "−    ×" : "×";
    caption.append(title, controls);
    frame.append(caption);
  }
  if (item.motion || item.refresh || item.back || typeof renderDeviceNames === "function" ||
      typeof renderDurationSetting === "function") {
    const content = document.createElement("div");
    content.className = "pairing-art";
    content.append(image);
    if (item.component === "MainWindow") {
      const settings = document.createElement("button");
      settings.type = "button";
      settings.className = "main-settings";
      settings.title = "设置";
      settings.setAttribute("aria-label", "设置");
      settings.addEventListener("click", () => {
        location.href = `settings.html?from=${encodeURIComponent(item.key)}`;
      });
      const needsNewReceiver = ["main-local-r", "main-revoked"].includes(item.key);
      const pair = document.createElement("button");
      pair.type = "button";
      pair.className = "main-pair";
      pair.setAttribute("aria-label", "配对新设备");
      pair.addEventListener("click", () => {
        location.href = needsNewReceiver ? "pairing.html?start=1" : `add-phone.html?from=${encodeURIComponent(item.key)}`;
      });
      content.append(settings, pair);
    }
    if (item.motion) {
      const motion = document.createElement("img");
      motion.className = "qr-motion";
      motion.src = item.motion;
      motion.alt = "";
      content.append(motion);
    }
    if (item.refresh) {
      const refresh = document.createElement("button");
      refresh.type = "button";
      refresh.className = "qr-refresh";
      refresh.setAttribute("aria-label", "刷新二维码");
      refresh.addEventListener("click", prepareQr);
      content.append(refresh);
    }
    if (item.back) {
      const back = document.createElement("a");
      const destination = new URL("main.html", location.href);
      const origin = new URLSearchParams(location.search).get("from");
      destination.searchParams.set("state", origin ?? "main-ready");
      back.className = "window-back";
      back.href = destination.href;
      back.setAttribute("aria-label", "返回主窗口");
      back.title = "返回主窗口";
      content.append(back);
    }
    if (typeof renderDeviceNames === "function") renderDeviceNames(content, item);
    if (typeof renderDurationSetting === "function") renderDurationSetting(content, item);
    if (currentMotion && content.querySelector(".device-name-form") &&
        !matchMedia("(prefers-reduced-motion: reduce)").matches) {
      currentMotion.classList.add("qr-motion-leaving");
      content.append(currentMotion);
      const fade = currentMotion.animate([{ opacity: 1 }, { opacity: 0 }], 180);
      fade.onfinish = () => currentMotion.remove();
    }
    frame.append(content);
    if (typeof renderSmsPopup === "function") renderSmsPopup(frame, content, item);
  } else {
    frame.append(image);
  }
  render.replaceChildren(frame);
}

function show(key) {
  clearTimeout(prepareTimer);
  const item = cases.find(candidate => candidate.key === key || candidate.aliases?.includes(key)) ?? cases[0];
  state.value = item.key;
  for (const link of document.querySelectorAll("[data-state]")) {
    link.hidden = !link.dataset.state.split(" ").includes(item.key);
  }
  showImage(item);
  const url = new URL(location.href);
  url.searchParams.set("state", item.key);
  history.replaceState(null, "", url);
}

function prepareQr() {
  show(state.dataset.prepare);
  prepareTimer = setTimeout(() => show(state.dataset.initial), 650);
}

state.addEventListener("change", () => show(state.value));
for (const link of document.querySelectorAll('.steps a[href^="?state="]')) {
  link.addEventListener("click", event => {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    history.pushState(null, "", link.href);
    show(new URL(link.href).searchParams.get("state"));
  });
}
window.addEventListener("popstate", () => show(new URL(location.href).searchParams.get("state")));
const initialUrl = new URL(location.href);
if (state.dataset.prepare && initialUrl.searchParams.has("start")) {
  initialUrl.searchParams.delete("start");
  history.replaceState(null, "", initialUrl);
  prepareQr();
} else {
  show(initialUrl.searchParams.get("state") ?? state.dataset.initial);
}
