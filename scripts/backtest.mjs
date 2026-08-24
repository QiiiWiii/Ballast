import { spawn } from "node:child_process";

const apiBase = (process.env.BACKTEST_API_BASE ?? "http://server:8080").replace(/\/$/, "");
const bearerToken = process.env.BACKTEST_BEARER_TOKEN;
const exchange = process.env.BACKTEST_EXCHANGE ?? "okx";
const marketKind = process.env.BACKTEST_MARKET_KIND ?? "spot";
const symbol = process.env.BACKTEST_SYMBOL ?? "BTC/USDT";
const dataType = process.env.BACKTEST_DATA_TYPE ?? "trades";
const timeframe = process.env.BACKTEST_TIMEFRAME ?? "1m";
const startAt = required("BACKTEST_START_AT");
const endAt = required("BACKTEST_END_AT");
const startAtMs = parseTimestamp(startAt, "BACKTEST_START_AT");
const endAtMs = parseTimestamp(endAt, "BACKTEST_END_AT");
if (startAtMs >= endAtMs) throw new Error("BACKTEST_START_AT must be before BACKTEST_END_AT");
const targetAmount = process.env.BACKTEST_TARGET_AMOUNT ?? "1";
const strategy = process.env.BACKTEST_STRATEGY ?? "twap";
const quantityUnit = process.env.BACKTEST_QUANTITY_UNIT ?? "base_quantity";
const tickIntervalSeconds = positiveInteger("BACKTEST_TICK_INTERVAL_SECONDS", "60");
const maxSlippageBps = boundedInteger("BACKTEST_MAX_SLIPPAGE_BPS", "100", 0, 10_000);
const participationRate = process.env.BACKTEST_PARTICIPATION_RATE?.trim() || null;
const pageLimit = boundedInteger("BACKTEST_PAGE_LIMIT", "500", 1, 1_000);
const maxPages = boundedInteger("BACKTEST_MAX_PAGES", "25", 1, 100);
const maxRows = boundedInteger("BACKTEST_MAX_ROWS", "100000", 1, 1_000_000);
const pollIntervalMs = positiveInteger("BACKTEST_POLL_INTERVAL_MS", "1000");
const backfillTimeoutSeconds = positiveInteger("BACKTEST_BACKFILL_TIMEOUT_SECONDS", "900");
const requestTimeoutMs = boundedInteger("BACKTEST_REQUEST_TIMEOUT_MS", "30000", 1_000, 300_000);
const idempotencyKey = process.env.BACKTEST_IDEMPOTENCY_KEY?.trim()
  || `backtest-${exchange}-${symbol}-${dataType}-${startAt}-${endAt}`;

function required(name) {
  const value = process.env[name];
  if (!value?.trim()) throw new Error(`${name} is required`);
  return value.trim();
}

function parseTimestamp(value, name) {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) throw new Error(`${name} must be an RFC 3339 timestamp`);
  return timestamp;
}

function positiveInteger(name, fallback) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive safe integer`);
  }
  return value;
}

function boundedInteger(name, fallback, minimum, maximum) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be an integer between ${minimum} and ${maximum}`);
  }
  return value;
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function request(path, options = {}) {
  const headers = { accept: "application/json", ...(options.headers ?? {}) };
  if (bearerToken) headers.authorization = `Bearer ${bearerToken}`;
  const response = await fetch(`${apiBase}${path}`, {
    ...options,
    headers,
    signal: options.signal ?? AbortSignal.timeout(requestTimeoutMs),
  });
  const text = await response.text();
  let body = null;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
  }
  if (!response.ok) {
    throw new Error(`${options.method ?? "GET"} ${path} failed with ${response.status}: ${JSON.stringify(body)}`);
  }
  return body;
}

async function syncInstrument() {
  await request("/api/v1/instruments/sync", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ exchanges: [exchange], reload: false }),
  });
  const query = new URLSearchParams({
    exchange,
    market_kind: marketKind,
    active_only: "true",
    search: symbol,
    limit: "100",
  });
  const page = await request(`/api/v1/instruments?${query}`);
  const instrument = page.items?.find((item) => item.symbol === symbol);
  if (!instrument) throw new Error(`instrument not found: ${exchange}:${marketKind}:${symbol}`);
  return instrument;
}

