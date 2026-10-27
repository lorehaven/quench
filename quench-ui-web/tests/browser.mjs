// End-to-end check in a real browser: drives headless Chromium over the DevTools protocol.
// Run through scripts/ui-web-test.fish, which builds the wasm and starts the example server.
//   node quench-ui-web/tests/browser.mjs http://127.0.0.1:3101/
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const url = process.argv[2] ?? "http://127.0.0.1:3101/";
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
const consoleErrors = [];
const exceptions = [];
ws.onmessage = ({ data }) => {
  const m = JSON.parse(data);
  if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
  else if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") {
    consoleErrors.push(m.params.args.map((a) => a.value ?? a.description).join(" "));
  } else if (m.method === "Runtime.exceptionThrown") exceptions.push(m.params.exceptionDetails.text);
  else if (m.method === "Log.entryAdded" && m.params.entry.level === "error") consoleErrors.push(m.params.entry.text);
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

try {
  await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
  await send("Page.navigate", { url });
  await until(() => ev(`${$("#server-status")}?.textContent.includes("from the server")`), "the page to load and fetch");

  check("the app rendered", await ev(`${$("h1")}?.textContent`) === "quench-ui");
  check("the fetch from quench-http rendered the list", await ev(`document.querySelectorAll("#server-list li").length`) === 3
    && await ev(`${$("#server-list li.done")}?.textContent`) === "read the contract");

  // counter: one patch per click, and the DOM node for the number stays the same node
  await ev(`window.__countNode = ${$("#count")}.firstChild`);
  for (let i = 0; i < 3; i++) await ev(`${$("#up")}.click()`);
  await ev(`${$("#down")}.click()`);
  check("counter counts", await ev(`${$("#count")}.textContent`) === "2");
  check("counter patched the same text node", await ev(`window.__countNode === ${$("#count")}.firstChild`));

  // todo: typing, submit (no page reload), controlled input value, keyed identity
  const type = (text) => ev(`(() => { const i = ${$("#draft")}; i.value = ${JSON.stringify(text)}; i.dispatchEvent(new Event("input", {bubbles: true})); })()`);
  await ev(`window.__marker = 42`);
  await type("milk");
  await ev(`${$("#add-form")}.requestSubmit()`);
  check("submit added an item", await ev(`document.querySelectorAll("ul li").length`) >= 1 && (await ev(`${$("#left")}.textContent`)) === "1 left");
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

  // handles: adding and removing many items returns the handle table to where it was
  const handles = () => ev(`window.__quench.handles()`);
  const before = await handles();
  for (let i = 0; i < 100; i++) { await type(`item ${i}`); await ev(`${$("#add-form")}.requestSubmit()`); }
  const during = await handles();
  while (await ev(`document.querySelectorAll("button.remove").length`) > 2) await ev(`document.querySelector("button.remove").click()`);
  const after = await handles();
  check("100 items added and removed: the handle table returns to its size", during > before + 300 && after === before, `${before} -> ${during} -> ${after}`);

  // fetch again
  await ev(`${$("#server-status")}.firstChild.data = "stale"`); // not textContent: that would replace the node Rust patches
  await ev(`${$("#reload")}.click()`);
  check("reload fetched again", await until(() => ev(`${$("#server-status")}.textContent`).then((t) => t !== "stale" && t.includes("from the server")), "the reload"));

  // Not a check: how long this machine took (headless Chromium, loopback, unthrottled).
  const timing = await ev(`(() => {
    const at = (name) => performance.getEntriesByName(name)[0].startTime;
    const wasm = performance.getEntriesByType("resource").find((r) => new URL(r.name).pathname === "/app.wasm");
    return JSON.stringify({
      wasmKB: Math.round(wasm.decodedBodySize / 1024),
      wasmFetchMs: Math.round(wasm.duration),
      instantiateAndFirstRenderMs: Math.round(at("quench:ready") - at("quench:instantiated")),
      fetchToReadyMs: Math.round(at("quench:ready") - wasm.startTime),
    });
  })()`);
  console.log(`INFO  load timing: ${timing}`);

  check("no console errors or exceptions so far", consoleErrors.length === 0 && exceptions.length === 0, JSON.stringify([...consoleErrors, ...exceptions]));

  // a panic reaches the console and shows the overlay; the app stops without hanging the page
  await ev(`${$("#boom")}.click()`);
  await until(() => ev(`!!document.body.querySelector("pre")`), "the overlay");
  check("panic overlay shown with the message", (await ev(`document.body.querySelector("pre")?.textContent`) ?? "").includes("boom"));
  await ev(`${$("#up")}.click()`);
  check("the stopped app ignores later events", await ev(`${$("#count")}.textContent`) === "2");
} catch (error) {
  check("test run", false, String(error));
} finally {
  ws.close();
  chrome.kill();
  rmSync(profile, { recursive: true, force: true });
}
console.log(failures === 0 ? "\nall browser checks passed" : `\n${failures} browser check(s) failed`);
process.exit(failures === 0 ? 0 : 1);
