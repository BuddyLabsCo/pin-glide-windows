function renderSmsPopup(frame, content, { sms, otp }, onDismiss) {
  const dismiss = onDismiss ?? (() => {
    if (!frame.isConnected) return;
    frame.remove();
    state.selectedIndex = -1;
    state.focus();
  });
  content.classList.add("sms-popup");
  frame.onclick = dismiss;
  frame.onkeydown = event => {
    if (event.key === "Escape") dismiss();
  };

  const header = document.createElement("div");
  header.className = "sms-header";
  header.append(content.querySelector("img"));
  content.append(header);

  const body = document.createElement("div");
  body.className = "sms-body";
  body.tabIndex = 0;
  body.setAttribute("role", "region");
  body.setAttribute("aria-label", "短信正文");
  body.textContent = sms;

  const source = document.createElement("div");
  source.className = "sms-device-name";
  source.textContent = source.title = savedNames[0] ?? defaultNames[0];
  content.append(body, source);

  if (otp) {
    content.classList.add("has-otp");
    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "sms-copy";
    copy.setAttribute("aria-label", "复制验证码");
    copy.onclick = async event => {
      event.stopPropagation();
      await navigator.clipboard?.writeText(otp).catch(() => {});
      dismiss();
    };
    header.append(copy);
  }
}
