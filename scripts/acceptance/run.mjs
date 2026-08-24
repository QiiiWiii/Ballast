import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { request as httpsRequest } from "node:https";

const apiBase = process.env.ACCEPTANCE_API_BASE ?? "http://server:8080";
const oidcBase = process.env.ACCEPTANCE_OIDC_BASE ?? "https://acceptance-oidc:8443";
const webhookBase = process.env.ACCEPTANCE_WEBHOOK_BASE ?? "https://acceptance-webhook:8444";
const statePath = "/state/context.json";
const accountId = "acceptance-okx-demo";

async function fetchJson(url, options = {}) {
  const result = url.startsWith("https://")
    ? await requestHttps(url, options)
    : await fetch(url, options).then(async (response) => ({
      ok: response.ok,
      status: response.status,
      text: await response.text(),
    }));
  const text = result.text;
  let body = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
  }
  return { response: result, body };
}

function requestHttps(url, options) {
  return new Promise((resolve, reject) => {
    const request = httpsRequest(url, {
      method: options.method ?? "GET",
      headers: options.headers,
      ca: readFileSync(process.env.ACCEPTANCE_CA_FILE ?? "/certs/ca.crt"),
    }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve({
        ok: response.statusCode >= 200 && response.statusCode < 300,
        status: response.statusCode,
        text: Buffer.concat(chunks).toString("utf8"),
      }));
    });
    request.on("error", reject);
    if (options.body !== undefined) request.write(options.body);
    request.end();
  });
}

async function token(sub, roles, audience = "ballast-acceptance") {
  const { response, body } = await fetchJson(`${oidcBase}/token`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ sub, roles, audience }),
  });
  assert(response.ok && body?.access_token, `token issuance failed: ${response.status}`);
  return body.access_token;
}

