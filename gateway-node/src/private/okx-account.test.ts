import assert from "node:assert/strict";
import test from "node:test";
import type { Balances, Exchange as CcxtExchange, Order as CcxtOrder, Position as CcxtPosition } from "ccxt";

import { MarketKind } from "../generated/ballast/gateway/v1/MarketKind.js";
import {
  OkxAccountAdapter,
  fetchOrdinaryOpenOrders,
  normalizeBalances,
  normalizeOrder,
  normalizePosition,
  rejectAlgoOpenOrders,
} from "./okx-account.js";

const account = {
  accountId: "okx-demo",
  exchange: "okx" as const,
  environment: "demo" as const,
  apiKey: "api-key",
  secret: "secret",
  password: "passphrase",
};

test("snapshot enables demo mode, preserves string parsing, paginates orders, and closes the client", async () => {
  const calls: string[] = [];
  const client = {
    has: { fetchPositions: true, fetchOpenOrders: true },
    loadMarkets: async () => { calls.push("loadMarkets"); },
    fetchBalance: async () => {
      calls.push("fetchBalance");
      return { timestamp: 1_700_000_000_000, USDT: { total: "1", free: "1", used: "0" } };
    },
    fetchPositions: async () => { calls.push("fetchPositions"); return []; },
    fetchOpenOrders: async (...args: unknown[]) => {
      calls.push(`fetchOpenOrders:${String(args[2])}:${JSON.stringify(args[3])}`);
      return [];
    },
    fetchTime: async () => { calls.push("fetchTime"); return 1_700_000_000_010; },
    setSandboxMode: (enabled: boolean) => { calls.push(`sandbox:${enabled}`); },
    close: async () => { calls.push("close"); },
  } as unknown as CcxtExchange;
  const adapter = new OkxAccountAdapter(account, 1_000, client);

  assert.equal((client as unknown as { number: unknown }).number, String);
  const snapshot = await adapter.snapshot();
  await adapter.close();

  assert.deepEqual(snapshot.balances, [{ asset: "USDT", total: "1", available: "1" }]);
  assert.deepEqual(snapshot.positions, []);
  assert.deepEqual(snapshot.openOrders, []);
  assert.equal(snapshot.exchangeTimeMs, "1700000000010");
  assert.ok(Number(snapshot.gatewayReceivedAtMs) >= 1_700_000_000_010);
  assert.deepEqual(calls, [
    "sandbox:true",
    "loadMarkets",
    "fetchBalance",
    "fetchPositions",
    "fetchOpenOrders:100:{}",
    "fetchTime",
    "fetchOpenOrders:1:{\"method\":\"privateGetTradeOrdersAlgoPending\",\"ordType\":\"conditional\"}",
    "fetchOpenOrders:1:{\"method\":\"privateGetTradeOrdersAlgoPending\",\"ordType\":\"oco\"}",
    "fetchOpenOrders:1:{\"method\":\"privateGetTradeOrdersAlgoPending\",\"ordType\":\"trigger\"}",
    "fetchOpenOrders:1:{\"method\":\"privateGetTradeOrdersAlgoPending\",\"ordType\":\"move_order_stop\"}",
    "fetchOpenOrders:1:{\"method\":\"privateGetTradeOrdersAlgoPending\",\"ordType\":\"iceberg\"}",
    "fetchOpenOrders:1:{\"method\":\"privateGetTradeOrdersAlgoPending\",\"ordType\":\"twap\"}",
    "close",
  ]);
});

test("ordinary open orders use OKX order-id cursors until the final page", async () => {
  const firstPage = Array.from({ length: 100 }, (_, index) => ({ id: `order-${index + 1}` })) as CcxtOrder[];
  const calls: unknown[] = [];
  const client = {
    fetchOpenOrders: async (_symbol: unknown, _since: unknown, limit: unknown, params: unknown) => {
      calls.push({ limit, params });
      return calls.length === 1 ? firstPage : [{ id: "order-101" }];
    },
  } as unknown as CcxtExchange;

  const orders = await fetchOrdinaryOpenOrders(client);
  assert.equal(orders.length, 101);
  assert.deepEqual(calls, [
    { limit: 100, params: {} },
    { limit: 100, params: { after: "order-100" } },
  ]);
});

