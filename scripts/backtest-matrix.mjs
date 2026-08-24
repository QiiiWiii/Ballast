import { spawn } from "node:child_process";

const runner = "/opt/ballast/backtest.mjs";
const defaultExchanges = "okx,binance,bybit,gate_io,bitget";
const defaultStrategies = "twap,pov";
const povRate = process.env.BACKTEST_MATRIX_POV_RATE ?? "0.05";
const matrix = loadCases();
const startedAt = new Date().toISOString();
const results = [];

for (const testCase of matrix) {
  console.error(`[backtest-matrix] running ${testCase.case_id}`);
  results.push(await runCase(testCase));
}

const finishedAt = new Date().toISOString();
const completedCases = results.filter((result) => result.status === "completed").length;
const partialCases = results.filter((result) => result.status === "partial").length;
const failedCases = results.filter((result) => result.status === "failed").length;
const status = failedCases > 0
  ? (completedCases + partialCases > 0 ? "partial" : "failed")
  : partialCases > 0 ? "partial" : "completed";

console.log(JSON.stringify({
  kind: "backtest_matrix",
  version: 1,
  status,
  started_at: startedAt,
  finished_at: finishedAt,
  requested_cases: matrix.length,
  completed_cases: completedCases,
  partial_cases: partialCases,
  failed_cases: failedCases,
  cases: results,
}, null, 2));
process.exitCode = failedCases > 0 ? 1 : 0;

function required(name) {
  const value = process.env[name];
  if (!value?.trim()) throw new Error(`${name} is required`);
  return value.trim();
}

function loadCases() {
  required("BACKTEST_START_AT");
  required("BACKTEST_END_AT");
  const encoded = process.env.BACKTEST_MATRIX_JSON?.trim();
  const cases = encoded ? parseJsonCases(encoded) : defaultCases();
  if (cases.length === 0) throw new Error("backtest matrix must contain at least one case");
  const normalized = cases.map(normalizeCase);
  const ids = new Set();
  for (const testCase of normalized) {
    if (ids.has(testCase.case_id)) throw new Error(`duplicate backtest case id: ${testCase.case_id}`);
    ids.add(testCase.case_id);
  }
  return normalized;
}

function parseJsonCases(encoded) {
  let value;
  try {
    value = JSON.parse(encoded);
  } catch (error) {
    throw new Error(`BACKTEST_MATRIX_JSON is invalid: ${error.message}`);
  }
  if (!Array.isArray(value)) throw new Error("BACKTEST_MATRIX_JSON must be an array");
  return value;
}

function defaultCases() {
  const exchanges = parseList(process.env.BACKTEST_EXCHANGES ?? defaultExchanges, "BACKTEST_EXCHANGES");
  const strategies = parseList(process.env.BACKTEST_STRATEGIES ?? defaultStrategies, "BACKTEST_STRATEGIES");
  return exchanges.flatMap((exchange) => strategies.map((strategy) => ({
    case_id: `${exchange}-${strategy}`,
    exchange,
    strategy,
    participation_rate: strategy === "pov" ? povRate : undefined,
  })));
}

function parseList(value, name) {
  const values = value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
  if (values.length === 0) throw new Error(`${name} must contain at least one value`);
  return values;
}

function normalizeCase(value, index) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`backtest matrix case ${index + 1} must be an object`);
  }
  const exchange = value.exchange ?? process.env.BACKTEST_EXCHANGE ?? "okx";
  const strategy = value.strategy ?? process.env.BACKTEST_STRATEGY ?? "twap";
  const symbol = value.symbol ?? process.env.BACKTEST_SYMBOL ?? "BTC/USDT";
  const caseId = value.case_id ?? value.id ?? `${exchange}-${strategy}-${symbol}`;
  return {
    ...value,
    case_id: String(caseId),
    exchange: String(exchange),
    strategy: String(strategy),
    symbol: String(symbol),
  };
}

