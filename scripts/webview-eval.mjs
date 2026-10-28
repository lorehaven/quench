// Drives the app's WebView over the DevTools protocol, for the Android scripts.
//   adb forward tcp:9333 localabstract:webview_devtools_remote_<pid>
//   node scripts/webview-eval.mjs "location.href" "document.title"     one JSON value per expression
//   node scripts/webview-eval.mjs --center "document.querySelector('#go')"
//       prints "x y": where on the device screen the element is (scrolled into view), for `adb shell input tap`
//   node scripts/webview-eval.mjs --navigate http://10.0.2.2:3141/
// Debug builds of the host enable WebView debugging.
const port = process.env.WEBVIEW_PORT ?? 9333;
const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
if (!list.length) {
  console.error("no page in the WebView");
  process.exit(1);
}
const ws = new WebSocket(list[0].webSocketDebuggerUrl);
await new Promise((resolve) => (ws.onopen = resolve));
let next = 0;
const waiting = new Map();
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && waiting.has(d.id)) waiting.get(d.id)(d.result);
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    waiting.set(++next, resolve);
    ws.send(JSON.stringify({ id: next, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.value;

const [mode, ...rest] = process.argv.slice(2);
if (mode === "--navigate") {
  await send("Page.enable");
  await send("Page.navigate", { url: rest[0] });
} else if (mode === "--center") {
  // The page's top edge on the screen is the host's inset, which DevTools reports as screenY.
  const top = JSON.parse(list[0].description || "{}").screenY ?? 0;
  const at = await evaluate(`(() => {
    const e = ${rest[0]};
    e.scrollIntoView({ inline: "center", block: "center", behavior: "instant" });
    const r = e.getBoundingClientRect();
    return [Math.round((r.x + r.width / 2) * devicePixelRatio), Math.round((r.y + r.height / 2) * devicePixelRatio)];
  })()`);
  // Scrolling settles on the next frame.
  await new Promise((resolve) => setTimeout(resolve, 150));
  const again = await evaluate(`(() => { const r = (${rest[0]}).getBoundingClientRect(); return [Math.round((r.x + r.width / 2) * devicePixelRatio), Math.round((r.y + r.height / 2) * devicePixelRatio)]; })()`);
  console.log(`${again?.[0] ?? at[0]} ${(again?.[1] ?? at[1]) + top}`);
} else {
  for (const expression of process.argv.slice(2)) console.log(JSON.stringify((await evaluate(expression)) ?? null));
}
process.exit(0);
