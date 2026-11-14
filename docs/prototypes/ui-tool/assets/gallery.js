// Shared presentation only. Each HTML page loads its own functional state data.
const cases = window.UI_CASES;
const feature = document.body.dataset.feature;
const query = new URLSearchParams(location.search);
let current = cases.some(item => item.key === query.get("state"))
  ? query.get("state") : cases[0].key;

document.querySelector("#count").textContent = `${cases.length} states`;
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
    ? `${item.component} / ${item.size[0]} x ${item.size[1]} logical content px / Slint Fluent`
    : "No app-designed surface is shown for this outcome";

  const target = document.querySelector("#render");
  target.replaceChildren();
  if (item.image) {
    const frame = document.createElement("div");
    frame.className = "native-frame";
    if (!["SmsWindow", "FeedbackWindow"].includes(item.component)) {
      const caption = document.createElement("div");
      caption.className = "caption";
      const title = document.createElement("span");
      title.textContent = "Pin Glide";
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
    title.textContent = item.group === "OS / installer"
      ? "Windows / NSIS owns this UI" : "No application popup";
    const detail = document.createElement("p");
    detail.textContent = item.behavior;
    box.append(title, detail);
    target.append(box);
  }
  document.querySelector("#annotation").textContent =
    item.component === "TrayContentPreview"
      ? "Content sketch only. Production tray icons and menus use native Windows APIs."
      : item.image
        ? "Actual compiled Slint output. Windows system integration is not implemented by this prototype."
        : "This outcome does not introduce a new application notification or force the main window open.";
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
