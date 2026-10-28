// Servers the plugin tests talk to, on the host (the device reaches them through `adb reverse`):
//   node scripts/android-fixtures.mjs sink 3151        records what is PUT or POSTed: one JSON line per request
//   node scripts/android-fixtures.mjs file 3152 a.apk  serves one file for every request
//   node scripts/android-fixtures.mjs warehouse 3153   a stand-in for the warehouse: a login, and PUT of an artifact (201, then 409 for the same version)
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
} else if (kind === "warehouse") {
  const seen = new Set();
  createServer((req, res) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks);
      const log = (extra) => console.log(JSON.stringify({ method: req.method, url: req.url, auth: req.headers.authorization ?? null, type: req.headers["content-type"] ?? null, bytes: body.length, ...extra }));
      if (req.method === "POST" && req.url === "/gatehouse/api/v1/auth/login") {
        log({ login: JSON.parse(body.toString()).username });
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ access_token: "token-from-login" }));
      } else if (req.method === "PUT" && req.url.startsWith("/warehouse/api/v1/artifacts/")) {
        const again = seen.has(req.url);
        seen.add(req.url);
        log({ status: again ? 409 : 201 });
        res.writeHead(again ? 409 : 201);
        res.end();
      } else {
        res.writeHead(404);
        res.end();
      }
    });
  }).listen(Number(port), "127.0.0.1");
} else {
  const body = readFileSync(path);
  createServer((req, res) => {
    res.writeHead(200, { "content-type": "application/octet-stream", "content-length": body.length });
    res.end(body);
  }).listen(Number(port), "127.0.0.1");
}
