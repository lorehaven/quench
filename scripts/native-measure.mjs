// Slice 0 of the native renderer: the memory of a minimal window drawn by the native stack, against the same
// kind of window shown by the WebView host. It opens each window on your display for a few seconds.
//   cargo build --release -p quench-example-native-window -p quench-desktop --features quench-desktop/window
//   node scripts/native-measure.mjs
// RSS counts a page for every process that maps it; PSS divides a shared page among the processes that do, so
// PSS is the fair total for a browser engine's several processes. Both are summed over the process tree.
import { spawn } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const parent = (pid) => { try { return Number(readFileSync(`/proc/${pid}/stat`, "utf8").split(") ")[1].split(" ")[1]); } catch { return 0; } };
const tree = (root) => {
  const all = readdirSync("/proc").filter((p) => /^\d+$/.test(p)).map(Number);
  const found = new Set([root]);
  for (let grew = true; grew;) {
    grew = false;
    for (const p of all) if (!found.has(p) && found.has(parent(p))) { found.add(p); grew = true; }
  }
  return [...found];
};
const memory = (root) => {
  let rss = 0, pss = 0, n = 0;
  for (const p of tree(root)) {
    try {
      rss += Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${p}/status`, "utf8"))[1]);
      pss += Number(/Pss:\s+(\d+)/.exec(readFileSync(`/proc/${p}/smaps_rollup`, "utf8"))[1]);
      n++;
    } catch { /* the process ended */ }
  }
  return { rss, pss, n };
};
// A session bus that is not there makes GTK's accessibility bridge crash: use the user's own when it exists.
const bus = `${process.env.XDG_RUNTIME_DIR ?? `/run/user/${process.getuid()}`}/bus`;
const session = existsSync(bus) ? { DBUS_SESSION_BUS_ADDRESS: `unix:path=${bus}` } : {};
async function run(label, program, env, settle) {
  const child = spawn(program, [], { env: { ...process.env, ...session, ...env }, stdio: ["ignore", "pipe", "ignore"] });
  let said = "";
  child.stdout.on("data", (d) => (said += d));
  await sleep(settle);
  const m = memory(child.pid);
  console.log(`${label}: RSS ${(m.rss / 1024).toFixed(1)} MiB, PSS ${(m.pss / 1024).toFixed(1)} MiB, ${m.n} process(es)`);
  child.kill("SIGTERM");
  await sleep(500);
  return said;
}
const said = await run("native ", "target/release/quench-example-native-window", { FIXTURE_SECONDS: "8" }, 4000);
console.log("   ", said.split("\n").filter((l) => /^(opened|frame)/.test(l)).join(" | "));
await run("webview", "target/release/quench-desktop-fixture", { FIXTURE: "app" }, 6000);