async function api(method, path, bearer, body, expectedStatus) {
  const headers = { accept: "application/json" };
  if (bearer) headers.authorization = `Bearer ${bearer}`;
  if (body !== undefined) {
    headers["content-type"] = "application/json";
  }
  const { response, body: responseBody } = await fetchJson(`${apiBase}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (expectedStatus !== undefined) {
    assert(
      response.status === expectedStatus,
      `${method} ${path}: expected ${expectedStatus}, got ${response.status}: ${JSON.stringify(responseBody)}`,
    );
  }
  return responseBody;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function uniqueKey(prefix) {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function limit(scopeType, scopeId, instrumentId, positive) {
  const value = positive ? "100000" : "0";
  return {
    scope_type: scopeType,
    scope_id: scopeId,
    allowed_exchanges: ["okx"],
    allowed_accounts: [accountId],
    allowed_instruments: [instrumentId],
    allowed_backends: ["managed_ioc"],
    max_order_notional: value,
    max_task_notional: value,
    max_daily_notional: value,
    max_active_tasks: positive ? 10 : 0,
    max_slippage_bps: positive ? 100 : 0,
    max_market_age_ms: positive ? 60_000 : 0,
    max_account_age_ms: positive ? 600_000 : 0,
    max_net_exposure: value,
    max_native_algo_orders: 0,
    max_native_duration_seconds: 0,
  };
}

async function setLimits(admin, instrumentId, positive) {
  for (const [scopeType, scopeId] of [
    ["global", "global"],
    ["exchange", "okx"],
    ["account", accountId],
  ]) {
    await api("POST", "/api/v1/risk/limits", admin, limit(scopeType, scopeId, instrumentId, positive), 200);
  }
}

async function createLiveTask(operator, context, prefix) {
  return requestWithHeaders("POST", "/api/v1/tasks", operator, {
    instrument_id: context.instrument_id,
    side: "buy",
    target_amount: "5",
    template_version_id: context.template_version_id,
    execution_mode: "live",
    account_id: accountId,
  }, 201, {
    "idempotency-key": uniqueKey(prefix),
  });
}

async function requestWithHeaders(method, path, bearer, body, expectedStatus, extraHeaders = {}) {
  const headers = { ...extraHeaders, accept: "application/json" };
  if (bearer) headers.authorization = `Bearer ${bearer}`;
  if (body !== undefined) {
    headers["content-type"] = "application/json";
  }
  const { response, body: responseBody } = await fetchJson(`${apiBase}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  assert(
    response.status === expectedStatus,
    `${method} ${path}: expected ${expectedStatus}, got ${response.status}: ${JSON.stringify(responseBody)}`,
  );
  return responseBody;
}

async function prepare() {
  const admin = await token("acceptance-admin", ["admin", "operator", "viewer"]);
  const operator = await token("acceptance-operator", ["operator", "viewer"]);
  const viewer = await token("acceptance-viewer", ["viewer"]);

  const unauthenticated = await fetchJson(`${apiBase}/api/v1/accounts`);
  assert(unauthenticated.response.status === 401, "unauthenticated account read was not rejected");
  await api("GET", "/api/v1/accounts", viewer, undefined, 200);
  await api("POST", "/api/v1/risk/kill-switches", viewer, {
    scope_type: "global",
    scope_id: "global",
    enabled: false,
    reason: "viewer role rejection test",
  }, 403);

  await api("POST", "/api/v1/instruments/sync", operator, { exchanges: ["okx"], reload: true }, 200);
  const instruments = await api(
    "GET",
    "/api/v1/instruments?exchange=okx&market_kind=spot&active_only=true&search=BTC%2FUSDT&limit=100",
    viewer,
    undefined,
    200,
  );
  const instrument = instruments.items?.find((item) => item.symbol === "BTC/USDT");
  assert(instrument, "acceptance instrument was not synchronized");

  const template = await api("POST", "/api/v1/strategy-templates", operator, {
    name: `acceptance-${Date.now()}`,
    description: "Docker controlled acceptance template",
    strategy: "twap",
    quantity_unit: "quote_notional",
    duration_seconds: 60,
    slice_interval_ms: 5_000,
    max_slippage_bps: 100,
    execution_backend: "managed_ioc",
    change_note: "acceptance",
  }, 201);
  const templateVersionId = template.versions?.[0]?.id;
  assert(templateVersionId, "acceptance template version is missing");

  await setLimits(admin, instrument.id, false);
  await api("POST", "/api/v1/risk/kill-switches", admin, {
    scope_type: "global",
    scope_id: "global",
    enabled: false,
    reason: "acceptance baseline",
  }, 200);
  const readiness = await api("GET", "/api/v1/live/readiness", undefined, undefined, 200);
  assert(readiness.enabled === false, "baseline acceptance server unexpectedly enabled live");
  assert(readiness.oidc === "configured", "OIDC did not reach configured readiness");
  assert(readiness.kill_switches.configured >= 1, "global kill switch was not configured");
  assert(readiness.limits === "configured", "risk limits did not reach configured readiness");

  mkdirSync("/state", { recursive: true });
  writeFileSync(statePath, JSON.stringify({
    instrument_id: instrument.id,
    template_version_id: templateVersionId,
  }));
  console.log(JSON.stringify({ phase: "prepare", instrument_id: instrument.id, template_version_id: templateVersionId }));
}

async function waitForWebhookEvent(event) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const { response, body } = await fetchJson(`${webhookBase}/events`);
    assert(response.ok, `webhook events request failed: ${response.status}`);
    if (body.some((entry) => entry.payload?.event === event)) return;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`webhook event was not received: ${event}`);
}

