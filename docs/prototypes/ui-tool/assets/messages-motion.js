const stack = document.querySelector("#sms-stack");
const workArea = stack.parentElement;
const motionStatus = document.querySelector("#motion-status");
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");
let playbackTimer;

function previewCard(key) {
  const item = window.UI_CASES.find(entry => entry.key === key);
  const frame = document.createElement("article");
  const content = document.createElement("div");
  const image = document.createElement("img");
  frame.className = "native-frame";
  image.src = item.image;
  image.alt = item.title;
  [image.width, image.height] = item.size;
  content.append(image);
  frame.append(content);
  renderSmsPopup(frame, content, item, () => dismissCard(frame, "已手动关闭弹窗。"));
  return frame;
}

function playMotion(frame, keyframes, duration) {
  return frame.animate(keyframes, {
    duration: reducedMotion.matches ? 0 : duration,
    easing: "ease-out",
  });
}

function slideStack(update) {
  const oldRects = new Map(Array.from(stack.children, card => [card, card.getBoundingClientRect()]));
  for (const card of oldRects.keys()) {
    for (const animation of card.getAnimations()) animation.cancel();
  }
  update(oldRects);
  for (const card of stack.children) {
    const old = oldRects.get(card);
    if (!old) {
      playMotion(card, [{ opacity: 0, transform: "translateX(12px)" }, { opacity: 1, transform: "none" }], 180);
      continue;
    }
    const offset = old.top - card.getBoundingClientRect().top;
    if (offset) playMotion(card, [{ transform: `translateY(${offset}px)` }, { transform: "none" }], 220);
  }
}

function fadeOut(card, rect) {
  const area = workArea.getBoundingClientRect();
  card.classList.add("sms-leaving");
  card.inert = true;
  card.style.left = `${rect.left - area.left}px`;
  card.style.top = `${rect.top - area.top}px`;
  workArea.append(card);
  playMotion(card, [{ opacity: 1, transform: "none" }, { opacity: 0, transform: "translateX(12px)" }], 180)
    .onfinish = () => card.remove();
}

function dismissCard(card, message) {
  if (card.parentElement !== stack) return;
  slideStack(oldRects => fadeOut(card, oldRects.get(card)));
  motionStatus.textContent = message;
}

function resetStack(keys = ["sms-no-otp", "sms-long", "sms-otp"]) {
  clearTimeout(playbackTimer);
  for (const card of workArea.querySelectorAll(".sms-leaving")) card.remove();
  stack.replaceChildren(...keys.map(previewCard));
  motionStatus.textContent = "可播放示例，也可点击任意短信关闭。";
}

function playDismissal(position, message) {
  resetStack();
  const card = stack.children[position];
  motionStatus.textContent = position === 2 ? "最下方的弹窗即将超时。" : "即将关闭中间的弹窗。";
  playbackTimer = setTimeout(() => dismissCard(card, message), 1000);
}

function showIncoming(key) {
  slideStack(oldRects => {
    stack.prepend(previewCard(key));
    while (stack.childElementCount > 5 || stack.offsetHeight > workArea.clientHeight - 24) {
      const oldest = stack.lastElementChild;
      fadeOut(oldest, oldRects.get(oldest));
    }
  });
}

document.querySelector("#play-new-message").onclick = () => {
  resetStack([]);
  motionStatus.textContent = "等待新短信。";
  playbackTimer = setTimeout(() => {
    showIncoming("sms-otp");
    motionStatus.textContent = "第一条短信在右下角弹出。";
    playbackTimer = setTimeout(() => {
      showIncoming("sms-no-otp");
      motionStatus.textContent = "第二条短信在第一条上方弹出。";
    }, 1000);
  }, 500);
};
document.querySelector("#play-timeout").onclick = () => playDismissal(2, "最下方的弹窗已超时。");
document.querySelector("#play-middle").onclick = () => playDismissal(1, "已关闭中间的弹窗。");
document.querySelector("#play-arrival").onclick = () => {
  resetStack();
  motionStatus.textContent = "即将收到一条新短信。";
  playbackTimer = setTimeout(() => {
    showIncoming("sms-long-code");
    motionStatus.textContent = "新短信出现在顶部，最早的一条因空间不足而关闭。";
  }, 1000);
};
document.querySelector("#reset-stack").onclick = () => resetStack();
resetStack();
