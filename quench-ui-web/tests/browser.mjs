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
  check("an empty note is refused, naming the problem", await until(async () => (await message()) === "text: write something", "the validation message"));
  await typeInto("#note-text", "buy milk"); await submit("#note-form");
  await until(() => ev(`document.querySelectorAll("#notes li").length === 1`), "the first note");
  await typeInto("#note-text", "walk the dog"); await submit("#note-form");
  check("notes are added and the field is cleared", await until(() => ev(`document.querySelectorAll("#notes li").length === 2`), "two notes")
    && await ev(`${$("#note-text")}.value`) === "" && await message() === "");
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
  await typeInto("#note-text", "streamed"); await submit("#note-form");
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
