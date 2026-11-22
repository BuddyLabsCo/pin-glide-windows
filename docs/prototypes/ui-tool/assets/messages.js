function renderSmsPopup(frame, content, { sms, otp }) {
  const dismiss = () => {
    if (!frame.isConnected) return;
    frame.remove();
    state.selectedIndex = -1;
    state.focus();
  };
  content.classList.add("sms-popup");
  frame.onclick = dismiss;
  frame.onkeydown = event => {
    if (event.key === "Escape") dismiss();
  };

  const body = document.createElement("div");
  body.className = "sms-body";
  body.tabIndex = 0;
  body.setAttribute("role", "region");
  body.setAttribute("aria-label", "短信正文");
  body.textContent = sms;

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
    content.append(copy);
  }
  content.append(body);
}