function descriptor(testCase) {
  return {
    case_id: testCase.case_id,
    exchange: testCase.exchange,
    market_kind: testCase.market_kind ?? process.env.BACKTEST_MARKET_KIND ?? "spot",
    symbol: testCase.symbol,
    strategy: testCase.strategy,
    quantity_unit: testCase.quantity_unit ?? process.env.BACKTEST_QUANTITY_UNIT ?? "base_quantity",
    target_amount: testCase.target_amount ?? process.env.BACKTEST_TARGET_AMOUNT ?? "1",
    side: testCase.side ?? process.env.BACKTEST_SIDE ?? "buy",
    data_type: testCase.data_type ?? process.env.BACKTEST_DATA_TYPE ?? "trades",
    timeframe: testCase.timeframe ?? process.env.BACKTEST_TIMEFRAME ?? "1m",
    tick_interval_seconds: testCase.tick_interval_seconds
      ?? process.env.BACKTEST_TICK_INTERVAL_SECONDS
      ?? "60",
    max_slippage_bps: testCase.max_slippage_bps
      ?? process.env.BACKTEST_MAX_SLIPPAGE_BPS
      ?? "100",
  };
}

function environmentFor(testCase) {
  const environment = { ...process.env };
  const fields = {
    exchange: "BACKTEST_EXCHANGE",
    market_kind: "BACKTEST_MARKET_KIND",
    symbol: "BACKTEST_SYMBOL",
    data_type: "BACKTEST_DATA_TYPE",
    timeframe: "BACKTEST_TIMEFRAME",
    target_amount: "BACKTEST_TARGET_AMOUNT",
    strategy: "BACKTEST_STRATEGY",
    quantity_unit: "BACKTEST_QUANTITY_UNIT",
    side: "BACKTEST_SIDE",
    tick_interval_seconds: "BACKTEST_TICK_INTERVAL_SECONDS",
    max_slippage_bps: "BACKTEST_MAX_SLIPPAGE_BPS",
    participation_rate: "BACKTEST_PARTICIPATION_RATE",
    page_limit: "BACKTEST_PAGE_LIMIT",
    max_pages: "BACKTEST_MAX_PAGES",
    max_rows: "BACKTEST_MAX_ROWS",
    idempotency_key: "BACKTEST_IDEMPOTENCY_KEY",
  };
  for (const [field, name] of Object.entries(fields)) {
    if (testCase[field] !== undefined && testCase[field] !== null) {
      environment[name] = String(testCase[field]);
    }
  }
  return environment;
}

function runCase(testCase) {
  const details = descriptor(testCase);
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [runner], {
      env: environmentFor(testCase),
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout = [];
    const stderr = [];
    let settled = false;
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", (error) => {
      if (settled) return;
      settled = true;
      resolve({ ...details, status: "failed", error: sanitizeError(error.message) });
    });
    child.on("close", (code) => {
      if (settled) return;
      settled = true;
      if (code !== 0) {
        resolve({
          ...details,
          status: "failed",
          error: sanitizeError(Buffer.concat(stderr).toString("utf8") || `runner exited with ${code}`),
        });
        return;
      }
      try {
        const report = JSON.parse(Buffer.concat(stdout).toString("utf8"));
        resolve({
          ...details,
          status: report.status,
          backfill_job_id: report.backfill_job_id,
          model: report.model,
          target_amount: report.target_amount,
          executed_amount: report.executed_amount,
          residual_amount: report.residual_amount,
          sample_count: report.sample_count,
          total_ticks: report.total_ticks,
          filled_ticks: report.filled_ticks,
          average_price: report.average_price,
          worst_price: report.worst_price,
          slippage_bps: report.slippage_bps,
          fee_amount: report.fee_amount,
          fee_asset: report.fee_asset,
          filled_quote_amount: report.filled_quote_amount,
        });
      } catch (error) {
        resolve({ ...details, status: "failed", error: sanitizeError(`invalid runner output: ${error.message}`) });
      }
    });
  });
}

function sanitizeError(value) {
  return value
    .replace(/Bearer\s+[^\s]+/gi, "Bearer [redacted]")
    .trim()
    .slice(-2_000);
}
