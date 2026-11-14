// Run in the rendered prototype with agent-browser eval --stdin.
(() => {
  let checks = 0;
  const check = (condition, label) => {
    if (!condition) throw new Error(label);
    checks++;
  };
  const click = selector => document.querySelector(selector).click();
  const scenario = value => {
    const select = document.querySelector("#scenario");
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  };
  for (const key of ["A", "B", "C"]) {
    click(`[data-variant="${key}"]`);
    check(new URL(location.href).searchParams.get("variant") === key, `${key}: URL selection`);
    check(document.querySelectorAll("#app .phone-row").length === 2, `${key}: paired phones`);
    check(document.querySelector('#app [role="switch"]').getAttribute("aria-checked") === "true", `${key}: startup enabled`);
    check(document.querySelector("#app").textContent.includes("Unassigned"), `${key}: shortcut unassigned`);
    check(document.querySelector(".sms").textContent === "【鹊信】这是一条测试短信，验证码 482913。请勿用于真实登录。", `${key}: unchanged fixed SMS`);
    scenario("no-otp");
    check(!document.querySelector('[data-action="copy"]'), `${key}: empty OTP has no copy action`);
    check(document.querySelector(".sms").textContent.includes("20:00."), `${key}: full no-OTP message`);
    scenario("copy-failed");
    click('[data-action="copy"]');
    check(!!document.querySelector(".popup .failure"), `${key}: failed copy keeps popup`);
    scenario("ready");
    click('[data-action="copy"]');
    check(!document.querySelector(".popup"), `${key}: success closes popup`);
    check(!document.querySelector("#popup .sms"), `${key}: closed popup releases SMS body`);
    click('[data-action="reset-popup"]');
    scenario("expired");
    check(!!document.querySelector('[data-action="refresh"]'), `${key}: explicit QR refresh`);
    click('[data-action="refresh"]');
    check(!document.querySelector(".qr-overlay"), `${key}: refresh restores QR preview`);
    scenario("offline");
    check(document.querySelector("#app").textContent.includes("Not Ready"), `${key}: reconnect reason uses Not Ready`);
    scenario("ready");
    check(document.documentElement.scrollWidth <= window.innerWidth, `${key}: no horizontal overflow`);
  }
  click('[data-variant="A"]');
  const select = document.querySelector(".duration");
  select.value = "60";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  click('[data-variant="B"]');
  check(document.querySelector(".popup-bottom").textContent.includes("30 s"), "duration change preserves existing popup");
  click('[data-action="close-popup"]');
  click('[data-action="reset-popup"]');
  check(document.querySelector(".popup-bottom").textContent.includes("60 s"), "new popup gets changed duration");
  click("#theme");
  check(document.querySelector("#board").classList.contains("dark"), "dark preview");
  check(new URL(location.href).searchParams.get("theme") === "dark", "theme URL");
  click("#theme");
  click('[data-variant="A"]');
  click("#prev");
  check(new URL(location.href).searchParams.get("variant") === "C", "previous wraps");
  click("#next");
  check(new URL(location.href).searchParams.get("variant") === "A", "next wraps");
  click('[data-action="hide-main"]');
  check(document.querySelector("#app").textContent.includes("hidden to tray"), "main window hides to tray");
  click('[data-action="show-main"]');
  check(!!document.querySelector(".sidebar"), "main window reopens");
  return { passed: checks, scope: "Prototype behavior only; no product or Windows integration validation." };
})();
