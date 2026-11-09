(() => {
  if (window.__farsailRenderSmokeStarted) return;
  window.__farsailRenderSmokeStarted = true;
  // Isolated native WebView2 workload. No account, server or phone is used.
  setTimeout(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    let postMessageFallback = false;
    const warn = console.warn;
    console.warn = (...args) => {
      if (String(args[0]).includes("IPC custom protocol failed"))
        postMessageFallback = true;
      warn.apply(console, args);
    };
    const started = performance.now();
    const samples = {
      submitted: 0,
      loaded: 0,
      decoded: 0,
      decodeMs: [],
      ipcMs: [],
      responseMs: [],
      raf: 0,
    };
    const decode = HTMLImageElement.prototype.decode;
    if (window.createImageBitmap) {
      const bitmap = window.createImageBitmap.bind(window);
      window.createImageBitmap = async (...args) => {
        const at = performance.now();
        const result = await bitmap(...args);
        samples.decoded++;
        samples.decodeMs.push(performance.now() - at);
        return result;
      };
    }
    HTMLImageElement.prototype.decode = async function () {
      const at = performance.now();
      if (window.__farsailRenderSlow)
        await new Promise((resolve) => setTimeout(resolve, 200));
      await decode.call(this);
      samples.decoded++;
      samples.decodeMs.push(performance.now() - at);
    };
    let last = "";
    const observer = new MutationObserver(() => {
      const surface = document.querySelector(
        ".remote-screen img, .remote-screen canvas",
      );
      const frame =
        surface?.getAttribute("data-frame-sequence") ||
        surface?.getAttribute("src");
      if (frame && frame !== last) {
        last = frame;
        samples.submitted++;
        if (surface.dataset.ipcMs)
          samples.ipcMs.push(Number(surface.dataset.ipcMs));
        if (surface.dataset.responseMs)
          samples.responseMs.push(Number(surface.dataset.responseMs));
      }
    });
    observer.observe(document, {
      subtree: true,
      attributes: true,
      attributeFilter: ["src", "data-frame-sequence"],
    });
    document.addEventListener(
      "load",
      (e) => {
        if (e.target.matches?.(".remote-screen img")) samples.loaded++;
      },
      true,
    );
    let running = true;
    const raf = () => {
      if (!running) return;
      samples.raf++;
      requestAnimationFrame(raf);
    };
    requestAnimationFrame(raf);
    setTimeout(async () => {
      running = false;
      observer.disconnect();
      const seconds = (performance.now() - started) / 1000;
      const sorted = samples.decodeMs.sort((a, b) => a - b);
      const ipc = samples.ipcMs.sort((a, b) => a - b);
      const response = samples.responseMs.sort((a, b) => a - b);
      const surface = document.querySelector(
        ".remote-screen img, .remote-screen canvas",
      );
      await invoke("ipc_render_smoke_report", {
        result: JSON.stringify({
          seconds,
          submitted: samples.submitted,
          submittedFps: samples.submitted / seconds,
          loaded: samples.loaded,
          decoded: samples.decoded,
          raf: samples.raf,
          decodeP95Ms: sorted[Math.floor((sorted.length - 1) * 0.95)] ?? null,
          ipcP50Ms: ipc[Math.floor((ipc.length - 1) * 0.5)] ?? null,
          ipcP95Ms: ipc[Math.floor((ipc.length - 1) * 0.95)] ?? null,
          responseP50Ms:
            response[Math.floor((response.length - 1) * 0.5)] ?? null,
          binaryType: surface?.dataset.binaryType,
          delivery: surface?.dataset.delivery,
          ipcUrl: window.__TAURI_INTERNALS__.convertFileSrc(
            "media_next",
            "ipc",
          ),
          postMessageFallback,
          surface: surface?.tagName,
          decoder: surface?.dataset.decoder,
          width: surface?.naturalWidth || surface?.width,
          height: surface?.naturalHeight || surface?.height,
          visibility: document.visibilityState,
          focused: document.hasFocus(),
        }),
      });
    }, 20000);
  }, 1000);
})();
