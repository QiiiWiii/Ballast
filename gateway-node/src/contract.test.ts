import assert from "node:assert/strict";
import test from "node:test";
import ccxt from "ccxt";
import type { Exchange as CcxtExchange, MarketInterface as CcxtMarket } from "ccxt";

import type { ExchangeId } from "./exchanges/adapter.js";
import { normalizeInstrument, normalizeOrderBook, normalizeTrade } from "./exchanges/ccxt-adapter.js";

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

test("missing precision fails explicitly", () => {
  assert.throws(() => normalizeInstrument("bitget", decimalPlacesClient, marketFixture({ spot: true, missingPrecision: true })), /precision is unavailable/);
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
