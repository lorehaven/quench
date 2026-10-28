// Slice 2 gate of the native renderer: draws the gallery with the native stack and with headless Chromium, and
// compares the two. Geometry: every element box the native layout produced against getBoundingClientRect().
// Pixels: the two screenshots, as a share of pixels that differ by more than a threshold.
//
//   node scripts/native-gate.mjs [width] [height] [out-dir]
//
// Only this program's own headless Chromium page is captured (Page.captureScreenshot); nothing of the desktop.
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { inflateSync, deflateSync } from "node:zlib";

const width = Number(process.argv[2] ?? 390);
const height = Number(process.argv[3] ?? 844);
const out = resolve(process.argv[4] ?? join(tmpdir(), "quench-native-gate"));
mkdirSync(out, { recursive: true });

execFileSync("cargo", ["run", "-q", "-p", "quench-example-native-gallery", "--", out, String(width), String(height)], { stdio: "inherit" });

const port = 9800 + Math.floor(Math.random() * 400);
const profile = mkdtempSync(join(tmpdir(), "quench-gate-chrome-"));
const chrome = spawn(process.env.CHROMIUM ?? "chromium", [
  "--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
  `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "about:blank",
], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function connect() {
  for (let i = 0; i < 100; i++) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = pages.find((p) => p.type === "page");
      if (page) return new WebSocket(page.webSocketDebuggerUrl);
    } catch { /* still starting */ }
    await sleep(100);
  }
  throw new Error("could not reach chromium");
}

const ws = await connect();
await new Promise((r) => (ws.onopen = r));
let id = 0;
const waiting = new Map();
ws.onmessage = ({ data }) => {
  const m = JSON.parse(data);
  if (m.id && waiting.has(m.id)) { waiting.get(m.id)(m); waiting.delete(m.id); }
};
const send = (method, params = {}) => new Promise((resolve) => {
  const mid = ++id;
  waiting.set(mid, resolve);
  ws.send(JSON.stringify({ id: mid, method, params }));
});

function decodePng(buf) {
  let at = 8, w = 0, h = 0, depth = 0, kind = 0; const idat = [];
  while (at < buf.length) {
    const len = buf.readUInt32BE(at), type = buf.toString("latin1", at + 4, at + 8), data = buf.subarray(at + 8, at + 8 + len);
    if (type === "IHDR") { w = data.readUInt32BE(0); h = data.readUInt32BE(4); depth = data[8]; kind = data[9]; }
    if (type === "IDAT") idat.push(data);
    at += 12 + len;
  }
  if (depth !== 8 || (kind !== 6 && kind !== 2)) throw new Error(`unsupported PNG ${depth}/${kind}`);
  const bpp = kind === 6 ? 4 : 3, stride = w * bpp;
  const raw = inflateSync(Buffer.concat(idat));
  const px = Buffer.alloc(w * h * 4);
  let prev = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? line[i - bpp] : 0, b = prev[i], c = i >= bpp ? prev[i - bpp] : 0;
      const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
      const add = filter === 0 ? 0 : filter === 1 ? a : filter === 2 ? b : filter === 3 ? (a + b) >> 1 : (pa <= pb && pa <= pc ? a : pb <= pc ? b : c);
      line[i] = (line[i] + add) & 255;
    }
    for (let x = 0; x < w; x++) {
      px[(y * w + x) * 4] = line[x * bpp]; px[(y * w + x) * 4 + 1] = line[x * bpp + 1]; px[(y * w + x) * 4 + 2] = line[x * bpp + 2];
      px[(y * w + x) * 4 + 3] = bpp === 4 ? line[x * bpp + 3] : 255;
    }
    prev = line;
  }
  return { w, h, px };
}

const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
const crc32 = (b) => { let c = 0xffffffff; for (const x of b) c = crcTable[(c ^ x) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
function encodePng(w, h, px) {
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) px.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type, "latin1"), data]);
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const head = Buffer.alloc(13); head.writeUInt32BE(w, 0); head.writeUInt32BE(h, 4); head[8] = 8; head[9] = 6;
  return Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", head), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

let failures = 0;
try {
  await send("Page.enable");
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }, { name: "hover", value: "hover" }, { name: "pointer", value: "fine" }] });
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await send("Page.navigate", { url: `file://${out}/page.html` });
  await sleep(1200);

  const evaluated = await send("Runtime.evaluate", {
    returnByValue: true,
    expression: `JSON.stringify([...document.querySelectorAll('[data-nk]')].map(e => { const r = e.getBoundingClientRect(); const s = getComputedStyle(e); return { k: +e.dataset.nk, x: r.x + scrollX, y: r.y + scrollY, w: r.width, h: r.height, tag: e.tagName.toLowerCase(), cls: e.getAttribute('class') || '', display: s.display }; }))`,
  });
  const theirs = new Map(JSON.parse(evaluated.result.result.value).map((e) => [e.k, e]));
  const ours = JSON.parse(readFileSync(`${out}/layout.json`, "utf8"));

  let compared = 0, within1 = 0, within3 = 0; const worst = [];
  for (const o of ours) {
    const t = theirs.get(o.k);
    if (!t || t.display === "inline" || t.display === "none" || t.display === "contents") continue;
    compared++;
    const d = Math.max(Math.abs(o.x - t.x), Math.abs(o.y - t.y), Math.abs(o.w - t.w), Math.abs(o.h - t.h));
    if (d <= 1) within1++;
    if (d <= 3) within3++;
    if (d > 1) worst.push({ d, tag: t.tag, cls: t.cls, ours: [o.x, o.y, o.w, o.h].map((v) => +v.toFixed(1)), theirs: [t.x, t.y, t.w, t.h].map((v) => +v.toFixed(1)), k: o.k });
  }
  if (process.env.GATE_DUMP) for (const k of process.env.GATE_DUMP.split(",").map(Number)) {
    const o = ours.find((e) => e.k === k), t = theirs.get(k);
    console.log(`  #${k} ${t?.tag ?? "?"}.${t?.cls ?? ""} ${t?.display ?? ""}  ours ${o ? [o.x, o.y, o.w, o.h].join(",") : "-"}  chromium ${t ? [t.x, t.y, t.w, t.h].map((v) => +v.toFixed(2)).join(",") : "-"}`);
  }
  if (!process.env.GATE_ORDER) worst.sort((a, b) => b.d - a.d);
  console.log(`geometry: ${compared} boxes, ${within1} within 1px (${(100 * within1 / compared).toFixed(1)}%), ${within3} within 3px (${(100 * within3 / compared).toFixed(1)}%)`);
  for (const w of worst.slice(0, Number(process.env.GATE_LIST ?? 20))) console.log(`  ${w.d.toFixed(1).padStart(6)}px  ${w.tag}${w.cls ? "." + w.cls.split(" ").join(".") : ""} #${w.k}  ours ${w.ours.join(",")}  chromium ${w.theirs.join(",")}`);

  const shot = await send("Page.captureScreenshot", { format: "png" });
  const chromePng = Buffer.from(shot.result.data, "base64");
  writeFileSync(`${out}/chromium.png`, chromePng);
  const a = decodePng(readFileSync(`${out}/native.png`)), b = decodePng(chromePng);
  if (a.w !== b.w || a.h !== b.h) throw new Error(`sizes differ: ${a.w}x${a.h} vs ${b.w}x${b.h}`);
  const diff = Buffer.alloc(a.px.length);
  let differing = 0;
  for (let i = 0; i < a.px.length; i += 4) {
    const d = Math.max(Math.abs(a.px[i] - b.px[i]), Math.abs(a.px[i + 1] - b.px[i + 1]), Math.abs(a.px[i + 2] - b.px[i + 2]));
    const bad = d > 32;
    if (bad) differing++;
    diff[i] = bad ? 255 : b.px[i] >> 2; diff[i + 1] = bad ? 0 : b.px[i + 1] >> 2; diff[i + 2] = bad ? 0 : b.px[i + 2] >> 2; diff[i + 3] = 255;
  }
  writeFileSync(`${out}/diff.png`, encodePng(a.w, a.h, diff));
  const share = 100 * differing / (a.w * a.h);
  console.log(`pixels: ${differing} of ${a.w * a.h} differ by more than 32 (${share.toFixed(2)}%)  -> ${out}/diff.png`);
  const geometryOk = within3 / compared >= Number(process.env.GATE_GEOMETRY ?? 0.9);
  const pixelsOk = share <= Number(process.env.GATE_PIXELS ?? 8);
  console.log(`${geometryOk ? "PASS" : "FAIL"}  geometry (${(100 * within3 / compared).toFixed(1)}% within 3px)`);
  console.log(`${pixelsOk ? "PASS" : "FAIL"}  pixels (${share.toFixed(2)}% differ)`);
  failures = (geometryOk ? 0 : 1) + (pixelsOk ? 0 : 1);
} finally {
  ws.close(); chrome.kill(); await sleep(200);
  try { rmSync(profile, { recursive: true, force: true }); } catch { /* the profile may be busy */ }
}
process.exit(failures ? 1 : 0);
