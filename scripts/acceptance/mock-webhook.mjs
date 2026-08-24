import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createHttpsServer } from "node:https";

const stateDirectory = "/state";
const stateFile = `${stateDirectory}/events.jsonl`;
mkdirSync(stateDirectory, { recursive: true });
try {
  readFileSync(stateFile);
} catch {
  writeFileSync(stateFile, "");
}

function json(response, status, value) {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(body),
  });
  response.end(body);
}

async function body(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

function events() {
  return readFileSync(stateFile, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line));
}

function handle(request, response) {
  const url = new URL(request.url, "https://acceptance-webhook:8444");
  if (request.method === "GET" && url.pathname === "/healthz") {
    return json(response, 200, { status: "ok" });
  }
  if (request.method === "GET" && url.pathname === "/events") {
    return json(response, 200, events());
  }
  if (request.method === "DELETE" && url.pathname === "/events") {
    writeFileSync(stateFile, "");
    return json(response, 200, { status: "cleared" });
  }
  if (request.method === "POST" && url.pathname === "/hook") {
    return body(request)
      .then((payload) => {
        appendFileSync(stateFile, `${JSON.stringify({ received_at: new Date().toISOString(), payload })}\n`);
        response.writeHead(204);
        response.end();
      })
      .catch(() => json(response, 400, { error: "invalid_json" }));
  }
  json(response, 404, { error: "not_found" });
}

const health = createHttpServer(handle);
const https = createHttpsServer({
  cert: readFileSync("/certs/webhook.crt"),
  key: readFileSync("/certs/webhook.key"),
}, handle);

health.listen(8081, "0.0.0.0");
https.listen(8444, "0.0.0.0");