async function live() {
  const context = JSON.parse(readFileSync(statePath, "utf8"));
  const admin = await token("acceptance-admin", ["admin", "operator", "viewer"]);
  const operator = await token("acceptance-operator", ["operator", "viewer"]);
  const operatorAdmin = await token("acceptance-operator", ["admin", "operator", "viewer"]);
  const viewer = await token("acceptance-viewer", ["viewer"]);

  const readiness = await api("GET", "/api/v1/live/readiness", undefined, undefined, 200);
  assert(readiness.enabled === true, "live acceptance server did not enable live");
  assert(readiness.prerequisites.every((item) => item.ready), "live readiness prerequisites are incomplete");

  await requestWithHeaders("POST", "/api/v1/tasks", viewer, {
    instrument_id: context.instrument_id,
    side: "buy",
    target_amount: "5",
    template_version_id: context.template_version_id,
    execution_mode: "live",
    account_id: accountId,
  }, 403, { "idempotency-key": uniqueKey("viewer-live") });

  const zeroLimit = await requestWithHeaders("POST", "/api/v1/tasks", operator, {
    instrument_id: context.instrument_id,
    side: "buy",
    target_amount: "5",
    template_version_id: context.template_version_id,
    execution_mode: "live",
    account_id: accountId,
  }, 409, { "idempotency-key": uniqueKey("zero-limit") });
  assert(zeroLimit.error?.code === "max_order_notional_exceeded", "zero limit did not block the live task");

  await setLimits(admin, context.instrument_id, true);
  const task = await createLiveTask(operator, context, "approval");
  assert(task.status === "pending_approval", `live task did not enter approval: ${task.status}`);
  const approvals = await api("GET", "/api/v1/approvals", viewer, undefined, 200);
  assert(approvals.items.some((item) => item.id === task.id), "pending task was not listed for approval");

  const selfApproval = await requestWithHeaders(
    "POST",
    `/api/v1/approvals/${task.id}/approve`,
    operatorAdmin,
    undefined,
    403,
  );
  assert(selfApproval.error?.code === "self_approval_forbidden", "self approval was not rejected");

  const approved = await requestWithHeaders(
    "POST",
    `/api/v1/approvals/${task.id}/approve`,
    admin,
    undefined,
    200,
  );
  assert(["scheduled", "running", "paused", "failed"].includes(approved.status), "approval did not advance the task");

  const cleared = await fetchJson(`${webhookBase}/events`, { method: "DELETE" });
  assert(cleared.response.status === 200, "failed to clear acceptance webhook events");
  await api("POST", "/api/v1/risk/kill-switches", admin, {
    scope_type: "global",
    scope_id: "global",
    enabled: true,
    reason: "acceptance kill switch test",
  }, 200);
  await waitForWebhookEvent("kill_switch_changed");
  const blockedResponse = await requestWithHeaders("POST", "/api/v1/tasks", operator, {
    instrument_id: context.instrument_id,
    side: "buy",
    target_amount: "5",
    template_version_id: context.template_version_id,
    execution_mode: "live",
    account_id: accountId,
  }, 409, { "idempotency-key": uniqueKey("kill-switch") });
  assert(blockedResponse.error?.code === "kill_switch_enabled", "kill switch did not block submission");

  await api("POST", "/api/v1/risk/kill-switches", admin, {
    scope_type: "global",
    scope_id: "global",
    enabled: false,
    reason: "acceptance cleanup",
  }, 200);
  const risk = await api("GET", "/api/v1/risk", viewer, undefined, 200);
  const decisions = risk.decisions ?? [];
  assert(decisions.some((decision) => decision.code === "max_order_notional_exceeded"), "zero-limit decision was not audited");
  assert(decisions.some((decision) => decision.code === "kill_switch_enabled"), "kill-switch decision was not audited");

  const { body: webhookEvents } = await fetchJson(`${webhookBase}/events`);
  assert(webhookEvents.some((entry) => entry.payload?.event === "kill_switch_changed"), "kill-switch webhook payload missing");
  const serialized = JSON.stringify(webhookEvents);
  assert(!serialized.includes("balances") && !serialized.includes("positions") && !serialized.includes("api_key"), "webhook payload contains sensitive fields");
  console.log(JSON.stringify({ phase: "live", status: "passed", task_id: task.id }));
}

const phase = process.argv[2];
if (phase === "prepare") {
  await prepare();
} else if (phase === "live") {
  await live();
} else {
  throw new Error("usage: run.mjs prepare|live");
}
