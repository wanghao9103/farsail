// Apply the saved appearance before the WebView paints the startup screen.
(() => {
  const root = document.documentElement;
  if (new URLSearchParams(location.search).has("viewer")) {
    root.dataset.viewer = "true";
    return;
  }
  try {
    if (localStorage.getItem("farsail.appearance") === "midnight") {
      root.dataset.appearance = "midnight";
    }
  } catch {
    // The default workspace remains available when browser storage is disabled.
  }
})();
