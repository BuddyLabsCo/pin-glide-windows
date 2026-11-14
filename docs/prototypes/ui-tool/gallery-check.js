// Run against the generated gallery with agent-browser eval -b.
(async () => {
  let checks = 0;
  const check = (condition, label) => {
    if (!condition) throw new Error(label);
    checks++;
  };
  check(cases.length === 62, "Catalogue count");
  check(new Set(cases.map(c => c.key)).size === cases.length, "Unique states");
  check(cases.filter(c => c.image).length === 51, "Native render count");
  for (const item of cases) {
    show(item.key);
    check(document.querySelector("#title").textContent === item.title, `${item.key}: title`);
    check(new URL(location.href).searchParams.get("state") === item.key, `${item.key}: URL`);
    if (item.image) {
      const image = document.querySelector("#render img");
      await image.decode();
      check(image.naturalWidth === item.size[0] && image.naturalHeight === item.size[1],
        `${item.key}: native image dimensions`);
    } else {
      check(!document.querySelector("#render img"), `${item.key}: no invented surface`);
    }
  }
  const filter = document.querySelector("#group");
  filter.value = "Tray";
  filter.dispatchEvent(new Event("change"));
  check(document.querySelectorAll("#list button").length === 4, "Tray filter");
  filter.value = "";
  filter.dispatchEvent(new Event("change"));
  const search = document.querySelector("#search");
  search.value = "credential";
  search.dispatchEvent(new Event("input"));
  check([...document.querySelectorAll("#list button")].some(b => b.dataset.state === "pair-issued"),
    "Progress-state search");
  search.value = "";
  search.dispatchEvent(new Event("input"));
  show("main-ready");
  check(document.querySelector("#render .caption").textContent.includes("− ×"), "No maximize caption");
  check(!document.querySelector("#render .caption").textContent.includes("□"), "No maximize affordance");
  const logo = document.querySelector("header .brand img");
  await logo.decode();
  check(logo.naturalWidth > 0, "Android logo loads");
  check(!document.querySelector('[id="theme"]'), "No theme control");
  return { passed: checks, scope: "Static state gallery only; no native integration claim." };
})();