async function backfill() {
  const body = {
    exchange,
    market_kind: marketKind,
    symbol,
    data_type: dataType,
    start_at: startAt,
    end_at: endAt,
    idempotency_key: idempotencyKey,
    page_limit: pageLimit,
    max_pages: maxPages,
  };
  if (dataType === "ohlcv") body.timeframe = timeframe;
  let job = await request("/api/v1/history/backfills", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const deadline = Date.now() + backfillTimeoutSeconds * 1_000;
  while (job.status !== "completed") {
    if (job.status === "failed") throw new Error(`historical backfill failed: ${job.last_error_code}`);
    if (Date.now() >= deadline) throw new Error("historical backfill polling timed out");
    await sleep(Math.min(pollIntervalMs, deadline - Date.now()));
    if (Date.now() >= deadline) throw new Error("historical backfill polling timed out");
    job = await request("/api/v1/history/backfills", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  }
  return job;
}

async function historyRows(instrument) {
  const items = [];
  let offset = 0;
  let total = 0;
  while (offset < total || offset === 0) {
    const query = new URLSearchParams({
      exchange,
      market_kind: marketKind,
      symbol,
      start_at: startAt,
      end_at: endAt,
      limit: String(Math.min(1_000, pageLimit)),
      offset: String(offset),
    });
    if (dataType === "ohlcv") query.set("timeframe", timeframe);
    const path = dataType === "ohlcv" ? "/api/v1/history/ohlcv" : "/api/v1/history/trades";
    const page = await request(`${path}?${query}`);
    total = page.total;
    items.push(...page.items);
    offset += page.items.length;
    if (page.items.length === 0 || items.length >= Math.min(total, maxRows)) break;
  }
  if (items.length === 0) throw new Error(`no historical ${dataType} rows returned`);
  return {
    instrument,
    samples: items.slice(0, maxRows).map((item) => ({
      at: dataType === "ohlcv" ? item.open_time : item.trade_time,
      price: dataType === "ohlcv" ? item.close : item.price,
      quantity: dataType === "ohlcv" ? item.volume : item.quantity,
    })),
  };
}

function replay(instrument, samples) {
  const input = {
    instrument: {
      id: {
        exchange: instrument.exchange,
        market_kind: instrument.market_kind,
        symbol: instrument.symbol,
      },
      exchange_symbol: instrument.exchange_symbol,
      base_asset: instrument.base_asset,
      quote_asset: instrument.quote_asset,
      settle_asset: instrument.settle_asset,
      contract_kind: instrument.contract_kind,
      contract_size: instrument.contract_size,
      price_tick: instrument.price_tick,
      quantity_step: instrument.quantity_step,
      minimum_quantity: instrument.minimum_quantity,
      minimum_notional: instrument.minimum_notional,
      maker_fee_rate: instrument.maker_fee_rate,
      taker_fee_rate: instrument.taker_fee_rate,
      active: instrument.active,
    },
    side: process.env.BACKTEST_SIDE ?? "buy",
    strategy,
    quantity_unit: quantityUnit,
    target_amount: targetAmount,
    start_at: startAt,
    end_at: endAt,
    tick_interval_seconds: tickIntervalSeconds,
    max_slippage_bps: maxSlippageBps,
    participation_rate: participationRate,
    samples,
  };
  return new Promise((resolve, reject) => {
    const child = spawn("/usr/local/bin/ballast-backtest", [], {
      stdio: ["pipe", "pipe", "inherit"],
    });
    const chunks = [];
    child.stdout.on("data", (chunk) => chunks.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`ballast-backtest exited with ${code}`));
        return;
      }
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (error) {
        reject(error);
      }
    });
    child.stdin.end(JSON.stringify(input));
  });
}

const instrument = await syncInstrument();
const job = await backfill();
const { samples } = await historyRows(instrument);
const report = await replay(instrument, samples);
console.log(JSON.stringify({
  backfill_job_id: job.id,
  data_type: dataType,
  exchange,
  market_kind: marketKind,
  symbol,
  start_at: startAt,
  end_at: endAt,
  ...report,
}, null, 2));
