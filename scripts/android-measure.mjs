// Measures an installed app on the attached device or emulator: cold starts, memory and size.
//   node scripts/android-measure.mjs dev.lorehaven.rel --web [--runs 7]
//   node scripts/android-measure.mjs dev.lorehaven.flhello [--runs 7]
// Cold start: the app's process is stopped, then `am start -W` reports how long until its first frame
// (TotalTime). With --web (a quench host, debug build) it also reads the time the app was ready from
// the page's own marks, on the device's clock: from the start command to `quench:ready`.
// Memory: proportional set size after 5 s idle, of the app process and of the WebView renderers.
// It measures THIS device: an emulator on a fast host says nothing about a low-end phone.
import { execFileSync } from "node:child_process";
const [pkg, ...flags] = process.argv.slice(2);
const web = flags.includes("--web");
const runs = Number(flags[flags.indexOf("--runs") + 1] || 7);
const adb = (...args) => execFileSync("adb", args, { encoding: "utf8" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const launcher = adb("shell", "cmd", "package", "resolve-activity", "--brief", pkg).trim().split("\n").pop().trim();

async function ready() {
  for (let i = 0; i < 150; i++) {
    try {
      const pid = adb("shell", "pidof", pkg).trim().split(/\s+/)[0];
      if (pid) {
        adb("forward", "--remove-all");
        adb("forward", "tcp:9333", `localabstract:webview_devtools_remote_${pid}`);
        const list = await (await fetch("http://127.0.0.1:9333/json")).json();
        const ws = new WebSocket(list[0].webSocketDebuggerUrl);
        await new Promise((r) => (ws.onopen = r));
        const value = await new Promise((resolve) => {
          ws.onmessage = (m) => resolve(JSON.parse(m.data).result?.result?.value);
          ws.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { returnByValue: true, expression:
            `(() => { const m = performance.getEntriesByName("quench:ready")[0]; const w = performance.getEntriesByType("resource").find((r) => r.name.endsWith(".wasm") || r.name.includes(".wasm?")); return m ? JSON.stringify({ at: performance.timeOrigin + m.startTime, wasmMs: w ? w.duration : null, wasmKB: w ? Math.round(w.decodedBodySize / 1024) : null }) : null; })()` } }));
        });
        ws.close();
        if (value) return JSON.parse(value);
      }
    } catch { /* the page is not there yet */ }
    await sleep(100);
  }
  return null;
}

const total = [], wait = [], readyMs = [], wasmMs = [];
let wasmKB = null;
for (let i = 0; i < runs; i++) {
  adb("shell", "am", "force-stop", pkg);
  adb("shell", "input", "keyevent", "KEYCODE_HOME");
  await sleep(1500);
  const t0 = Number(adb("shell", "date", "+%s%3N").trim());
  const out = adb("shell", "am", "start", "-W", "-n", launcher);
  total.push(Number(/TotalTime: (\d+)/.exec(out)?.[1]));
  wait.push(Number(/WaitTime: (\d+)/.exec(out)?.[1]));
  if (web) {
    const r = await ready();
    if (r) { readyMs.push(Math.round(r.at - t0)); wasmMs.push(Math.round(r.wasmMs)); wasmKB = r.wasmKB; }
  }
}
await sleep(5000);
const mem = adb("shell", "dumpsys", "meminfo", "-a", pkg);
const appPss = Number(/TOTAL PSS:\s+(\d+)/.exec(mem)?.[1] ?? /TOTAL\s+(\d+)/.exec(mem)?.[1]);
const all = adb("shell", "dumpsys", "meminfo");
// "Total PSS by process:" lists each process once ("  52,168K: name (pid 123 / activity)").
const section = /Total PSS by process:\n([\s\S]*?)\n\n/.exec(all)?.[1] ?? "";
const processes = [...section.matchAll(/^\s*([\d,]+)K: (\S+) \(pid (\d+)/gm)].map((m) => ({ kb: Number(m[1].replace(/,/g, "")), name: m[2], pid: m[3] }));
const renderers = processes.filter((p) => p.name.includes("sandboxed_process"));
const apk = adb("shell", "pm", "path", pkg).trim().replace("package:", "");
const apkKB = Math.round(Number(adb("shell", "stat", "-c", "%s", apk).trim()) / 1024);
console.log(JSON.stringify({
  package: pkg, runs, abi: adb("shell", "getprop", "ro.product.cpu.abi").trim(),
  start_ms: { total_median: median(total), total_all: total, wait_median: median(wait) },
  ...(web ? { ready_ms: { median: median(readyMs), all: readyMs }, wasm_fetch_ms: { median: median(wasmMs) } } : {}),
  memory_pss_mb: { app: Math.round(appPss / 1024), webview_renderers: renderers.map((p) => `${Math.round(p.kb / 1024)} (pid ${p.pid})`), total: Math.round((appPss + renderers.reduce((a, p) => a + p.kb, 0)) / 1024) },
  apk_kb: apkKB,
}, null, 1));
