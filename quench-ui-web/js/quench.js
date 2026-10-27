// quench.js - the browser half of quench-ui-web. The contract it implements is in
// docs/quench-ui-web.md; read that before changing anything here.
//
//   <div id="app"></div>
//   <script type="module" src="/quench.js?wasm=/app.wasm"></script>      (add &dev=0 or &dev=1 to force)

const EVENTS = ["click", "dblclick", "input", "change", "keydown", "keyup", "submit", "focus", "blur",
  "pointerdown", "pointerup", "pointermove"];
const LEVELS = ["debug", "info", "warn", "error"];
const FORM_PROPS = new Set(["value", "checked", "selected"]);

/** Loads the module at `url`, wires it to the DOM and calls its `q_main`. */
export async function start(url, options = {}) {
  const decoder = new TextDecoder();
  const encoder = new TextEncoder();
  const dev = options.dev ?? (typeof window.QUENCH_DEV === "boolean" ? window.QUENCH_DEV
    : ["localhost", "127.0.0.1", "[::1]"].includes(location.hostname));
  let ex = null;       // the instance's exports
  let dead = false;    // the instance trapped
  let busy = 0;        // imports running right now
  const queued = [];   // events raised during an import, delivered after it
  const root = document.getElementById("app") ?? document.body;
  const nodes = [null, root]; // handle -> node
  const free = [];     // released handles, reused
  const listeners = new Map(); // handle -> [[type, fn]]
  const timers = new Map();
  const fetches = new Map();
  const sources = new Map(); // id -> EventSource

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

  // --- hydration ------------------------------------------------------------------------------
  // While hydrating, creating a node claims the next node of the server-rendered DOM instead of
  // making one (nodes are created in document order). A node that does not match is replaced by a
  // fresh one, so the result is always what a client-only render would have built.
  let hydrating = null; // { frames: [{ parent, cursor, fresh }], mismatches }
  const origin = new Map(); // handle -> "claimed" | "replace" | "insert", until it is inserted
  const frame = () => hydrating.frames[hydrating.frames.length - 1];
  const where = (n) => {
    const path = [];
    for (; n?.nodeType === 1; n = n.parentElement) {
      path.unshift(n.id ? `#${n.id}` : `${n.localName}:nth-child(${[...(n.parentElement?.children ?? [])].indexOf(n) + 1})`);
      if (n.id) break;
    }
    return path.join(" > ");
  };
  const what = (n) => (!n ? "nothing" : n.nodeType === 1 ? `<${n.localName}>` : n.nodeType === 3 ? `text "${n.data.slice(0, 20)}"` : "a comment");
  const bad = (expected, found, at) => {
    hydrating.mismatches++;
    if (dev) console.error(`quench: hydration mismatch in ${where(at)}: expected ${expected}, found ${what(found)}; rebuilt it`);
  };
  const trim = (f) => { // server nodes the app has no use for
    for (let c = f.cursor; c && !f.fresh; c = f.cursor) { bad("the end of its content", c, f.parent); f.cursor = c.nextSibling; c.remove(); }
  };
  /** The next server node if it is the one wanted, else a fresh one that will replace it. */
  function take(wanted, fresh, expected) {
    const f = frame();
    if (f.fresh) return add(fresh()); // inside a rebuilt subtree there is nothing to claim
    const c = f.cursor;
    const claimed = c && wanted(c);
    if (claimed) f.cursor = c.nextSibling; else bad(expected, c, f.parent);
    const h = add(claimed ? c : fresh());
    origin.set(h, claimed ? "claimed" : "replace");
    return h;
  }
  function createElement(tag) {
    if (!hydrating) return add(document.createElement(tag));
    const h = take((c) => c.nodeType === 1 && c.localName === tag, () => document.createElement(tag), `<${tag}>`);
    const claimed = origin.get(h) === "claimed";
    hydrating.frames.push({ parent: nodes[h], cursor: claimed ? nodes[h].firstChild : null, fresh: !claimed });
    return h;
  }
  function createText(s) {
    if (!hydrating) return add(document.createTextNode(s));
    const f = frame(), c = f.cursor;
    if (!f.fresh && s === "") { // an empty text node has no markup to claim
      const h = add(document.createTextNode(""));
      origin.set(h, "insert");
      return h;
    }
    if (!f.fresh && c?.nodeType === 3 && c.data.length > s.length && c.data.startsWith(s)) c.splitText(s.length); // the parser merged neighbours
    return take((n) => n.nodeType === 3 && n.data === s, () => document.createTextNode(s), `text "${s.slice(0, 20)}"`);
  }
  function insert(parent, child, before) {
    const node = get(child);
    if (!hydrating) return get(parent)?.insertBefore(node, get(before));
    if (node.nodeType === 1 && frame().parent === node) trim(hydrating.frames.pop());
    const mode = origin.get(child);
    origin.delete(child);
    if (mode === "claimed") return;
    const f = frame();
    if (f.fresh || !mode) return get(parent)?.insertBefore(node, get(before));
    f.parent.insertBefore(node, f.cursor);
    if (mode === "replace" && f.cursor) { const old = f.cursor; f.cursor = old.nextSibling; old.remove(); }
  }

  // --- DOM ------------------------------------------------------------------------------------
  const setAttr = (h, name, value) => {
    const node = get(h);
    if (node?.nodeType !== 1) return;
    if (hydrating && node.getAttribute(name) === value) return; // the server got it right
    if (hydrating && origin.get(h) === "claimed") bad(`${name}="${value}"`, node, node);
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
    call(() => ex.q_fetch_done(id, status, ...put(body), ...put(encoder.encode(headers))));
  };

  /** Tells Rust what an event source did: 0 opened, 1 an event, 2 reconnecting, 3 failed for good. */
  const sse = (id, kind, name = "", data = "") => {
    if (!sources.has(id)) return;
    call(() => ex.q_sse_event(id, kind, ...put(encoder.encode(name)), ...put(encoder.encode(data))));
  };

  const env = {
    q_create_el: guarded((p, n) => createElement(text(p, n))),
    q_create_text: guarded((p, n) => createText(text(p, n))),
    q_create_anchor: guarded(() => (hydrating
      ? take((c) => c.nodeType === 8 && c.data === "", () => document.createComment(""), "an anchor comment")
      : add(document.createComment("")))),
    q_set_text: guarded((h, p, n) => { const node = get(h); if (node) node.data = text(p, n); }),
    q_set_attr: guarded((h, np, nl, vp, vl) => setAttr(h, text(np, nl), text(vp, vl))),
    q_remove_attr: guarded((h, p, n) => removeAttr(h, text(p, n))),
    q_insert: guarded(insert),
    q_root_has_content: guarded(() => (root.firstChild ? 1 : 0)),
    q_hydrate_begin: guarded(() => { hydrating = { frames: [{ parent: root, cursor: root.firstChild, fresh: false }], mismatches: 0 }; }),
    q_hydrate_end: guarded(() => {
      trim(hydrating.frames[0]);
      const { mismatches } = hydrating;
      hydrating = null;
      origin.clear();
      return mismatches;
    }),
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
    q_sse_open: guarded((id, up, ul, np, nl) => {
      const source = new EventSource(text(up, ul));
      sources.set(id, source);
      for (const name of new Set(["message", ...text(np, nl).split("\n").filter(Boolean)])) {
        source.addEventListener(name, (event) => sse(id, 1, name, event.data));
      }
      source.onopen = () => sse(id, 0);
      source.onerror = () => {
        sse(id, source.readyState === EventSource.CLOSED ? 3 : 2);
        if (source.readyState === EventSource.CLOSED) sources.delete(id);
      };
    }),
    q_sse_close: guarded((id) => { sources.get(id)?.close(); sources.delete(id); }),
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
  const state = document.getElementById("__q_state"); // JSON the server embedded for the app
  if (state) call(() => ex.q_set_state(...put(encoder.encode(state.textContent))));
  call(() => ex.q_main?.());
  performance.mark("quench:ready");
  const api = { exports: ex, handles: () => nodes.filter(Boolean).length - 1 };
  if (dev) window.__quench = api; // for tests and the console
  return api;
}

const params = new URL(import.meta.url).searchParams;
const auto = params.get("wasm");
if (auto) start(auto, { dev: params.has("dev") ? params.get("dev") === "1" : undefined }).catch((error) => console.error("quench: could not start:", error));
