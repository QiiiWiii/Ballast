import { generateKeyPairSync, createSign } from "node:crypto";
import { readFileSync } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createHttpsServer } from "node:https";

const issuer = process.env.ACCEPTANCE_ISSUER ?? "https://acceptance-oidc:8443";
const audience = process.env.ACCEPTANCE_AUDIENCE ?? "ballast-acceptance";
const roleClaim = process.env.ACCEPTANCE_ROLE_CLAIM ?? "ballast_roles";
const kid = "ballast-acceptance-key";
const { privateKey, publicKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
const publicJwk = {
  ...publicKey.export({ format: "jwk" }),
  alg: "RS256",
  kid,
  use: "sig",
};

function json(response, status, value) {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(body),
  });
  response.end(body);
}

function base64url(value) {
  return Buffer.from(value).toString("base64url");
}

function signToken(input) {
  const now = Math.floor(Date.now() / 1000);
  const header = base64url(JSON.stringify({ alg: "RS256", kid, typ: "JWT" }));
  const payload = base64url(JSON.stringify({
    iss: issuer,
    aud: input.audience ?? audience,
    sub: input.sub ?? "acceptance-user",
    iat: now,
    exp: now + 300,
    [roleClaim]: Array.isArray(input.roles) ? input.roles : ["viewer"],
  }));
  const signingInput = `${header}.${payload}`;
  const signature = createSign("RSA-SHA256")
    .update(signingInput)
    .sign(privateKey)
    .toString("base64url");
  return `${signingInput}.${signature}`;
}

async function readJson(request) {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

function handleHttps(request, response) {
  const url = new URL(request.url, issuer);
  if (request.method === "GET" && url.pathname === "/.well-known/openid-configuration") {
    return json(response, 200, {
      issuer,
      jwks_uri: `${issuer}/jwks.json`,
      token_endpoint: `${issuer}/token`,
    });
  }
  if (request.method === "GET" && url.pathname === "/jwks.json") {
    return json(response, 200, { keys: [publicJwk] });
  }
  if (request.method === "POST" && url.pathname === "/token") {
    return readJson(request)
      .then((input) => json(response, 200, { access_token: signToken(input), token_type: "Bearer", expires_in: 300 }))
      .catch(() => json(response, 400, { error: "invalid_request" }));
  }
  json(response, 404, { error: "not_found" });
}

const health = createHttpServer((request, response) => {
  if (request.url === "/healthz") return json(response, 200, { status: "ok" });
  json(response, 404, { error: "not_found" });
});

const https = createHttpsServer({
  cert: readFileSync("/certs/oidc.crt"),
  key: readFileSync("/certs/oidc.key"),
}, handleHttps);

health.listen(8080, "0.0.0.0");
https.listen(8443, "0.0.0.0");
