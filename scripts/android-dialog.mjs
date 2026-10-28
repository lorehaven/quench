// Presses a button of the system's UI on the attached device, found by resource id or by text.
//   node scripts/android-dialog.mjs permission_allow_button        a resource id (or the end of one)
//   node scripts/android-dialog.mjs text:Documents                 an element whose text is exactly this
// It waits for the element to stop moving (a dialog slides in, and a tap on its way in is lost), then
// taps its centre. Exits 1 when there is no such element on the screen.
import { execFileSync } from "node:child_process";
const target = process.argv[2];
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const match = (node) =>
  target.startsWith("text:")
    ? node.includes(` text="${target.slice(5)}"`)
    : new RegExp(`resource-id="[^"]*${target}"`).test(node);
const find = () => {
  const xml = execFileSync("adb", ["exec-out", "uiautomator", "dump", "/dev/tty"], { encoding: "utf8" });
  const node = [...xml.matchAll(/<node[^>]*>/g)].map((m) => m[0]).find(match);
  return node?.match(/bounds="(\[\d+,\d+\]\[\d+,\d+\])"/)?.[1] ?? null;
};
let bounds = find();
for (let i = 0; i < 3 && bounds; i++) {
  await sleep(500);
  const again = find();
  if (again === bounds) break;
  bounds = again; // it moved: look again
}
// A dialog ignores touches for a moment after it appears (protection against tap-jacking).
if (bounds) await sleep(1500);
const numbers = bounds?.match(/\d+/g)?.map(Number);
if (!numbers) {
  console.error(`no ${target} on the screen`);
  process.exit(1);
}
const [x1, y1, x2, y2] = numbers;
execFileSync("adb", ["shell", "input", "tap", String(Math.round((x1 + x2) / 2)), String(Math.round((y1 + y2) / 2))]);
