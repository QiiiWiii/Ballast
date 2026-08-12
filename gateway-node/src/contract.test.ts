import assert from "node:assert/strict";
import test from "node:test";
import ccxt from "ccxt";
import type { Exchange as CcxtExchange, MarketInterface as CcxtMarket } from "ccxt";

import type { ExchangeId } from "./exchanges/adapter.js";
import { nextHistoricalCursor, normalizeHistoricalCandle, normalizeHistoricalTrade, normalizeInstrument, normalizeInstruments, normalizeOrderBook, normalizeTrade, withReadRetry } from "./exchanges/ccxt-adapter.js";

const exchanges: readonly ExchangeId[] = ["binance", "okx", "bybit", "gate_io", "bitget"];
const decimalPlacesClient = { precisionMode: ccxt.DECIMAL_PLACES } as CcxtExchange;

for (const exchange of exchanges) {
  test(`${exchange} fixture normalizes spot and perpetual contracts`, () => {
    const spot = normalizeInstrument(exchange, decimalPlacesClient, marketFixture({ spot: true }));
    const linear = normalizeInstrument(exchange, decimalPlacesClient, marketFixture({ swap: true, linear: true }));
    const inverse = normalizeInstrument(exchange, decimalPlacesClient, marketFixture({ swap: true, inverse: true, settle: "BTC" }));

    assert.equal(spot.key.marketKind, "spot");
    assert.equal(spot.contractKind, undefined);
    assert.equal(linear.contractKind, "linear");
    assert.equal(inverse.contractKind, "inverse");
    assert.equal(linear.priceTick, "0.1");
    assert.equal(linear.quantityStep, "0.001");
  });
}

test("fixture order book preserves sequence and depth", () => {
  const instrument = { exchange: "binance", marketKind: "spot", symbol: "BTC/USDT" } as const;
  const book = normalizeOrderBook(instrument, {
    bids: [[100, 2], [99, 3]], asks: [[101, 1], [102, 4]], nonce: 42, timestamp: 1_700_000_000_000,
  } as never, 1);
  assert.deepEqual(book.bids, [{ price: "100", quantity: "2" }]);
  assert.equal(book.sequence, "42");
});

test("fixture trades have stable exchange-scoped event ids", () => {
  const instrument = { exchange: "okx", marketKind: "perpetual", symbol: "BTC/USDT:USDT" } as const;
  const trade = normalizeTrade("okx", instrument, { id: "trade-1", price: 100, amount: 3, side: "sell", timestamp: 7 } as never, 0);
  assert.equal(trade.eventId, "okx:trade-1");
  assert.equal(trade.quantity, "3");
  assert.equal(trade.takerSide, "sell");
});

test("historical fixtures preserve timestamps, ids, and decimal strings", () => {
  assert.deepEqual(normalizeHistoricalCandle([7, 100, 110, 90, 105, 12.5]), {
    openTimeMs: 7, open: "100", high: "110", low: "90", close: "105", volume: "12.5",
  });
  assert.deepEqual(normalizeHistoricalTrade({ id: "t-1", timestamp: 8, price: 105, amount: 2, side: "buy" } as never), {
    exchangeTradeId: "t-1", tradeTimeMs: 8, price: "105", quantity: "2", takerSide: "buy",
  });
});

test("historical trades without exchange ids fail explicitly", () => {
  assert.throws(() => normalizeHistoricalTrade({ timestamp: 8, price: 105, amount: 2 } as never), /trade.id/);
});

test("historical pagination advances and clamps to the requested boundary", () => {
  assert.equal(nextHistoricalCursor([1_000, 2_000], 1_000, 2_500, 1_000), 2_500);
  assert.equal(nextHistoricalCursor([], 1_000, 2_500, 1_000), 2_500);
  assert.equal(nextHistoricalCursor([1_000], 1_000, 5_000, 0), 1_000);
});

