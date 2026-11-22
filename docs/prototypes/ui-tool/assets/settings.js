function renderDurationSetting(content, { durationTop, durationDraft, durationError }) {
  const setting = document.querySelector("#duration-setting").content.firstElementChild.cloneNode(true);
  const input = setting.querySelector("input");
  const error = setting.querySelector("#duration-error");
  const storageKey = "pin-glide-preview-popup-duration";
  setting.style.top = `${durationTop}px`;
  input.value = durationDraft ?? sessionStorage.getItem(storageKey) ?? "30";
  error.textContent = durationError ?? "";
  if (durationError) input.setAttribute("aria-invalid", "true");
  input.onblur = () => {
    const value = input.value.trim();
    if (!/^[0-9]*[1-9][0-9]*$/.test(value)) {
      error.textContent = "请输入大于零的整数秒数。";
      input.setAttribute("aria-invalid", "true");
      return;
    }
    sessionStorage.setItem(storageKey, value);
    input.value = value;
    error.textContent = "";
    input.removeAttribute("aria-invalid");
  };
  content.append(setting);
}
