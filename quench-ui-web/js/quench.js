// quench.js - the browser half of quench-ui-web. The contract it implements is in
// docs/quench-ui-web.md; read that before changing anything here.
//
//   <div id="app"></div>
//   <script type="module" src="/quench.js?wasm=/app.wasm"></script>

const EVENTS = ["click", "dblclick", "input", "change", "keydown", "keyup", "submit", "focus", "blur",
  "pointerdown", "pointerup", "pointermove"];
const LEVELS = ["debug", "info", "warn", "error"];
const FORM_PROPS = new Set(["value", "checked", "selected"]);

/** Loads the module at `url`, wires it to the DOM and calls its `q_main`. */
export async function start(url, options = {}) {
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();
  const dev = options.dev ?? (window.QUENCH_DEV === true
    || ["localhost", "127.0.0.1", "[::1]"].includes(location.hostname));
  let ex = null;       // the instance's exports
  let dead = false;    // the instance trapped
  let busy = 0;        // imports running right now
  const queued = [];   // events raised during an import, delivered after it
  const nodes = [null, document.getElementById("app") ?? document.body]; // handle -> node
  const free = [];     // released handles, reused
  const listeners = new Map(); // handle -> [[type, fn]]
  const timers = new Map();
  const fetches = new Map();

  // --- memory ---------------------------------------------------------------------------------
  // The memory may grow during any call into Rust, so a view is never kept.
  const bytes = () => new Uint8Array(ex.memory.buffer);
  const text = (ptr, len) => (len === 0 ? "" : decoder.decode(bytes().subarray(ptr, ptr + len)));
  const copy = (ptr, len) => (len === 0 ? new Uint8Array(0) : bytes().slice(ptr, ptr + len));
  /** Puts data in a fresh Rust buffer; the export it is passed to takes ownership. */
  const put = (data) => {
    const ptr = ex.q_alloc(data.length);
    if (data.length > 0) bytes().set(data, ptr);
    return [ptr, data.length];
  };

  // --- calls into Rust ------------------------------------------------------------------------
  const call = (fn) => {
    if (dead) return;
    try {
      fn();
    } catch (error) {
      dead = true;
      console.error("quench: the app stopped:", error);
      if (dev) overlay(`The app stopped: ${error}`);
    }
  };
  const deliver = (args) => call(() => ex.q_dispatch_event(...args));
  const dispatch = (...args) => (busy > 0 ? queued.push(args) : deliver(args));
  /** Wraps an import: events raised inside it wait until it returns. */
  const guarded = (fn) => (...args) => {
    busy++;
    try {
      return fn(...args);
    } catch (error) {
      console.warn("quench:", error);
    } finally {
      if (--busy === 0) while (queued.length > 0) deliver(queued.shift());
    }
  };

  function overlay(message) {
    const box = document.createElement("pre");
    box.textContent = message;
    Object.assign(box.style, { position: "fixed", inset: "0", margin: "0", padding: "1rem", zIndex: "2147483647",
      background: "#300", color: "#fcc", font: "14px monospace", whiteSpace: "pre-wrap", overflow: "auto" });
    document.body.append(box);
  }

  // --- handles --------------------------------------------------------------------------------
  const add = (node) => {
    const h = free.pop() ?? nodes.length;
    nodes[h] = node;
    return h;
  };
  const get = (h) => nodes[h] ?? null;

  function release(h) {
    if (h < 2 || !nodes[h]) return;
    for (const [type, fn] of listeners.get(h) ?? []) nodes[h].removeEventListener(type, fn);
    listeners.delete(h);
    nodes[h] = null;
    free.push(h);
  }

  function listen(h, kind) {
    const node = get(h);
    const type = EVENTS[kind];
    if (!node || !type) return;
    const entry = listeners.get(h) ?? [];
    if (entry.some(([t]) => t === type)) return;
    const fn = (event) => {
      let payload = "", a = 0, b = 0, c = 0, flags = 0;
      if (type === "submit") event.preventDefault(); // a form never reloads the page
      else if (type === "input" || type === "change") payload = String(event.currentTarget.value ?? "");
      else if (type === "keydown" || type === "keyup") {
        payload = event.key;
        flags = (event.ctrlKey ? 1 : 0) | (event.shiftKey ? 2 : 0) | (event.altKey ? 4 : 0) | (event.metaKey ? 8 : 0);
      } else if (type.startsWith("pointer")) [a, b, c] = [event.clientX, event.clientY, event.pointerId];
      const [ptr, len] = put(encoder.encode(payload));
      dispatch(h, kind, ptr, len, a, b, c, flags);
    };
    node.addEventListener(type, fn);
    entry.push([type, fn]);
    listeners.set(h, entry);
  }

  // --- DOM ------------------------------------------------------------------------------------
  const setAttr = (h, name, value) => {
    const node = get(h);
    if (node?.nodeType !== 1) return;
    node.setAttribute(name, value);
    if (FORM_PROPS.has(name) && name in node) node[name] = name === "value" ? value : true;
  };
  const removeAttr = (h, name) => {
    const node = get(h);
    if (node?.nodeType !== 1) return;
    node.removeAttribute(name);
    if (FORM_PROPS.has(name) && name in node) node[name] = name === "value" ? "" : false;
  };

  // --- timers and fetch -----------------------------------------------------------------------
  const finish = (id, status, body, headers) => {
    fetches.delete(id);
    const [bp, bl] = put(body);
    const [hp, hl] = put(encoder.encode(headers));
    call(() => ex.q_fetch_done(id, status, bp, bl, hp, hl));
  };

  const env = {
    q_create_el: guarded((p, n) => add(document.createElement(text(p, n)))),
    q_create_text: guarded((p, n) => add(document.createTextNode(text(p, n)))),
    q_create_anchor: guarded(() => add(document.createComment(""))),
    q_set_text: guarded((h, p, n) => { const node = get(h); if (node) node.data = text(p, n); }),
    q_set_attr: guarded((h, np, nl, vp, vl) => setAttr(h, text(np, nl), text(vp, vl))),
    q_remove_attr: guarded((h, p, n) => removeAttr(h, text(p, n))),
    q_insert: guarded((parent, child, before) => get(parent)?.insertBefore(get(child), get(before))),
    q_remove: guarded((parent, child) => {
      const node = get(child);
      if (node && node.parentNode === get(parent)) node.remove();
    }),
    q_listen: guarded(listen),
    q_release: guarded(release),
    q_log: guarded((level, p, n) => {
      const message = text(p, n);
      console[LEVELS[level] ?? "error"](`quench: ${message}`);
      if (level === 3 && dev) overlay(message);
    }),
    q_set_timeout: guarded((id, ms) => {
      timers.set(id, setTimeout(() => { timers.delete(id); call(() => ex.q_timer_fire(id)); }, ms));
    }),
    q_clear_timeout: guarded((id) => { clearTimeout(timers.get(id)); timers.delete(id); }),
    q_fetch: guarded((id, mp, ml, up, ul, hp, hl, bp, bl) => {
      const method = text(mp, ml);
      const init = { method, signal: undefined, headers: {} };
      for (const line of text(hp, hl).split("\n")) {
        const at = line.indexOf(":");
        if (at > 0) init.headers[line.slice(0, at).trim()] = line.slice(at + 1).trim();
      }
      if (bl > 0 && method !== "GET" && method !== "HEAD") init.body = copy(bp, bl);
      const controller = new AbortController();
      init.signal = controller.signal;
      fetches.set(id, controller);
      fetch(text(up, ul), init)
        .then(async (response) => {
          const body = new Uint8Array(await response.arrayBuffer());
          const headers = [...response.headers].map(([k, v]) => `${k}: ${v}`).join("\n");
          finish(id, response.status, body, headers);
        })
        .catch((error) => finish(id, 0, encoder.encode(error?.name === "AbortError" ? "aborted" : String(error)), ""));
    }),
    q_fetch_abort: guarded((id) => fetches.get(id)?.abort()),
    q_schedule_tick: guarded(() => queueMicrotask(() => call(() => ex.q_tick()))),
  };

  // --- boot -----------------------------------------------------------------------------------
  let result;
  try {
    result = await WebAssembly.instantiateStreaming(fetch(url), { env });
  } catch (error) { // a server that does not send application/wasm
    result = await WebAssembly.instantiate(await (await fetch(url)).arrayBuffer(), { env });
  }
  ex = result.instance.exports;
  performance.mark("quench:instantiated");
  call(() => ex.q_main?.());
  performance.mark("quench:ready");
  const api = { exports: ex, handles: () => nodes.filter(Boolean).length - 1 };
  if (dev) window.__quench = api; // for tests and the console
  return api;
}

const auto = new URL(import.meta.url).searchParams.get("wasm");
if (auto) start(auto).catch((error) => console.error("quench: could not start:", error));
