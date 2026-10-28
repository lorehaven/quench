// Runs a script in the page of a desktop host started with QUENCH_DESKTOP_CONTROL=<socket> (a debug build) and
// prints its value as JSON, like scripts/webview-eval.mjs does for the Android host.
//   node scripts/linux-eval.mjs <socket> "document.title"
//   node scripts/linux-eval.mjs <socket> --center "document.querySelector('button')"   # x y of the element's middle
import net from "node:net";

const [socket, ...rest] = process.argv.slice(2);
let script = rest.join(" ");
if (rest[0] === "--center") {
  const target = rest.slice(1).join(" ");
  script = `(() => { const r = (${target}).getBoundingClientRect(); return Math.round(r.x + r.width / 2) + " " + Math.round(r.y + r.height / 2); })()`;
}
const client = net.createConnection(socket);
let buffer = "";
const timer = setTimeout(() => { console.error("no answer in 25 s"); process.exit(1); }, 25000);
client.on("error", (error) => { console.error(error.message); process.exit(1); });
client.on("data", (chunk) => {
  buffer += chunk;
  if (!buffer.includes("\n")) return;
  clearTimeout(timer);
  const reply = JSON.parse(buffer.split("\n")[0]);
  if ("error" in reply) { console.error(reply.error); process.exit(1); }
  console.log(rest[0] === "--center" ? reply.ok : JSON.stringify(reply.ok));
  client.end();
});
client.write(script + "\n");
