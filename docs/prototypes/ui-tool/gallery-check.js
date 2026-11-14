// Run against the generated gallery with agent-browser eval -b.
(async () => {
  let checks = 0;
  const check = (condition, label) => {
    if (!condition) throw new Error(label);
    checks++;
  };
  check(cases.length > 0, "Feature has states");
  check(new Set(cases.map(c => c.key)).size === cases.length, "Unique states");
  check(cases.every(c => c.feature === feature), "No states from another functional boundary");
  check(location.pathname.endsWith(`/${feature}.html`), "Separate functional HTML file");
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
  const search = document.querySelector("#search");
  search.value = cases[0].key;
  search.dispatchEvent(new Event("input"));
  check([...document.querySelectorAll("#list button")].some(b => b.dataset.state === cases[0].key),
    "Feature-local search");
  search.value = "";
  search.dispatchEvent(new Event("input"));
  show(cases[0].key);
  move(-1);
  check(current === cases.at(-1).key, "Previous stays in this feature and wraps");
  move(1);
  check(current === cases[0].key, "Next stays in this feature and wraps");
  const caption = document.querySelector("#render .caption");
  check(!caption || !caption.textContent.includes("□"), "No maximize affordance");
  const logo = document.querySelector("header .brand img");
  await logo.decode();
  check(logo.naturalWidth > 0, "Android logo loads");
  check(!document.querySelector('[id="theme"]'), "No theme control");
  check(document.querySelectorAll(".feature-links a").length === 8, "Overview and seven feature links");
  return { feature, states: cases.length, nativeRenders: cases.filter(c => c.image).length,
    passed: checks, scope: "Static feature pages only; no native integration claim." };
})();
