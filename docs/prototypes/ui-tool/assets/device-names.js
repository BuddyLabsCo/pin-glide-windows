const nameStorageKey = "pin-glide-preview-device-names";
const defaultNames = ["我的设备", "备用设备"];
const savedNames = JSON.parse(sessionStorage.getItem(nameStorageKey) || "[]");

function nameMatchKey(name) {
  // Match the Sender's per-character comparison without expanding ß into SS.
  return [...name].map(character => {
    const upper = character.toUpperCase();
    const simpleUpper = [...upper].length === 1 ? upper : character;
    return simpleUpper === "\u0130" ? "i" : simpleUpper.toLowerCase();
  }).join("");
}

function validateDeviceName(value, otherNames) {
  const name = value.replace(/^[\p{Z}\t-\r\u001c-\u001f]+|[\p{Z}\t-\r\u001c-\u001f]+$/gu, "");
  let error = "";
  if (!name) error = "请填写设备名称。";
  else if ([...name].length > 40) error = "设备名称最多 40 个字符。";
  else if (/[\uD800-\uDFFF]/u.test(name)) error = "名称包含无效字符，请重新输入。";
  else if (otherNames.some(other => nameMatchKey(other) === nameMatchKey(name))) {
    error = "已有一台同名设备，请换一个名称。";
  }
  return { name, error };
}

function saveDeviceName(index, name) {
  savedNames[index] = name;
  sessionStorage.setItem(nameStorageKey, JSON.stringify(savedNames));
}

function addDeviceNameForm(content, index) {
  const scenario = document.querySelector("#name-scenario");
  const firstName = savedNames[0] ?? defaultNames[0];
  const initialName = savedNames[index] ?? defaultNames[index];
  const form = document.createElement("form");
  form.className = "device-name-form";
  const field = document.createElement("input");
  field.type = "text";
  field.value = initialName;
  field.placeholder = "输入设备名称";
  field.autocomplete = "off";
  field.setAttribute("aria-label", "设备名称");
  field.setAttribute("aria-describedby", "device-name-help");
  const help = document.createElement("div");
  help.id = "device-name-help";
  help.className = "device-name-help";
  help.setAttribute("aria-live", "polite");
  const done = document.createElement("button");
  done.type = "submit";
  done.className = "device-name-done";
  done.setAttribute("aria-label", "完成");
  form.append(field, help, done);
  content.append(form);

  function showError(error) {
    help.textContent = error;
    field.setAttribute("aria-invalid", error ? "true" : "false");
  }

  scenario.value = "normal";
  scenario.onchange = () => {
    field.value = {
      normal: initialName,
      empty: "",
      duplicate: firstName,
      "too-long": "我的备用设备".repeat(7),
    }[scenario.value];
    showError("");
  };

  field.addEventListener("input", () => showError(""));
  form.addEventListener("submit", event => {
    event.preventDefault();
    // The first-pairing preset simulates an existing device for preview only.
    const others = index === 1 || scenario.value === "duplicate" ? [firstName] : [];
    const { name, error } = validateDeviceName(field.value, others);
    if (error) {
      showError(error);
      field.focus();
      return;
    }
    if (index === 0) savedNames.length = 0;
    saveDeviceName(index, name);
    location.href = index === 0 ? "main.html?state=main-ready" : "main.html?state=main-two-phones";
  });
}

function editDeviceName(form, index, devices, close) {
  const field = form.querySelector("input");
  const message = form.querySelector("#rename-error");
  form.querySelector('[value="cancel"]').onclick = close;
  field.value = savedNames[index] ?? devices[index];
  const otherNames = devices
    .map((fallback, row) => savedNames[row] ?? fallback)
    .filter((_, row) => row !== index);
  form.oninput = () => {
    message.textContent = "";
    field.removeAttribute("aria-invalid");
  };
  form.onsubmit = event => {
    event.preventDefault();
    const { name, error } = validateDeviceName(field.value, otherNames);
    if (error) {
      message.textContent = error;
      field.setAttribute("aria-invalid", "true");
      field.focus({ preventScroll: true });
      form.scrollIntoView({ block: "nearest", container: "nearest" });
      return;
    }
    saveDeviceName(index, name);
    close();
  };
}

