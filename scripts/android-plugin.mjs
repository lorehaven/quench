// Calls a plugin method of the running app through the plugin console (scripts/android-console) and
// prints the answer as JSON: {"ok": value} or {"error": {"code", "message"}}.
//   node scripts/android-plugin.mjs dev.lorehaven.console jobs run_now '{"task":"heartbeat"}'
// It forwards the app's WebView DevTools port, so a debug build must be running.
import { execFileSync } from "node:child_process";
const [pkg, plugin, method, args = "{}"] = process.argv.slice(2);
// A page that is paused (a system dialog is in front of the app) answers only when it is back: give up
// after a while instead of waiting for ever.
const deadline = Number(process.env.PLUGIN_TIMEOUT_MS ?? 100000);
// Output goes through a pipe when this is used in a command substitution, and exiting straight after
// `console.log` can drop it: leave only when the write has been taken.
let finished = false;
const done = (text) => {
  if (finished) return;
  finished = true;
  process.stdout.write(`${text}\n`, () => process.exit(0));
};
const stop = () => new Promise(() => {}); // wait for the write above to end the process
setTimeout(() => done(JSON.stringify({ error: { code: "timeout", message: `no answer in ${deadline / 1000} s` } })), deadline).unref();
const fail = (code, message) => done(JSON.stringify({ error: { code, message } }));
process.on("uncaughtException", (e) => fail("helper", String(e.message ?? e)));
process.on("unhandledRejection", (e) => fail("helper", String(e?.message ?? e)));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const port = process.env.WEBVIEW_PORT ?? "9333";
// The app may be starting, or its page loading: look for up to 20 s before saying it is not there.
let ws = null;
let problem = ["not_running", `${pkg} is not running`];
for (let i = 0; i < 40 && !ws; i++) {
  let pid = "";
  try {
    pid = execFileSync("adb", ["shell", "pidof", pkg], { encoding: "utf8" }).trim().split(/\s+/)[0];
  } catch { /* not running: pidof exits 1 */ }
  if (pid) {
    try {
      execFileSync("adb", ["forward", "--remove-all"]);
      execFileSync("adb", ["forward", `tcp:${port}`, `localabstract:webview_devtools_remote_${pid}`]);
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      if (list.length) {
        const socket = new WebSocket(list[0].webSocketDebuggerUrl);
        await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = () => reject(new Error("websocket")); });
        ws = socket;
      } else problem = ["no_devtools", "the app's WebView has no page"];
    } catch (e) {
      problem = ["no_devtools", `cannot reach the app's WebView: ${e.message}`];
    }
  } else problem = ["not_running", `${pkg} is not running`];
  if (!ws) await sleep(500);
}
if (!ws) {
  fail(...problem);
  await stop();
}
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id === 1) {
    const text = d.result?.result?.value;
    if (text === undefined) fail("evaluate", JSON.stringify(d.result));
    done(text);
  }
};
const expression = `(async () => {
  const set = (id, v) => { const e = document.querySelector('#' + id); e.value = v; e.dispatchEvent(new Event('input', { bubbles: true })); };
  set('plugin', ${JSON.stringify(plugin)}); set('method', ${JSON.stringify(method)}); set('args', ${JSON.stringify(args)});
  await new Promise((r) => setTimeout(r, 100));
  document.querySelector('#run').click();
  for (let i = 0; i < 150; i++) {
    await new Promise((r) => setTimeout(r, 100));
    const t = document.querySelector('#out').textContent;
    if (t !== 'running' && t !== 'idle') {
      const m = /^(ok|error): (.*)$/s.exec(t);
      if (m[1] === 'ok') return JSON.stringify({ ok: JSON.parse(m[2]) });
      const [code, ...rest] = m[2].split(': ');
      return JSON.stringify({ error: { code, message: rest.join(': ') } });
    }
  }
  return JSON.stringify({ error: { code: 'timeout', message: 'no answer in 15 s' } });
})()`;
ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