test("algorithmic open orders fail closed because the ordinary order contract cannot represent them", async () => {
  const calls: unknown[] = [];
  const client = {
    fetchOpenOrders: async (_symbol: unknown, _since: unknown, limit: unknown, params: unknown) => {
      calls.push({ limit, params });
      return calls.length === 3 ? [{ id: "algo-1" }] : [];
    },
  } as unknown as CcxtExchange;
  await assert.rejects(rejectAlgoOpenOrders(client), /private_algo_open_orders_unsupported/);
  assert.deepEqual(calls, [
    { limit: 1, params: { method: "privateGetTradeOrdersAlgoPending", ordType: "conditional" } },
    { limit: 1, params: { method: "privateGetTradeOrdersAlgoPending", ordType: "oco" } },
    { limit: 1, params: { method: "privateGetTradeOrdersAlgoPending", ordType: "trigger" } },
  ]);
});

test("balance normalization preserves decimal strings and removes zero assets", () => {
  const balances = {
    USDT: { total: "123456789.123456789123456789", free: "123456780.000000000000000001", used: "9.123456789123456788" },
    BTC: { total: "0", free: "0", used: "0" },
    free: { USDT: "123456780.000000000000000001" },
    used: { USDT: "9.123456789123456788" },
    total: { USDT: "123456789.123456789123456789" },
    info: {},
  } as unknown as Balances;
  assert.deepEqual(normalizeBalances(balances), [{
    asset: "USDT",
    total: "123456789.123456789123456789",
    available: "123456780.000000000000000001",
  }]);
});

test("perpetual positions normalize contracts to signed base quantity", () => {
  const client = {
    market: () => ({ swap: true, linear: true, contractSize: "0.00000001" }),
  } as unknown as CcxtExchange;
  const position = {
    symbol: "BTC/USDT:USDT",
    side: "short",
    contracts: "123456789.123456789123456789",
    contractSize: "0.00000001",
    entryPrice: "60000.123456789123456789",
  } as unknown as CcxtPosition;
  assert.deepEqual(normalizePosition(client, position), {
    instrument: { exchange: 2, marketKind: 2, symbol: "BTC/USDT:USDT" },
    quantity: "-1.23456789123456789123456789",
    entryPrice: "60000.123456789123456789",
  });
});

test("open orders preserve decimal strings and require stable identifiers", () => {
  const client = {
    market: () => ({ spot: false, swap: true, linear: true }),
  } as unknown as CcxtExchange;
  const order = {
    id: "exchange-order-1",
    clientOrderId: "client-order-1",
    symbol: "BTC/USDT:USDT",
    side: "buy",
    amount: "123456789.123456789123456789",
    filled: "0.000000000000000001",
    average: "60000.123456789123456789",
    status: "open",
    info: { state: "partially_filled" },
    timestamp: 1_700_000_000_000,
  } as unknown as CcxtOrder;

  assert.deepEqual(normalizeOrder(account, client, order, 1_700_000_000_100), {
    account: { accountId: "okx-demo", exchange: 2, environment: 1 },
    instrument: { exchange: 2, marketKind: 2, symbol: "BTC/USDT:USDT" },
    clientOrderId: "client-order-1",
    exchangeOrderId: "exchange-order-1",
    state: 4,
    side: 1,
    quantity: "123456789.123456789123456789",
    filledQuantity: "0.000000000000000001",
    averagePrice: "60000.123456789123456789",
    exchangeTimeMs: "1700000000000",
    gatewayReceivedAtMs: "1700000000100",
  });

  assert.throws(
    () => normalizeOrder(account, client, { ...order, clientOrderId: undefined } as CcxtOrder, 1_700_000_000_100),
    /order.client_order_id_invalid/,
  );
});

test("order lookup uses the OKX client-order-id parameter and validates the instrument", async () => {
  const calls: unknown[] = [];
  const client = {
    has: { fetchOrder: true },
    setSandboxMode: () => undefined,
    loadMarkets: async () => { calls.push("loadMarkets"); },
    market: (symbol: string) => {
      calls.push({ market: symbol });
      return { spot: true };
    },
    fetchOrder: async (...args: unknown[]) => {
      calls.push({ fetchOrder: args });
      return {
        id: "exchange-order-1",
        clientOrderId: "clientOrder1",
        symbol: "BTC/USDT",
        side: "sell",
        amount: "2.500000000000000001",
        filled: "2.500000000000000001",
        average: "60000.123456789123456789",
        status: "closed",
        info: { state: "filled" },
        lastUpdateTimestamp: 1_700_000_000_001,
      } as unknown as CcxtOrder;
    },
  } as unknown as CcxtExchange;
  const adapter = new OkxAccountAdapter(account, 1_000, client);

  const order = await adapter.getOrderByClientId("BTC/USDT", 1, "clientOrder1");

  assert.equal(order.state, 5);
  assert.equal(order.quantity, "2.500000000000000001");
  assert.equal(order.filledQuantity, "2.500000000000000001");
  assert.equal(order.averagePrice, "60000.123456789123456789");
  assert.deepEqual(calls.slice(0, 3), [
    "loadMarkets",
    { market: "BTC/USDT" },
    { fetchOrder: ["clientOrder1", "BTC/USDT", { clientOrderId: "clientOrder1" }] },
  ]);
});

