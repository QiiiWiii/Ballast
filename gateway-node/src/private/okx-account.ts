import ccxt, { Precise } from "ccxt";
import type {
  Balances as CcxtBalances,
  Exchange as CcxtExchange,
  MarketInterface as CcxtMarket,
  Order as CcxtOrder,
  Position as CcxtPosition,
} from "ccxt";
import { Decimal } from "decimal.js";

import { MarketKind } from "../generated/ballast/gateway/v1/MarketKind.js";
import { OrderState } from "../generated/ballast/gateway/v1/OrderState.js";
import { Side } from "../generated/ballast/gateway/v1/Side.js";
import { Exchange } from "../generated/ballast/gateway/v1/Exchange.js";
import { AccountEnvironment } from "../generated/ballast/gateway/v1/AccountEnvironment.js";
import type { AccountRef } from "../generated/ballast/gateway/v1/AccountRef.js";
import type { AccountSnapshot } from "../generated/ballast/gateway/v1/AccountSnapshot.js";
import type { PrivateAccountConfig } from "./accounts.js";

type CcxtConstructor = new (config?: Record<string, unknown>) => CcxtExchange;
const OPEN_ORDER_PAGE_SIZE = 100;
const ALGO_ORDER_TYPES = [
  "conditional",
  "oco",
  "trigger",
  "move_order_stop",
  "iceberg",
  "twap",
] as const;

export class OkxAccountAdapter {
  readonly #account: PrivateAccountConfig;
  readonly #client: CcxtExchange;

