// A minimal Chrome DevTools Protocol client for driving Silo's WebView2 window (debug builds
// expose it on a local port when SILO_PROFILE_CDP_PORT is set; see src-tauri/src/profile.rs).
// Uses Node's built-in WebSocket and fetch, so the kit has no dependencies.
//   const page = await connectToApp(9333);
//   await page.eval("document.title");
//   const png = await page.screenshot();

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Wait for the app's page target on the DevTools port.
 * @param {number} port
 * @param {number} [timeoutMs]
 * @returns {Promise<string>} the page's WebSocket debugger URL
 */
export async function waitForPage(port, timeoutMs = 60_000) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = list.find((t) => t.type === "page" && /^(tauri:|https?:\/\/tauri\.localhost)/.test(t.url));
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      /* not up yet */
    }
    await sleep(500);
  }
  throw new Error(`no Silo page on DevTools port ${port} after ${timeoutMs / 1000} s`);
}

/**
 * Connect to the app page.
 * @param {number} port
 */
export async function connectToApp(port) {
  const ws = new WebSocket(await waitForPage(port));
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("couldn't open the DevTools socket"));
  });
  let id = 0;
  const pending = new Map();
  const listeners = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(`${msg.error.message} ${msg.error.data ?? ""}`));
      else resolve(msg.result);
    } else if (msg.method) {
      for (const cb of listeners.get(msg.method) ?? []) cb(msg.params);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const n = ++id;
      pending.set(n, { resolve, reject });
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  const page = {
    send,
    on(method, cb) {
      if (!listeners.has(method)) listeners.set(method, []);
      listeners.get(method).push(cb);
    },
    /** Evaluate an expression in the page (promises awaited) and return its JSON value. */
    async eval(expression) {
      const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) {
        throw new Error(`page error: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}`);
      }
      return r.result.value;
    },
    /** Wait until `expression` is truthy in the page. */
    async waitFor(expression, timeoutMs = 20_000, label = expression) {
      const until = Date.now() + timeoutMs;
      while (Date.now() < until) {
        try {
          if (await page.eval(`!!(${expression})`)) return;
        } catch {
          /* page reloading */
        }
        await sleep(150);
      }
      throw new Error(`timed out waiting for: ${label}`);
    },
    /** A PNG of the page as laid out (no window frame). */
    async screenshot() {
      const r = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false, fromSurface: true });
      return Buffer.from(r.data, "base64");
    },
    close() {
      ws.close();
    },
  };
  await send("Runtime.enable");
  await send("Page.enable");
  return page;
}

export { sleep };