function renderDeviceNames(content, { key: stateKey, devices, scrollImage }) {
  if (stateKey === "pair-done" || stateKey === "pair-added") {
    addDeviceNameForm(content, stateKey === "pair-done" ? 0 : 1);
    return;
  }

  if (!devices?.length) return;

  // The preview uses slices of the Slint render around the editable section.
  const listTop = 268;
  const editorHeight = "var(--rename-editor-height)";
  const image = content.querySelector("img");
  const viewport = document.createElement("div");
  viewport.className = "main-list-viewport";
  const surface = document.createElement("div");
  surface.className = "main-list-surface";
  viewport.append(surface);
  content.append(viewport);

  function slice(top, height, sourceTop) {
    const part = document.createElement("div");
    part.className = "main-list-slice";
    part.style.top = top;
    part.style.height = `${height}px`;
    part.style.backgroundImage = `url("${scrollImage ?? image.src}")`;
    part.style.backgroundPosition = `0 -${listTop + sourceTop}px`;
    surface.append(part);
  }

  function draw(openIndex = -1, focusIndex = -1) {
    const scrollTop = viewport.scrollTop;
    const height = devices.length * 49 - 1;
    surface.style.height = `calc(${height}px + ${editorHeight})`;
    surface.replaceChildren();
    if (openIndex < 0) {
      slice("0px", height, 0);
    } else {
      const gap = openIndex * 49 + 48;
      slice("0px", gap, 0);
      slice(`calc(${gap}px + ${editorHeight})`, height - gap, gap);
    }

    for (let index = 0; index < devices.length; index++) {
      const rowTop = index * 49;
      const offset = openIndex >= 0 && index > openIndex ? editorHeight : "0px";
      const name = savedNames[index] ?? devices[index];
      if (savedNames[index]) {
        const label = document.createElement("span");
        label.className = "main-device-name";
        label.style.top = `calc(${rowTop + 14}px + ${offset})`;
        label.textContent = label.title = name;
        if (stateKey === "main-one-key" && index === 0) label.classList.add("needs-repair");
        surface.append(label);
      }
      const button = document.createElement("button");
      button.type = "button";
      button.className = "main-rename";
      button.style.top = `calc(${rowTop}px + ${offset})`;
      const needsRepair = stateKey === "main-one-key" && index === 0;
      const action = openIndex === index ? "收起" : needsRepair ? "展开需要重新配对的设备" : "编辑名称";
      button.setAttribute("aria-label", `${action} ${name}`);
      button.setAttribute("aria-expanded", String(openIndex === index));
      button.onclick = () => openIndex === index ? draw(-1, index) : draw(index);
      surface.append(button);
      if (index !== openIndex) continue;
      if (needsRepair) {
        button.setAttribute("aria-controls", "device-delete-actions");
        const actions = document.querySelector("#delete-device").content.firstElementChild.cloneNode(true);
        actions.style.top = `${rowTop + 48}px`;
        actions.querySelector("button").onclick = () => {
          const remainingName = savedNames[1] ?? devices[1];
          savedNames.length = 1;
          saveDeviceName(0, remainingName);
          show("main-ready");
          render.querySelector(".main-rename").focus({ preventScroll: true });
        };
        actions.addEventListener("keydown", event => {
          if (event.key === "Escape") draw(-1, index);
        });
        surface.append(actions);
        continue;
      }
      button.setAttribute("aria-controls", "device-name-editor");
      const form = document.querySelector("#rename-device").content.firstElementChild.cloneNode(true);
      form.style.top = `${rowTop + 48}px`;
      form.classList.toggle("last-row", index === devices.length - 1);
      surface.append(form);
      const close = () => draw(-1, index);
      editDeviceName(form, index, devices, close);
      form.addEventListener("keydown", event => {
        if (event.key === "Escape") {
          event.preventDefault();
          close();
        }
      });
    }
    viewport.scrollTop = scrollTop;
    const form = surface.querySelector(".rename-form, .delete-device");
    if (form) {
      form.scrollIntoView({ block: "nearest", container: "nearest" });
      const input = form.querySelector("input");
      (input ?? surface.querySelector('[aria-expanded="true"]')).focus({ preventScroll: true });
    }
    if (focusIndex >= 0) {
      const button = surface.querySelectorAll(".main-rename")[focusIndex];
      button.scrollIntoView({ block: "nearest", container: "nearest" });
      button.focus({ preventScroll: true });
    }
  }
  draw();
}
