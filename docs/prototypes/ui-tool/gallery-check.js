(async () => {
  const initial = state.value;
  for (const option of state.options) {
    state.value = option.value;
    state.dispatchEvent(new Event("change"));
    const image = render.querySelector("img");
    await image.decode();
    if (image.alt !== option.label || new URL(location.href).searchParams.get("state") !== option.value) {
      throw new Error(`Incorrect preview for ${option.value}`);
    }
  }
  show(initial);
  return state.options.length;
})();
