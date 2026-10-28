// End-to-end checks in a real browser: drives headless Chromium over the DevTools protocol.
// Run through scripts/ui-web-test.fish, which builds the wasm and starts the example server.
//   node quench-ui-web/tests/browser.mjs http://127.0.0.1:3101
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const base = (process.argv[2] ?? "http://127.0.0.1:3101").replace(/\/$/, "");
const port = 9400 + Math.floor(Math.random() * 400);
const profile = mkdtempSync(join(tmpdir(), "quench-chrome-"));
const chrome = spawn(process.env.CHROMIUM ?? "chromium", [
  "--headless=new", "--no-sandbox", "--disable-gpu", `--remote-debugging-port=${port}`,
  `--user-data-dir=${profile}`, "about:blank",
], { stdio: "ignore" });

let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${ok ? "" : `  ${detail}`}`);
  if (!ok) failures++;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, what, ms = 8000) => {
  for (let t = 0; t < ms; t += 50) { if (await fn()) return true; await sleep(50); }
  check(`waited for ${what}`, false, "timed out");
  return false;
};

async function connect() {
  for (let i = 0; i < 100; i++) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = pages.find((p) => p.type === "page");
      if (page) return new WebSocket(page.webSocketDebuggerUrl);
    } catch { /* chromium is still starting */ }
    await sleep(100);
  }
  throw new Error("could not reach chromium");
}

const ws = await connect();
await new Promise((r) => (ws.onopen = r));
let id = 0;
const waiting = new Map();
const errors = [];      // console errors, exceptions and failed log entries since the last reset
const requests = [];    // URLs the page asked for since the last reset
ws.onmessage = ({ data }) => {
  const m = JSON.parse(data);
  if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
  else if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") {
    errors.push(m.params.args.map((a) => a.value ?? a.description).join(" "));
  } else if (m.method === "Runtime.exceptionThrown") errors.push(m.params.exceptionDetails.text);
  else if (m.method === "Log.entryAdded" && m.params.entry.level === "error") errors.push(m.params.entry.text);
  else if (m.method === "Network.requestWillBeSent") requests.push(m.params.request.url);
};
const send = (method, params = {}) => new Promise((resolve) => {
  const mid = ++id;
  waiting.set(mid, resolve);
  ws.send(JSON.stringify({ id: mid, method, params }));
});
const ev = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description ?? "evaluate failed");
  return r.result.result.value;
};
const $ = (sel) => `document.querySelector(${JSON.stringify(sel)})`;
const hydrated = () => ev(`performance.getEntriesByName("quench:ready").length > 0`); // set in dev and production alike
const go = async (path) => {
  errors.length = 0;
  requests.length = 0;
  await send("Page.navigate", { url: `${base}${path}` });
};
const load = async (path) => {
  await go(path);
  return until(hydrated, `${path} to start`);
};
const setDelay = (ms) => fetch(`${base}/__delay?ms=${ms}`).then((r) => r.text());
const appHtml = () => ev(`document.getElementById("app").innerHTML`);

try {
  await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable"); await send("Network.enable");

  // ---- the server's side: what view-source shows, and how the assets are served ----------------
  const page = await fetch(`${base}/`);
  const source = await page.text();
  check("view-source already has the content", source.includes("<h1>quench-ui</h1>") && source.includes('<span id="count">0</span>')
    && source.includes("3 from the server") && source.includes("<li>build the wasm</li>"));
  check("the state is embedded as JSON", /<script type="application\/json" id="__q_state" nonce="[0-9a-f]{32}">\{"Demo":\{"server_todos":\[/.test(source));
  const nonce = /nonce="([0-9a-f]{32})"/.exec(source)[1];
  const csp = page.headers.get("content-security-policy");
  check("the policy names the page's nonce and allows no inline code", csp.includes(`'nonce-${nonce}'`)
    && csp.includes("default-src 'self'") && !csp.includes("'unsafe-inline'"));
  check("no inline handlers or styles in the markup", !/\son[a-z]+=/.test(source) && !/<style|\sstyle=/.test(source));
  const wasmPath = /wasm=([^"]+)"/.exec(source)[1];
  const scriptPath = /src="([^?"]+)\?wasm=/.exec(source)[1];
  check("the assets have content-hashed names", /^\/_quench\/app\.[0-9a-f]{16}\.wasm$/.test(wasmPath) && /^\/_quench\/quench\.[0-9a-f]{16}\.js$/.test(scriptPath));
  const asset = await fetch(`${base}${wasmPath}`, { headers: { "accept-encoding": "gzip" } });
  check("an asset is cacheable for a year and immutable", asset.headers.get("cache-control") === "public, max-age=31536000, immutable"
    && /^"[0-9a-f]{16}"$/.test(asset.headers.get("etag")) && asset.headers.get("content-type") === "application/wasm");
  const again = await fetch(`${base}${wasmPath}`, { headers: { "if-none-match": asset.headers.get("etag") } });
  check("a repeat request is a 304", again.status === 304);
  const gz = await fetch(`${base}${scriptPath}`, { headers: { "accept-encoding": "gzip" } });
  const plain = await fetch(`${base}${scriptPath}`, { headers: { "accept-encoding": "identity" } });
  check("gzip is sent when accepted and not otherwise", gz.headers.get("content-encoding") === "gzip" && plain.headers.get("content-encoding") === null);
  check("an old hash is a 404", (await fetch(`${base}/_quench/app.0000000000000000.wasm`)).status === 404);

  // ---- hydration keeps the nodes the server sent -----------------------------------------------
  await setDelay(700); // the module arrives late, so the page can be inspected before it starts
  await go("/");
  await until(() => ev(`!!${$("#count")}`), "the server-rendered page");
  check("the page is visible before the app starts", !(await hydrated()) && await ev(`${$("h1")}?.textContent`) === "quench-ui");
  await ev(`window.__ssr = {
    h1: ${$("h1")}, countText: ${$("#count")}.firstChild, form: ${$("#add-form")}, input: ${$("#draft")},
    items: [...document.querySelectorAll("#server-list li")], anchor: ${$("#server-list")}.lastChild,
    total: document.getElementById("app").querySelectorAll("*").length }`);
  const before = await appHtml();
  await until(hydrated, "hydration", 12000);
  await setDelay(0);
  check("hydration kept every server node", await ev(`window.__ssr.h1 === ${$("h1")}
    && window.__ssr.countText === ${$("#count")}.firstChild && window.__ssr.form === ${$("#add-form")}
    && window.__ssr.input === ${$("#draft")} && window.__ssr.anchor === ${$("#server-list")}.lastChild
    && window.__ssr.items.every((li, i) => li === document.querySelectorAll("#server-list li")[i])`));
  check("it added no nodes", await ev(`document.getElementById("app").querySelectorAll("*").length === window.__ssr.total`));
  check("the markup is unchanged by hydration", await appHtml() === before);
  check("no errors, no mismatches", errors.length === 0, JSON.stringify(errors));
  check("the embedded state meant no request for it", !requests.some((u) => u.endsWith("/api/todos")), JSON.stringify(requests));
  await ev(`${$("#up")}.click()`);
  check("the hydrated counter works on the server's text node", await ev(`window.__ssr.countText.data`) === "1"
    && await ev(`window.__ssr.countText === ${$("#count")}.firstChild`));
  const cleanDom = (await appHtml()).replace(/id="count">1</, 'id="count">0<'); // the page as the server sent it, before the click

  // ---- interactions after hydration ------------------------------------------------------------
  await load("/");
  await ev(`window.__countNode = ${$("#count")}.firstChild`);
  for (let i = 0; i < 3; i++) await ev(`${$("#up")}.click()`);
  await ev(`${$("#down")}.click()`);
  check("counter counts", await ev(`${$("#count")}.textContent`) === "2");
  check("counter patched the same text node", await ev(`window.__countNode === ${$("#count")}.firstChild`));

  const type = (text) => ev(`(() => { const i = ${$("#draft")}; i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input", {bubbles: true})); })()`);
  await ev(`window.__marker = 42`);
  await type("milk");
  await ev(`${$("#add-form")}.requestSubmit()`);
  check("submit added an item", await ev(`${$("#left")}.textContent`) === "1 left");
  check("submit did not reload the page", await ev(`window.__marker`) === 42);
  check("the input was cleared (value property)", await ev(`${$("#draft")}.value`) === "");
  await ev(`window.__first = document.querySelector("ul li")`);
  await type("eggs");
  await ev(`${$("#draft")}.dispatchEvent(new KeyboardEvent("keydown", {key: "Enter", bubbles: true}))`);
  await type("bread");
  await ev(`${$("#add-form")}.requestSubmit()`);
  const titles = () => ev(`[...document.querySelectorAll("ul li:not(#server-list li) span")].map((s) => s.textContent).join(",")`);
  check("three items, in order", await titles() === "milk,eggs,bread", await titles());
  check("adding kept the first item's node", await ev(`window.__first === document.querySelector("ul li")`));
  await ev(`document.querySelectorAll("button.toggle")[1].click()`);
  check("toggle marks the item done", await ev(`document.querySelectorAll("ul li")[1].classList.contains("done")`)
    && await ev(`${$("#left")}.textContent`) === "2 left");
  await ev(`document.querySelectorAll("button.remove")[0].click()`);
  check("remove drops the item and keeps the others' nodes", await titles() === "eggs,bread"
    && await ev(`window.__first.isConnected`) === false);

  const handles = () => ev(`window.__quench.handles()`);
  const baseline = await handles();
  for (let i = 0; i < 100; i++) { await type(`item ${i}`); await ev(`${$("#add-form")}.requestSubmit()`); }
  const during = await handles();
  while (await ev(`document.querySelectorAll("button.remove").length`) > 2) await ev(`document.querySelector("button.remove").click()`);
  const after = await handles();
  check("100 items added and removed: the handle table returns to its size", during > baseline + 300 && after === baseline, `${baseline} -> ${during} -> ${after}`);

  requests.length = 0;
  await ev(`${$("#server-status")}.firstChild.data = "stale"`); // not textContent: that would replace the node Rust patches
  await ev(`${$("#reload")}.click()`);
  check("reload fetched again", await until(() => ev(`${$("#server-status")}.textContent`).then((t) => t !== "stale" && t.includes("from the server")), "the reload"));
  check("and that was a request to the server", requests.some((u) => u.endsWith("/api/todos")));
  check("no console errors or exceptions so far", errors.length === 0, JSON.stringify(errors));


  // ---- server functions, sessions and the notes app (Stage 6) ---------------------------------
  const rpc = { "content-type": "application/json", "x-quench-rpc": "1" };
  const call = (path, body, headers = {}) => fetch(`${base}${path}`, { method: "POST", headers: { ...rpc, ...headers }, body: JSON.stringify(body) });
  check("a call without the custom header is refused", (await fetch(`${base}/_fn/list_notes`, { method: "POST", headers: { "content-type": "application/json" }, body: "{}" })).status === 403);
  check("a call without a session is 401 with a typed error", await call("/_fn/list_notes", {}).then(async (r) => r.status === 401 && (await r.json()).kind === "unauthorized"));
  check("a cross-site call is refused", (await call("/_fn/list_notes", {}, { "sec-fetch-site": "cross-site", origin: "https://evil.example" })).status === 403);
  check("a GET is 405", (await fetch(`${base}/_fn/list_notes`)).status === 405);
  const login = await (await call("/api/v1/auth/login", { username: "ola", password: "pw" })).json();
  const bearer = { authorization: `Bearer ${login.access_token}` };
  check("a bearer token is a session without any cookie", await call("/_fn/list_notes", {}, bearer).then((r) => r.status === 200));
  const refreshed = await call("/api/v1/auth/refresh", { refresh_token: login.refresh_token });
  check("the refresh token rotates", refreshed.status === 200 && (await refreshed.json()).refresh_token !== login.refresh_token);
  check("a spent refresh token is refused", (await call("/api/v1/auth/refresh", { refresh_token: login.refresh_token })).status === 401);
  check("a wrong password is refused", (await call("/api/v1/auth/login", { username: "ola", password: "x" })).status === 401);

  const notesSource = await (await fetch(`${base}/notes`)).text();
  check("signed out, the server renders the sign-in form", notesSource.includes('id="login-form"') && !notesSource.includes('id="note-form"')
    && notesSource.includes('{"Notes":{"user":null,"notes":[]}}'));

  const typeInto = (sel, text) => ev(`(() => { const i = ${$(sel)}; i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input", {bubbles: true})); })()`);
  const submit = (sel) => ev(`${$(sel)}.requestSubmit()`);
  const message = () => ev(`${$("#message")}.textContent`);
  await load("/notes");
  check("the page starts with the sign-in form", await ev(`!!${$("#login-form")}`) && !(await ev(`!!${$("#note-form")}`)));
  await typeInto("#username", "ola"); await typeInto("#password", "wrong"); await submit("#login-form");
  check("a wrong password shows the server's reason", await until(async () => (await message()) === "invalid credentials", "the message"));
  await typeInto("#password", "pw"); await submit("#login-form");
  check("signing in shows the user's notes", await until(() => ev(`${$("#who")}?.textContent === "Signed in as ola"`), "the signed-in view"));
  check("the session is a cookie the page cannot read", !(await ev(`document.cookie`)).includes("session") && !(await ev(`document.cookie`)).includes("refresh"));
  await submit("#note-form");
  const requestsBefore = requests.length;
  check("an empty note is refused on the field, without a request", await until(() => ev(`${$("#text-error")}.textContent === "write something"`), "the field error")
    && await ev(`${$("#text")}.getAttribute("aria-invalid")`) === "true" && await ev(`${$("#text")}.getAttribute("aria-describedby")`) === "text-error"
    && !requests.slice(requestsBefore).some((u) => u.includes("/_fn/create_note")));
  await typeInto("#text", "x".repeat(201)); await submit("#note-form");
  check("the server's verdict shows on the same field", await until(() => ev(`${$("#text-error")}.textContent === "too long"`), "the server's message"));
  await typeInto("#text", "buy milk");
  check("typing clears the error", await ev(`${$("#text-error")}.textContent`) === "");
  await submit("#note-form");
  await until(() => ev(`document.querySelectorAll("#notes li").length === 1`), "the first note");
  await typeInto("#text", "walk the dog"); await submit("#note-form");
  check("notes are added and the field is cleared", await until(() => ev(`document.querySelectorAll("#notes li").length === 2`), "two notes")
    && await ev(`${$("#text")}.value`) === "" && await message() === "");
  await ev(`document.querySelector("#notes li button.toggle").click()`);
  check("a note can be marked done", await until(() => ev(`document.querySelector("#notes li").classList.contains("done")
    && document.querySelector("#notes li button.toggle").textContent === "reopen"`), "the toggle"));

  // The server now renders the signed-in view itself, from the cookie.
  await setDelay(700);
  await send("Network.setCacheDisabled", { cacheDisabled: true }); // the module is immutable-cached by now, which would skip the delay
  await go("/notes");
  await until(() => ev(`!!${$("#who")}`), "the server-rendered page");
  const served = { hydrated: await hydrated(), who: await ev(`${$("#who")}?.textContent`), items: await ev(`document.querySelectorAll("#notes li").length`),
    done: await ev(`document.querySelector("#notes li")?.classList.contains("done")`) };
  check("after a reload the server's page already has the notes", !served.hydrated && served.who === "Signed in as ola" && served.items === 2 && served.done === true, JSON.stringify(served));
  await ev(`window.__ssr = { ul: ${$("#notes")}, first: document.querySelector("#notes li"), who: ${$("#who")}.firstChild }`);
  await until(hydrated, "hydration of /notes", 12000);
  await setDelay(0);
  await send("Network.setCacheDisabled", { cacheDisabled: false });
  check("hydration kept the server's nodes", await ev(`window.__ssr.ul === ${$("#notes")} && window.__ssr.first === document.querySelector("#notes li")
    && window.__ssr.who === ${$("#who")}.firstChild`));
  check("and nothing was reported", errors.length === 0, JSON.stringify(errors));

  // Server-sent events: the page subscribes from Rust, and a change made anywhere shows up in it.
  await typeInto("#text", "streamed"); await submit("#note-form");
  check("the page's own change arrives over its stream", await until(() => ev(`[...document.querySelectorAll("#activity li")].some((li) => li.textContent.startsWith("ola added note"))`), "the activity line"));
  const cookieLogin = await call("/api/v1/auth/login", { username: "ola", password: "pw" });
  const elsewhere = { authorization: `Bearer ${(await cookieLogin.json()).access_token}` };
  const lines = await ev(`document.querySelectorAll("#activity li").length`);
  await call("/_fn/create_note", { text: "from another device" }, elsewhere);
  check("and so does a change made from another session", await until(() => ev(`document.querySelectorAll("#activity li").length > ${lines}`), "the other device's line"));
  check("the stream is only for signed-in users", (await fetch(`${base}/api/notes/stream`)).status === 401);

  // Another user sees none of it.
  await ev(`${$("#signout")}.click()`);
  check("signing out shows the form again", await until(() => ev(`!!${$("#login-form")}`), "the sign-in form"));
  check("and ends the session on the server", (await ev(`fetch("/_fn/list_notes", {method: "POST", headers: ${JSON.stringify(rpc)}, body: "{}"}).then((r) => r.status)`)) === 401);
  await typeInto("#username", "ewa"); await typeInto("#password", "pw2"); await submit("#login-form");
  await until(() => ev(`${$("#who")}?.textContent === "Signed in as ewa"`), "ewa's view");
  check("another user does not see them", await ev(`document.querySelectorAll("#notes li").length`) === 0);
  const theirs = await call("/_fn/set_done", { id: 1, done: true }, { authorization: `Bearer ${(await (await call("/api/v1/auth/login", { username: "ewa", password: "pw2" })).json()).access_token}` });
  check("or change them: someone else's note is not found", theirs.status === 404 && (await theirs.json()).kind === "not_found");
  check("no console errors in the notes flow", errors.filter((e) => !e.includes("401") && !e.includes("Failed to load resource")).length === 0, JSON.stringify(errors));

  // ---- routing: deep links, nested layouts, history, guards, scroll (Stage 7) -----------------
  const here = () => ev(`location.pathname + location.search`);
  const pathIs = (want) => until(async () => (await here()) === want, `the URL to be ${want}`);
  const deep = await fetch(`${base}/app/users/7`);
  check("a deep link is rendered by the server", deep.status === 200 && (await deep.text()).includes("User 7"));
  check("a URL no route matches is a 404 from the server", (await fetch(`${base}/app/zzz`)).status === 404);
  const guarded = await fetch(`${base}/app/admin`, { redirect: "manual" });
  check("a guarded page redirects on the server", guarded.status === 302 && guarded.headers.get("location") === "/app/login?next=/app/admin");

  await load("/app/users/7");
  check("the deep link hydrated without a mismatch", errors.filter((e) => e.includes("mismatch")).length === 0, JSON.stringify(errors));
  check("it shows the user from the URL", await ev(`${$("#user-name")}.textContent`) === "User 7");
  await ev(`window.__shell = ${$("#shell")}; ${$("#bump")}.click(); ${$("#bump")}.click()`);
  check("the page works after hydration", await ev(`${$("#visits")}.textContent`) === "Clicks here: 2");
  await ev(`${$("#all-users")}.click()`);
  check("a link changes the URL without a page load", await here() === "/app/users" && await ev(`window.__shell === ${$("#shell")}`));
  check("the layout around it was kept", await ev(`${$("#where")}.textContent`) === "/app/users");

  await ev(`window.scrollTo(0, 700)`);
  await until(() => ev(`scrollY > 600`), "the page to scroll");
  await sleep(250); // the browser reports the end of the scroll after a moment
  await ev(`document.querySelectorAll("#user-list a")[49].click()`);
  check("a link to another page is followed", await here() === "/app/users/50" && await ev(`${$("#user-name")}.textContent`) === "User 50");
  check("a new page starts at the top", await ev(`scrollY`) === 0);
  await ev(`history.back()`);
  await pathIs("/app/users");
  check("back shows the list again", await ev(`document.querySelectorAll("#user-list li").length`) === 60);
  check("back restores the scroll position", await until(() => ev(`Math.abs(scrollY - 700) < 5`), "the scroll to be restored"));
  await ev(`history.forward()`);
  await pathIs("/app/users/50");
  check("forward goes on to the page it left", await ev(`${$("#user-name")}.textContent`) === "User 50");

  const modified = await ev(`(() => {
    const a = document.querySelector("nav a[href='/app']");
    let prevented = null;
    const stop = (e) => { prevented = e.defaultPrevented; e.preventDefault(); };
    document.addEventListener("click", stop, { once: true });
    a.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
    return prevented;
  })()`);
  check("a ctrl-click is left to the browser", modified === false && await here() === "/app/users/50");

  await ev(`document.querySelector("nav a[href='/app/admin']").click()`);
  await pathIs("/app/login?next=/app/admin");
  check("a guarded page sends a signed-out visitor to sign in", await ev(`${$("#sign-in")} !== null`));
  await ev(`${$("#sign-in")}.click()`);
  await pathIs("/app/admin");
  check("after signing in the guarded page is shown", (await ev(`${$("#view")}.textContent`)).includes("Only for the signed in"));
  await ev(`history.back()`);
  await pathIs("/app/users/50");
  check("the redirect and sign-in left no history entry of their own", await here() === "/app/users/50");

  await ev(`document.querySelector("nav a[href='/app/settings/profile']").click()`);
  await pathIs("/app/settings/profile");
  await ev(`document.querySelector("#view a[href='/app/settings/about']").click()`);
  await pathIs("/app/settings/about");
  check("a nested layout shows its child", (await ev(`${$("#view")}.textContent`)).includes("About this app."));
  check("the current link is marked", await ev(`document.querySelectorAll("a[aria-current='page']").length`) >= 1);
  check("the outer layout survived all of it", await ev(`window.__shell === ${$("#shell")}`));

  // ---- a virtualised list of 10,000 rows ---------------------------------------------------------
  await load("/app/big");
  const rowsNow = () => ev(`document.querySelectorAll("#big-list .q-vlist-row").length`);
  const firstRow = () => ev(`document.querySelector("#big-list .q-vlist-row")?.textContent`);
  check("the list hydrated", errors.filter((e) => e.includes("mismatch")).length === 0, JSON.stringify(errors));
  check("only the rows in view are in the page", await until(async () => (await rowsNow()) > 5 && (await rowsNow()) < 40, "the rows") && await firstRow() === "Row 1");
  check("the spacer is as tall as all 10,000 rows (set through the style object)", await ev(`getComputedStyle(document.querySelector("#big-list .q-vlist-space")).height`) === "320000px");
  await ev(`document.getElementById("big-list").scrollTop = 160000`);
  check("scrolling far shows the rows there", await until(async () => /^Row 4\d{3}$|^Row 5\d{3}$/.test(await firstRow()), "rows near 5000"));
  const flick = await ev(`(async () => {
    const list = document.getElementById("big-list");
    const t0 = performance.now();
    for (let i = 0; i < 120; i++) { list.scrollTop = 1000 + i * 2500; await new Promise((r) => requestAnimationFrame(r)); }
    return { ms: performance.now() - t0, nodes: list.querySelectorAll("*").length };
  })()`);
  check("120 scroll steps stay smooth (under 40 ms each on average)", flick.ms / 120 < 40, JSON.stringify(flick));
  check("and the page stays small", flick.nodes < 150, JSON.stringify(flick));
  await ev(`document.getElementById("big-list").scrollTop = 1e9`);
  check("the end of the list is reachable", await until(async () => (await ev(`document.querySelector("#big-list .q-vlist-row:last-child")?.textContent`)) === "Row 10000", "the last row"));
  check("no errors in the list", errors.length === 0, JSON.stringify(errors));

  // ---- the component gallery: keyboard, ARIA, forms, gestures, motion, language (Stage 7) ------
  const press = async (key, code, vk) => {
    for (const type of ["keyDown", "keyUp"]) await send("Input.dispatchKeyEvent", { type, key, code, windowsVirtualKeyCode: vk });
  };
  const shot = async (name) => {
    const { result } = await send("Page.captureScreenshot", { format: "png" });
    if (process.env.SHOTS) (await import("node:fs")).writeFileSync(`${process.env.SHOTS}/${name}.png`, Buffer.from(result.data, "base64"));
  };
  const view = (width, height, scheme, mobile) => Promise.all([
    send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile }),
    send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] }),
  ]);
  await load("/gallery");
  check("the gallery hydrated without a mismatch", errors.filter((e) => e.includes("mismatch")).length === 0, JSON.stringify(errors));
  check("it is the English page", await ev(`${$("#gallery-title")}.textContent`) === "Gallery");
  const tab = (i) => ev(`document.querySelectorAll("[role=tab]")[${i}].click()`);
  check("the tab bar has the right roles", await ev(`document.querySelectorAll("[role=tablist] [role=tab]").length`) === 6
    && await ev(`document.querySelector("[role=tabpanel]").getAttribute("aria-labelledby")`) === "g-tab-0");

  await ev(`${$("#notifications")}.click()`);
  check("a switch toggles and says so", await ev(`${$("#notifications")}.getAttribute("aria-checked")`) === "false");
  await ev(`${$("#unread")}.click()`);
  check("a chip is pressed", await ev(`${$("#unread")}.getAttribute("aria-pressed")`) === "true");

  await ev(`${$("#open-sheet")}.click()`);
  check("a sheet is a modal dialog", await until(() => ev(`!!document.querySelector("[role=dialog][aria-modal=true]")`), "the sheet"));
  await sleep(100);
  check("it takes focus", await ev(`document.querySelector("[role=dialog]").contains(document.activeElement)`));
  let escaped = false;
  for (let i = 0; i < 12; i++) { await press("Tab", "Tab", 9); if (!(await ev(`document.querySelector("[role=dialog]").contains(document.activeElement)`))) escaped = true; }
  check("Tab never leaves it", !escaped);
  await press("Escape", "Escape", 27);
  check("Escape closes it", await until(() => ev(`!document.querySelector("[role=dialog]")`), "the sheet to leave"));

  await ev(`${$("#gallery-form")}.requestSubmit()`);
  check("an invalid form shows every error", await ev(`document.querySelectorAll("#gallery-form [role=alert]").length`) >= 3
    && (await ev(`${$("#email-error")}.textContent`)) === "Enter your email" && await ev(`${$("#email")}.getAttribute("aria-invalid")`) === "true");
  await ev(`(() => { for (const [id, v] of [["email", "ola@example.com"], ["day", "2025-03-04"], ["at", "09:30"]]) { const i = document.getElementById(id); i.value = v; i.dispatchEvent(new Event("input", {bubbles: true})); } })()`);
  await ev(`${$("#gallery-form")}.requestSubmit()`);
  check("a valid form is sent and announced", await until(() => ev(`document.querySelector("[role=status]")?.textContent.includes("Form sent")`), "the snackbar"));

  await ev(`document.querySelector("[role=tab]").focus()`);
  await press("ArrowRight", "ArrowRight", 39);
  check("the arrow key moves to the next tab and focuses it", await ev(`document.activeElement.getAttribute("aria-selected") === "true" && document.activeElement.id === "g-tab-1"`));
  check("and shows its panel", (await ev(`${$("#rows")}.getBoundingClientRect().height`)) > 100);
  await press("End", "End", 35);
  check("End goes to the last tab", await ev(`document.activeElement.id`) === "g-tab-5");
  check("and the page did not scroll", await ev(`scrollY`) === 0);

  await press("Home", "Home", 36);
  await tab(2);
  const pad = (type, x, y, t = 0) => ev(`${$("#pad")}.dispatchEvent(new PointerEvent(${JSON.stringify(type)}, {clientX: ${x}, clientY: ${y}, pointerId: 1, bubbles: true}))`);
  await pad("pointerdown", 100, 100); await pad("pointerup", 100, 100);
  check("a tap is a tap", await ev(`${$("#pad")}.textContent`) === "Tap");
  await pad("pointerdown", 100, 100); await sleep(650);
  check("a long press is recognised by the clock", await ev(`${$("#pad")}.textContent`) === "LongPress");
  await pad("pointerup", 100, 100);
  await pad("pointerdown", 100, 100); await pad("pointermove", 160, 102); await pad("pointermove", 260, 104); await pad("pointerup", 260, 104);
  check("a quick drag is a swipe", await ev(`${$("#pad")}.textContent`) === "Swipe(Right)", await ev(`${$("#pad")}.textContent`));

  await tab(3);
  await ev(`${$("#toggle-card")}.click()`);
  check("animated content appears", await until(() => ev(`!!document.querySelector(".gallery-card")`), "the card"));
  await sleep(100);
  check("and enters", await ev(`document.querySelector(".q-anim").classList.contains("q-anim-in")`));
  await ev(`${$("#toggle-card")}.click()`);
  check("and leaves after its exit", await until(() => ev(`!document.querySelector(".gallery-card")`), "the card to go", 3000));

  await tab(4);
  const files = () => ev(`${$("#files")}.textContent`);
  check("English: one file", await files() === "You have selected 1 file.", await files());
  await ev(`${$("#lang-pl")}.click()`);
  check("the language switches without a reload", await files() === "Wybrano: 1 plik." && await ev(`${$("#gallery-title")}.textContent`) === "Galeria", await files());
  const more = async (n) => { for (let i = 0; i < n; i++) await ev(`${$("#more")}.click()`); };
  const seen = [await files()];
  for (const _ of [2, 3, 4, 5]) { await more(1); seen.push(await files()); }
  check("Polish plural forms: 1, 2-4, 5+", seen.join("|") === "Wybrano: 1 plik.|Wybrano: 2 pliki.|Wybrano: 3 pliki.|Wybrano: 4 pliki.|Wybrano: 5 plików.", seen.join("|"));
  await more(7);
  check("12 is not 'few'", await files() === "Wybrano: 12 plików.", await files());
  await more(10);
  check("22 is 'few' again", await files() === "Wybrano: 22 pliki.", await files());
  await ev(`${$("#lang-en")}.click()`);
  check("and back to English: 22 files", await files() === "You have selected 22 files.", await files());
  check("no errors in the gallery", errors.filter((e) => !e.includes("Failed to load resource")).length === 0, JSON.stringify(errors));

  for (const [name, w, h, scheme, mobile] of [["desktop-dark", 1280, 800, "dark", false], ["desktop-light", 1280, 800, "light", false], ["mobile-dark", 390, 844, "dark", true], ["mobile-light", 390, 844, "light", true]]) {
    await view(w, h, scheme, mobile);
    for (const [label, index] of [["controls", 0], ["lists", 1], ["language", 4]]) { await tab(index); await sleep(150); await shot(`${name}-${label}`); }
  }
  await send("Emulation.clearDeviceMetricsOverride");

  await load("/app/zzz");
  check("an unknown URL shows the not-found page", (await ev(`document.body.textContent`)).includes("There is nothing here."));
  await load("/app/login?next=/app/users");
  await ev(`${$("#sign-in")}.click()`);
  await pathIs("/app/users");
  check("no errors while routing", errors.filter((e) => !e.includes("Failed to load resource")).length === 0, JSON.stringify(errors));

  // ---- the host bridge (Stage 9.2): no host in a plain browser, and no port for a forged message ----
  const say = async () => {
    await ev(`${$("#native-say")}.click()`);
    return () => ev(`${$("#native-result")}.textContent`);
  };
  await load("/gallery");
  await tab(5);
  const noHost = await say();
  check("in a plain browser a plugin call fails at once with `unavailable`", await until(async () => (await noHost()) === "say: error unavailable", "the answer", 1500), await noHost());
  const ua = await ev(`navigator.userAgent`);
  await send("Emulation.setUserAgentOverride", { userAgent: `${ua} QuenchHost/1` });
  await load("/gallery");
  await tab(5);
  // A page script (or another window) can post a port, but not as the host does: the page ignores it.
  await ev(`(() => {
    window.__got = [];
    const c = new MessageChannel();
    c.port1.onmessage = (m) => window.__got.push(m.data);
    window.postMessage("quench-bridge", "*", [c.port2]);
    window.dispatchEvent(new MessageEvent("message", { data: "quench-bridge", ports: [new MessageChannel().port2] }));
  })()`);
  const hosted = await say();
  await sleep(500);
  check("a page that claims a host waits for it", await hosted() === "idle", await hosted());
  check("then gives up with `unavailable` after a few seconds", await until(async () => (await hosted()) === "say: error unavailable", "the timeout", 5000), await hosted());
  check("a forged port never received a call", await ev(`window.__got.length`) === 0);
  await send("Emulation.setUserAgentOverride", { userAgent: ua });

  // ---- the desktop transport: `webkit.messageHandlers.quench` to the host, `__quenchBridge(text)` back ----
  const fake = (await send("Page.addScriptToEvaluateOnNewDocument", { source: `window.__posted = [];
    window.webkit = { messageHandlers: { quench: { postMessage: (text) => {
      window.__posted.push(text);
      const call = JSON.parse(text);
      setTimeout(() => window.__quenchBridge(JSON.stringify({ id: call.id, ok: call.args.text })), 10);
    } } } };` })).result.identifier;
  await load("/gallery");
  await tab(5);
  const unclaimed = await say();
  check("a handler without the host's user agent is not used", await until(async () => (await unclaimed()) === "say: error unavailable", "the answer", 1500) && await ev(`window.__posted.length`) === 0, await unclaimed());
  await send("Emulation.setUserAgentOverride", { userAgent: `${ua} QuenchHost/1` });
  await load("/gallery");
  await tab(5);
  const desktop = await say();
  check("on a desktop host a call goes through the message handler and its answer comes back", await until(async () => (await desktop()) === "say: hi", "the answer", 3000), await desktop());
  const posted = JSON.parse(await ev(`window.__posted[0]`));
  check("the envelope the host receives is the contract's", posted.plugin === "echo" && posted.method === "say" && posted.args.text === "hi" && typeof posted.id === "number", JSON.stringify(posted));
  const descriptor = await ev(`(({ writable, configurable }) => ({ writable, configurable }))(Object.getOwnPropertyDescriptor(window, "__quenchBridge"))`);
  check("the page cannot replace `__quenchBridge`", descriptor.writable === false && descriptor.configurable === false, JSON.stringify(descriptor));
  await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: fake });
  await send("Emulation.setUserAgentOverride", { userAgent: ua });

  // ---- a server page that does not match: the result is the client's page ---------------------
  for (const kind of ["tag", "text", "drop", "extra", "attr"]) {
    await load(`/?corrupt=${kind}`);
    const mismatches = errors.filter((e) => e.includes("hydration mismatch"));
    check(`corrupt:${kind} is reported with a path and what was expected`, mismatches.length > 0 && mismatches.every((m) => m.includes("hydration mismatch in") && m.includes("expected")), JSON.stringify(errors));
    check(`corrupt:${kind} ends as the correct page`, await appHtml() === cleanDom, `\n got: ${await appHtml()}\n want: ${cleanDom}`);
    await ev(`${$("#up")}.click()`);
    check(`corrupt:${kind}: the repaired page works`, await ev(`${$("#count")}.textContent`) === "1");
  }
  await load("/?corrupt=empty");
  check("corrupt:empty (nothing rendered) is a plain client render", errors.length === 0 && await appHtml() === cleanDom, JSON.stringify(errors));

  // in production mode a mismatch is repaired without noise
  const injected = (await send("Page.addScriptToEvaluateOnNewDocument", { source: "window.QUENCH_DEV = false" })).result.identifier;
  await load("/?corrupt=tag");
  check("production mode: repaired quietly", errors.length === 0 && await appHtml() === cleanDom, JSON.stringify(errors));
  await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: injected });

  // ---- the policy really blocks inline code ----------------------------------------------------
  await load("/");
  await ev(`window.__violations = 0; document.addEventListener("securitypolicyviolation", () => window.__violations++);
    const s = document.createElement("script"); s.textContent = "window.__injected = 1"; document.body.append(s);`);
  await sleep(200);
  check("an injected inline script without the nonce is blocked", await ev(`window.__injected`) === undefined && await ev(`window.__violations`) >= 1);

  // ---- a panic reaches the console and shows the overlay; the app stops without hanging --------
  await load("/");
  errors.length = 0;
  await ev(`${$("#boom")}.click()`);
  await until(() => ev(`!!document.body.querySelector("pre")`), "the overlay");
  check("panic overlay shown with the message", (await ev(`document.body.querySelector("pre")?.textContent`) ?? "").includes("boom"));
  await ev(`${$("#up")}.click()`);
  check("the stopped app ignores later events", await ev(`${$("#count")}.textContent`) === "0");

  // Not a check: how long this machine took (headless Chromium, loopback, unthrottled).
  await load("/");
  const timing = await ev(`(() => {
    const at = (name) => performance.getEntriesByName(name)[0].startTime;
    const wasm = performance.getEntriesByType("resource").find((r) => new URL(r.name).pathname.endsWith(".wasm"));
    return JSON.stringify({ wasmKB: Math.round(wasm.decodedBodySize / 1024), wasmFetchMs: Math.round(wasm.duration),
      hydrateMs: Math.round(at("quench:ready") - at("quench:instantiated")), fetchToReadyMs: Math.round(at("quench:ready") - wasm.startTime) });
  })()`);
  console.log(`INFO  load timing: ${timing}`);
} catch (error) {
  check("test run", false, String(error));
} finally {
  ws.close();
  chrome.kill();
  rmSync(profile, { recursive: true, force: true });
}
console.log(failures === 0 ? "\nall browser checks passed" : `\n${failures} browser check(s) failed`);
process.exit(failures === 0 ? 0 : 1);
