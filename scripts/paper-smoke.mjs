#!/usr/bin/env node

const baseUrl = (process.env.BALLAST_SMOKE_BASE_URL ?? "http://localhost:8080").replace(/\/$/, "");
const exchange = process.env.BALLAST_SMOKE_EXCHANGE ?? "okx";
const symbol = process.env.BALLAST_SMOKE_SYMBOL ?? "BTC/USDT";
const targetAmount = process.env.BALLAST_SMOKE_TARGET_AMOUNT ?? "25";
const timeoutMs = Number(process.env.BALLAST_SMOKE_TIMEOUT_MS ?? "90000");
const token = process.env.BALLAST_SMOKE_BEARER_TOKEN;

if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) {
  throw new Error("BALLAST_SMOKE_TIMEOUT_MS must be a positive number");
}

const runId = `${Date.now()}-${process.pid}`;
const headers = { "content-type": "application/json" };
if (token) headers.authorization = `Bearer ${token}`;

async function request(path, options = {}) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers: { ...headers, ...options.headers },
  });
  const text = await response.text();
  let body;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    throw new Error(`${options.method ?? "GET"} ${path} returned non-JSON HTTP ${response.status}`);
  }
  if (!response.ok) {
    throw new Error(`${options.method ?? "GET"} ${path} failed with HTTP ${response.status}: ${JSON.stringify(body)}`);
  }
  return body;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

console.log(`paper smoke: ${baseUrl} ${exchange} ${symbol}`);

const health = await request("/health");
assert(health.status === "ok", `server health is ${health.status}`);

const sync = await request("/api/v1/instruments/sync", {
  method: "POST",
  body: JSON.stringify({ exchanges: [exchange], reload: true }),
});
assert(sync.synchronized?.[exchange] > 0, `instrument sync failed: ${JSON.stringify(sync)}`);

const instruments = await request(
  `/api/v1/instruments?exchange=${encodeURIComponent(exchange)}&market_kind=spot&active_only=true&search=${encodeURIComponent(symbol)}&limit=100`,
);
const instrument = instruments.items?.find((item) => item.symbol === symbol);
assert(instrument, `${exchange} spot instrument ${symbol} was not found after synchronization`);

const template = await request("/api/v1/strategy-templates", {
  method: "POST",
  body: JSON.stringify({
    name: `release-smoke-${runId}`,
    description: "Automated v0.1 paper release smoke",
    strategy: "twap",
    quantity_unit: "quote_notional",
    duration_seconds: 30,
    slice_interval_ms: 5000,
    max_slippage_bps: 100,
    execution_backend: "managed_ioc",
    change_note: "release smoke",
  }),
});
const templateVersionId = template.versions?.find((version) => version.version === 1)?.id;
assert(templateVersionId, "created template did not return version 1");

const task = await request("/api/v1/tasks", {
  method: "POST",
  headers: { "idempotency-key": `release-smoke-${runId}` },
  body: JSON.stringify({
    instrument_id: instrument.id,
    side: "buy",
    target_amount: targetAmount,
    template_version_id: templateVersionId,
  }),
});
assert(task.execution_mode === "paper", `task execution mode is ${task.execution_mode}`);
assert(task.execution_backend === "managed_ioc", `task backend is ${task.execution_backend}`);

const deadline = Date.now() + timeoutMs;
let observedTask = task;
let slices = [];
let events = [];
while (Date.now() < deadline) {
  [observedTask, slices, events] = await Promise.all([
    request(`/api/v1/tasks/${task.id}`),
    request(`/api/v1/tasks/${task.id}/slices`),
    request(`/api/v1/events?task_id=${task.id}&limit=1000`),
  ]);
  const hasFill = slices.some((slice) => Number(slice.filled_native_quantity) > 0);
  const completedEvent = events.some(
    (event) => event.task_id === task.id
      && event.event_type === "task_state_changed"
      && event.payload?.status === "completed",
  );
  if (observedTask.status === "completed" && hasFill && completedEvent) break;
  if (["failed", "expired", "cancelled"].includes(observedTask.status)) {
    throw new Error(`task reached ${observedTask.status} before producing evidence: ${observedTask.failure_code ?? observedTask.paused_reason ?? "unknown"}`);
  }
  await sleep(2000);
}

assert(observedTask.status === "completed", `task did not complete within ${timeoutMs}ms; status=${observedTask.status}`);
assert(slices.some((slice) => Number(slice.filled_native_quantity) > 0), "task completed without an actual simulated fill");
assert(events.some(
  (event) => event.task_id === task.id
    && event.event_type === "task_state_changed"
    && event.payload?.status === "completed",
), "task completion audit event was not produced");

console.log(JSON.stringify({
  status: "ok",
  task_id: task.id,
  task_status: observedTask.status,
  slice_count: slices.length,
  event_count: events.length,
  instrument: `${exchange}:${symbol}`,
}, null, 2));