test("IOC submission sends one protected limit order with the stable client id", async () => {
  const calls: unknown[] = [];
  const client = {
    loadMarkets: async () => undefined,
    market: () => ({ spot: true }),
    setSandboxMode: () => undefined,
    createOrder: async (...args: unknown[]) => {
      calls.push(args);
      return {
        id: "exchange-order-1",
        clientOrderId: "clientOrder1",
        symbol: "BTC/USDT",
        side: "buy",
        amount: "1.000000000000000001",
        filled: "1.000000000000000001",
        average: "60000.123456789123456789",
        status: "closed",
        info: { state: "filled" },
        timestamp: 1_700_000_000_000,
      } as unknown as CcxtOrder;
    },
  } as unknown as CcxtExchange;
  const adapter = new OkxAccountAdapter(account, 1_000, client);

  const order = await adapter.placeIocOrder(
    "BTC/USDT",
    MarketKind.MARKET_KIND_SPOT,
    "buy",
    "1.000000000000000001",
    "60000.123456789123456789",
    "clientOrder1",
  );

  assert.equal(order.clientOrderId, "clientOrder1");
  assert.deepEqual(calls, [[
    "BTC/USDT",
    "limit",
    "buy",
    "1.000000000000000001",
    "60000.123456789123456789",
    { ordType: "ioc", clOrdId: "clientOrder1", tdMode: "cash" },
  ]]);
});

test("queried order states normalize without treating rejected orders as cancelled", () => {
  const client = { market: () => ({ spot: true }) } as unknown as CcxtExchange;
  const base = {
    id: "exchange-order-1",
    clientOrderId: "clientOrder1",
    symbol: "BTC/USDT",
    side: "buy",
    amount: "10",
    filled: "0",
    timestamp: 1_700_000_000_000,
  } as unknown as CcxtOrder;
  const states = [
    [{ status: "open", info: { state: "live" } }, 3],
    [{ status: "open", info: { state: "partially_filled" }, filled: "1" }, 4],
    [{ status: "closed", info: { state: "filled" }, filled: "10" }, 5],
    [{ status: "canceled", info: { state: "canceled" } }, 7],
    [{ status: "canceled", info: { state: "order_failed" } }, 8],
    [{ status: "expired", info: { state: "expired" } }, 9],
  ] as const;
  for (const [fields, expected] of states) {
    assert.equal(normalizeOrder(account, client, { ...base, ...fields } as CcxtOrder, 1_700_000_000_100).state, expected);
  }
  assert.throws(
    () => normalizeOrder(account, client, { ...base, status: "open", info: { state: "pending" } } as CcxtOrder, 1_700_000_000_100),
    /order.status_invalid/,
  );
});

test("orders from unsupported market kinds fail instead of being labeled perpetual", () => {
  const client = {
    market: () => ({ future: true, linear: true }),
  } as unknown as CcxtExchange;
  const order = {
    id: "exchange-order-1",
    clientOrderId: "client-order-1",
    symbol: "BTC/USDT:USDT-260925",
    side: "buy",
    amount: "1",
    filled: "0",
    timestamp: 1_700_000_000_000,
  } as unknown as CcxtOrder;

  assert.throws(
    () => normalizeOrder(account, client, order, 1_700_000_000_100),
    /private_order_market_kind_invalid/,
  );
});

test("inverse positions fail instead of using an ambiguous quantity unit", () => {
  const client = {
    market: () => ({ swap: true, inverse: true, contractSize: "100" }),
  } as unknown as CcxtExchange;
  const position = {
    symbol: "BTC/USD:BTC",
    side: "long",
    contracts: "2",
    contractSize: "100",
  } as unknown as CcxtPosition;
  assert.throws(() => normalizePosition(client, position), /private_position_market_kind_invalid/);
});