  constructor(account: PrivateAccountConfig, timeoutMs: number, client?: CcxtExchange) {
    this.#account = account;
    if (client) {
      this.#client = client;
    } else {
      const Okx = ccxt.okx as unknown as CcxtConstructor;
      this.#client = new Okx({
        apiKey: account.apiKey,
        secret: account.secret,
        password: account.password,
        enableRateLimit: true,
        timeout: timeoutMs,
      });
    }
    (this.#client as unknown as { number: StringConstructor }).number = String;
    if (account.environment === "demo") this.#client.setSandboxMode(true);
  }

  async snapshot(): Promise<AccountSnapshot> {
    await this.#client.loadMarkets();
    if (this.#client.has.fetchPositions !== true) throw new Error("fetch_positions_not_supported");
    if (this.#client.has.fetchOpenOrders !== true) throw new Error("fetch_open_orders_not_supported");
    const [balances, positions, orders, exchangeTime] = await Promise.all([
      this.#client.fetchBalance(),
      this.#client.fetchPositions(),
      fetchOrdinaryOpenOrders(this.#client),
      this.#client.fetchTime(),
    ]);
    await rejectAlgoOpenOrders(this.#client);
    const receivedAt = Date.now();
    return {
      account: accountRef(this.#account),
      balances: normalizeBalances(balances),
      positions: positions
        .filter(hasOpenContracts)
        .map((position) => normalizePosition(this.#client, position)),
      openOrders: orders.map((order) => normalizeOrder(this.#account, this.#client, order, receivedAt)),
      exchangeTimeMs: latestExchangeTime(
        balances,
        positions,
        orders,
        requiredNumber(exchangeTime, "exchange.time"),
      ).toString(),
      gatewayReceivedAtMs: receivedAt.toString(),
    };
  }

  async close(): Promise<void> {
    await this.#client.close();
  }
}

export async function fetchOrdinaryOpenOrders(client: CcxtExchange): Promise<CcxtOrder[]> {
  const orders: CcxtOrder[] = [];
  const cursors = new Set<string>();
  let after: string | undefined;
  while (true) {
    const page = await client.fetchOpenOrders(
      undefined,
      undefined,
      OPEN_ORDER_PAGE_SIZE,
      after === undefined ? {} : { after },
    );
    orders.push(...page);
    if (page.length < OPEN_ORDER_PAGE_SIZE) return orders;
    const next = requiredText(page.at(-1)?.id, "order.exchange_order_id");
    if (cursors.has(next)) throw new Error("open_order_pagination_stalled");
    cursors.add(next);
    after = next;
  }
}

export async function rejectAlgoOpenOrders(client: CcxtExchange): Promise<void> {
  for (const ordType of ALGO_ORDER_TYPES) {
    const orders = await client.fetchOpenOrders(undefined, undefined, 1, {
      method: "privateGetTradeOrdersAlgoPending",
      ordType,
    });
    if (orders.length > 0) throw new Error("private_algo_open_orders_unsupported");
  }
}

export function normalizeBalances(balances: CcxtBalances): NonNullable<AccountSnapshot["balances"]> {
  return Object.entries(balances)
    .filter(([asset, value]) => !["info", "timestamp", "datetime", "free", "used", "total"].includes(asset) && isRecord(value))
    .map(([asset, value]) => ({
      asset,
      total: decimalText(value.total, `balance.${asset}.total`),
      available: decimalText(value.free, `balance.${asset}.free`),
    }))
    .filter((balance) => !new Decimal(balance.total).isZero() || !new Decimal(balance.available).isZero())
    .sort((left, right) => left.asset.localeCompare(right.asset));
}

export function normalizePosition(client: CcxtExchange, position: CcxtPosition): NonNullable<AccountSnapshot["positions"]>[number] {
  const market = requiredMarket(client, position.symbol);
  if (!market.swap || market.linear !== true) throw new Error("private_position_market_kind_invalid");
  const contracts = decimalText(position.contracts, "position.contracts");
  if (new Decimal(contracts).isNegative()) throw new Error("position.contracts_invalid");
  const contractSize = decimalText(position.contractSize ?? market.contractSize, "position.contract_size");
  const unsignedQuantity = Precise.stringMul(contracts, contractSize);
  if (position.side !== "long" && position.side !== "short") {
    throw new Error("position.side_invalid");
  }
  const quantity = position.side === "short" ? Precise.stringNeg(unsignedQuantity) : unsignedQuantity;
  return compactOptional({
    instrument: { exchange: Exchange.EXCHANGE_OKX, marketKind: MarketKind.MARKET_KIND_PERPETUAL, symbol: position.symbol },
    quantity,
    entryPrice: optionalDecimal(position.entryPrice),
    markPrice: optionalDecimal(position.markPrice),
    unrealizedPnl: optionalDecimal(position.unrealizedPnl),
  }) as NonNullable<AccountSnapshot["positions"]>[number];
}

export function normalizeOrder(
  account: PrivateAccountConfig,
  client: CcxtExchange,
  order: CcxtOrder,
  receivedAt: number,
): NonNullable<AccountSnapshot["openOrders"]>[number] {
  const market = requiredMarket(client, order.symbol);
  const marketKind = orderMarketKind(market);
  const filled = new Decimal(decimalText(order.filled, "order.filled"));
  if (order.side !== "buy" && order.side !== "sell") throw new Error("order.side_invalid");
  return compactOptional({
    account: accountRef(account),
    instrument: {
      exchange: Exchange.EXCHANGE_OKX,
      marketKind,
      symbol: order.symbol,
    },
    clientOrderId: requiredText(order.clientOrderId, "order.client_order_id"),
    exchangeOrderId: requiredText(order.id, "order.exchange_order_id"),
    state: filled.isZero() ? OrderState.ORDER_STATE_OPEN : OrderState.ORDER_STATE_PARTIALLY_FILLED,
    side: order.side === "buy" ? Side.SIDE_BUY : Side.SIDE_SELL,
    quantity: decimalText(order.amount, "order.amount"),
    filledQuantity: filled.toFixed(),
    averagePrice: optionalDecimal(order.average),
    exchangeTimeMs: requiredTimestamp(order.lastUpdateTimestamp ?? order.timestamp, "order.timestamp").toString(),
    gatewayReceivedAtMs: receivedAt.toString(),
  }) as NonNullable<AccountSnapshot["openOrders"]>[number];
}

function accountRef(account: PrivateAccountConfig): AccountRef {
  return {
    accountId: account.accountId,
    exchange: Exchange.EXCHANGE_OKX,
    environment: account.environment === "demo"
      ? AccountEnvironment.ACCOUNT_ENVIRONMENT_DEMO
      : AccountEnvironment.ACCOUNT_ENVIRONMENT_PRODUCTION,
  };
}

function latestExchangeTime(
  balances: CcxtBalances,
  positions: readonly CcxtPosition[],
  orders: readonly CcxtOrder[],
  exchangeTime: number,
): number {
  const observed = Math.max(
    exchangeTime,
    typeof balances.timestamp === "number" ? balances.timestamp : 0,
    ...positions.map((position) => position.timestamp ?? 0),
    ...orders.map((order) => order.lastUpdateTimestamp ?? order.timestamp ?? 0),
  );
  if (!Number.isSafeInteger(observed) || observed <= 0) throw new Error("exchange_time_unavailable");
  return observed;
}

function hasOpenContracts(position: CcxtPosition): boolean {
  const contracts = new Decimal(decimalText(position.contracts, "position.contracts"));
  if (contracts.isNegative()) throw new Error("position.contracts_invalid");
  return contracts.gt(0);
}

function requiredMarket(client: CcxtExchange, symbol: string): CcxtMarket {
  const market = client.market(symbol);
  if (!market) throw new Error("private_market_not_loaded");
  return market;
}

function decimalText(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim() === "") throw new Error(`${field}_invalid`);
  try {
    const parsed = new Decimal(value);
    if (!parsed.isFinite()) throw new Error();
    return parsed.toFixed();
  } catch {
    throw new Error(`${field}_invalid`);
  }
}

function optionalDecimal(value: unknown): string | undefined {
  if (value === null || value === undefined) return undefined;
  return decimalText(value, "optional_decimal");
}

function requiredNumber(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`${field}_invalid`);
  return value;
}

function requiredTimestamp(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) throw new Error(`${field}_invalid`);
  return value;
}

function requiredText(value: unknown, field: string): string {
  if (typeof value !== "string" || value.trim() === "") throw new Error(`${field}_invalid`);
  return value;
}

function orderMarketKind(market: CcxtMarket): MarketKind {
  if (market.spot === true) return MarketKind.MARKET_KIND_SPOT;
  if (market.swap === true && market.linear === true) return MarketKind.MARKET_KIND_PERPETUAL;
  throw new Error("private_order_market_kind_invalid");
}


function compactOptional<T extends Record<string, unknown>>(value: T): T {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== undefined)) as T;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