test("saturated same-millisecond trade pages never skip to the next millisecond", () => {
  const upstream = [
    { id: "a", timestamp: 1_000 },
    { id: "b", timestamp: 1_001 },
    { id: "c", timestamp: 1_001 },
    { id: "d-unseen", timestamp: 1_001 },
  ];
  const firstPage = upstream.slice(0, 3);
  const firstCursor = nextHistoricalCursor(
    firstPage.map((trade) => trade.timestamp), 1_000, 5_000, 0,
  );
  assert.equal(firstCursor, 1_001);
  assert.notEqual(firstCursor, 1_002);
  assert.deepEqual(
    upstream.filter((trade) => trade.timestamp >= firstCursor).map((trade) => trade.id),
    ["b", "c", "d-unseen"],
  );

  const repeatedCursor = nextHistoricalCursor(
    [1_001, 1_001, 1_001], firstCursor, 5_000, 0,
  );
  assert.equal(repeatedCursor, firstCursor);
});

test("read-only historical calls retry transient failures", async () => {
  let attempts = 0;
  const value = await withReadRetry(async () => {
    attempts += 1;
    if (attempts < 3) throw new Error("rate limited");
    return "ok";
  });
  assert.equal(value, "ok");
  assert.equal(attempts, 3);
});

test("partial historical failures surface after the retry budget", async () => {
  let attempts = 0;
  await assert.rejects(withReadRetry(async () => {
    attempts += 1;
    throw new Error("upstream unavailable");
  }), /upstream unavailable/);
  assert.equal(attempts, 3);
});

test("missing precision fails explicitly", () => {
  assert.throws(() => normalizeInstrument("bitget", decimalPlacesClient, marketFixture({ spot: true, missingPrecision: true })), /precision is unavailable/);
});

test("instrument listing rejects invalid markets without dropping valid markets", async () => {
  const { instruments, rejected } = normalizeInstruments("okx", decimalPlacesClient, [
    marketFixture({ spot: true }),
    marketFixture({ spot: true, missingPrecision: true }),
  ]);
  assert.equal(instruments.length, 1);
  assert.equal(instruments[0]?.key.symbol, "BTC/USDT:USDT");
  assert.deepEqual(rejected, [{
    symbol: "BTC/USDT:USDT",
    reason: "market price precision is unavailable",
  }]);
});

test("instrument listing does not downgrade unexpected normalization failures", () => {
  const invalidClient = { precisionMode: ccxt.DECIMAL_PLACES } as CcxtExchange;
  const market = marketFixture({ spot: true });
  Object.defineProperty(market, "precision", {
    get() { throw new TypeError("programmer failure"); },
  });

  assert.throws(
    () => normalizeInstruments("okx", invalidClient, [market]),
    /programmer failure/,
  );
});

test("non-positive optional trading limits are treated as unavailable", () => {
  const instrument = normalizeInstrument(
    "gate_io",
    decimalPlacesClient,
    marketFixture({ spot: true, zeroLimits: true }),
  );

  assert.equal(instrument.minimumQuantity, undefined);
  assert.equal(instrument.minimumNotional, undefined);
});

function marketFixture(options: {
  spot?: boolean;
  swap?: boolean;
  linear?: boolean;
  inverse?: boolean;
  settle?: string;
  missingPrecision?: boolean;
  zeroLimits?: boolean;
}): CcxtMarket {
  return {
    id: "BTCUSDT",
    symbol: options.inverse ? "BTC/USD:BTC" : "BTC/USDT:USDT",
    base: "BTC",
    quote: options.inverse ? "USD" : "USDT",
    settle: options.settle ?? (options.swap ? "USDT" : undefined),
    spot: options.spot ?? false,
    swap: options.swap ?? false,
    linear: options.linear,
    inverse: options.inverse,
    contractSize: options.swap ? 1 : undefined,
    precision: options.missingPrecision ? {} : { price: 1, amount: 3 },
    limits: options.zeroLimits
      ? { amount: { min: 0 }, cost: { min: 0 } }
      : { amount: { min: 0.001 }, cost: { min: 5 } },
    maker: 0.0002,
    taker: 0.0005,
    active: true,
  } as CcxtMarket;
}
