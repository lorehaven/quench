// Servers the plugin tests talk to, on the host (the device reaches them through `adb reverse`):
//   node scripts/android-fixtures.mjs sink 3151        records what is PUT or POSTed: one JSON line per request
//   node scripts/android-fixtures.mjs file 3152 a.apk  serves one file for every request
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
const [kind, port, path] = process.argv.slice(2);
if (kind === "sink") {
  createServer((req, res) => {
    const hash = createHash("sha256");
    let bytes = 0;
    req.on("data", (c) => { hash.update(c); bytes += c.length; });
    req.on("end", () => {
      console.log(JSON.stringify({ method: req.method, bytes, sha256: hash.digest("hex") }));
      res.end("stored");
    });
  }).listen(Number(port), "127.0.0.1");
} else {
  const body = readFileSync(path);
  createServer((req, res) => {
    res.writeHead(200, { "content-type": "application/octet-stream", "content-length": body.length });
    res.end(body);
  }).listen(Number(port), "127.0.0.1");
}
