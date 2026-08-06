import ccxt from "ccxt";
import type {
  Exchange as CcxtExchange,
  MarketInterface as CcxtMarket,
  OrderBook as CcxtOrderBook,
  Trade as CcxtTrade,
} from "ccxt";
import { Decimal } from "decimal.js";

import type {
  BookLevel,
  ContractKind,
  ExchangeCapabilities,
  ExchangeId,
  Instrument,
  InstrumentKey,
  MarketDataAdapter,
  MarketKind,
  OrderBook,
  Trade,
  TradeSide,
} from "./adapter.js";

type CcxtConstructor = new (config?: Record<string, unknown>) => CcxtExchange;

const CCXT_IDS: Readonly<Record<ExchangeId, string>> = {
  binance: "binance",
  okx: "okx",
  bybit: "bybit",
  gate_io: "gate",
  bitget: "bitget",
};

export class CcxtMarketDataAdapter implements MarketDataAdapter {
  readonly exchange: ExchangeId;
  readonly #rest: CcxtExchange;
  readonly #stream: CcxtExchange;

  constructor(exchange: ExchangeId, timeoutMs: number) {
    this.exchange = exchange;
    const ccxtId = CCXT_IDS[exchange];
    const Rest = ccxt[ccxtId as keyof typeof ccxt] as unknown as CcxtConstructor;
    const Pro = ccxt.pro[ccxtId as keyof typeof ccxt.pro] as unknown as CcxtConstructor;
    const config = { enableRateLimit: true, timeout: timeoutMs };
    this.#rest = new Rest(config);
    this.#stream = new Pro(config);
  }

  async listInstruments(reload: boolean): Promise<readonly Instrument[]> {
    const markets = await this.#rest.loadMarkets(reload);
    return Object.values(markets)
      .filter((market): market is CcxtMarket => market !== undefined)
      .filter((market) => market.spot || market.swap)
      .map((market) => normalizeInstrument(this.exchange, this.#rest, market));
  }

  async capabilities(): Promise<ExchangeCapabilities> {
    const instruments = await this.listInstruments(false);
    return {
      exchange: this.exchange,
      spot: instruments.some((instrument) => instrument.key.marketKind === "spot"),
      perpetualLinear: instruments.some(
        (instrument) => instrument.contractKind === "linear",
      ),
      perpetualInverse: instruments.some(
        (instrument) => instrument.contractKind === "inverse",
      ),
      fetchOrderBook: this.#rest.has.fetchOrderBook === true,
      watchOrderBook: this.#stream.has.watchOrderBook === true,
      watchTrades: this.#stream.has.watchTrades === true,
    };
  }

  async getOrderBook(instrument: InstrumentKey, depth: number): Promise<OrderBook> {
    await this.#rest.loadMarkets();
    const book = await this.#rest.fetchOrderBook(instrument.symbol, depth);
    return normalizeOrderBook(instrument, book, depth);
  }

  async watchOrderBook(instrument: InstrumentKey, depth: number): Promise<OrderBook> {
    await this.#stream.loadMarkets();
    const book = await this.#stream.watchOrderBook(instrument.symbol, depth);
    return normalizeOrderBook(instrument, book, depth);
  }

  async watchTrades(instrument: InstrumentKey): Promise<readonly Trade[]> {
    await this.#stream.loadMarkets();
    const trades = await this.#stream.watchTrades(instrument.symbol);
    return trades.map((trade, index) => normalizeTrade(this.exchange, instrument, trade, index));
  }

  async close(): Promise<void> {
    await Promise.allSettled([this.#rest.close(), this.#stream.close()]);
  }

}

export function normalizeInstrument(
  exchange: ExchangeId,
  client: CcxtExchange,
  market: CcxtMarket,
): Instrument {
  const marketKind: MarketKind = market.spot ? "spot" : "perpetual";
  const contractKind = normalizeContractKind(market);
  const priceTick = precisionToStep(client, market.precision.price, "price");
  const quantityStep = precisionToStep(client, market.precision.amount, "amount");
  return compactOptional({
    key: { exchange, marketKind, symbol: market.symbol },
    exchangeSymbol: requiredText(market.id, "market.id"),
    baseAsset: requiredText(market.base, "market.base"),
    quoteAsset: requiredText(market.quote, "market.quote"),
    settleAsset: optionalText(market.settle),
    contractKind,
    contractSize: optionalPositiveDecimal(market.contractSize),
    priceTick,
    quantityStep,
    minimumQuantity: optionalPositiveDecimal(market.limits.amount?.min),
    minimumNotional: optionalPositiveDecimal(market.limits.cost?.min),
    makerFeeRate: optionalDecimal(market.maker),
    takerFeeRate: optionalDecimal(market.taker),
    active: market.active !== false,
  }) as unknown as Instrument;
}

export function normalizeOrderBook(
  instrument: InstrumentKey,
  book: CcxtOrderBook,
  depth: number,
): OrderBook {
  const receivedAt = Date.now();
  return compactOptional({
    instrument,
    bids: book.bids.slice(0, depth).map(normalizeBookLevel),
    asks: book.asks.slice(0, depth).map(normalizeBookLevel),
    exchangeTimeMs: book.timestamp ?? receivedAt,
    gatewayReceivedAtMs: receivedAt,
    sequence: book.nonce === undefined ? undefined : String(book.nonce),
  }) as unknown as OrderBook;
}

export function normalizeTrade(
  exchange: ExchangeId,
  instrument: InstrumentKey,
  trade: CcxtTrade,
  index: number,
): Trade {
  const receivedAt = Date.now();
  const exchangeTradeId = requiredText(trade.id, "trade.id");
  return {
    eventId: `${exchange}:${exchangeTradeId}`,
    instrument,
    exchangeTradeId,
    price: requiredDecimal(trade.price, "trade.price"),
    quantity: requiredDecimal(trade.amount, "trade.amount"),
    takerSide: normalizeSide(trade.side),
    exchangeTimeMs: trade.timestamp ?? receivedAt + index,
    gatewayReceivedAtMs: receivedAt,
  };
}

function normalizeContractKind(market: CcxtMarket): ContractKind | undefined {
  if (market.linear) return "linear";
  if (market.inverse) return "inverse";
  return undefined;
}

function normalizeBookLevel(level: [number | undefined, number | undefined]): BookLevel {
  if (level.length < 2) throw new Error("order book level is missing price or quantity");
  return {
    price: requiredDecimal(level[0], "book.price"),
    quantity: requiredDecimal(level[1], "book.quantity"),
  };
}

function normalizeSide(side: string | undefined): TradeSide {
  if (side === "buy" || side === "sell") return side;
  return "unknown";
}

function precisionToStep(
  exchange: CcxtExchange,
  precision: number | undefined,
  field: string,
): string {
  if (precision === undefined || !Number.isFinite(precision)) {
    throw new Error(`market ${field} precision is unavailable`);
  }
  if (exchange.precisionMode === ccxt.DECIMAL_PLACES) {
    return new Decimal(10).pow(-precision).toFixed();
  }
  return requiredDecimal(precision, `market.${field}_precision`);
}

function requiredText(value: unknown, field: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${field} is unavailable`);
  }
  return value;
}

function optionalText(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function requiredDecimal(value: unknown, field: string): string {
  const normalized = optionalDecimal(value);
  if (normalized === undefined) throw new Error(`${field} is unavailable`);
  return normalized;
}

function optionalDecimal(value: unknown): string | undefined {
  if (value === undefined || value === null) return undefined;
  const decimal = new Decimal(value as Decimal.Value);
  if (!decimal.isFinite()) return undefined;
  return decimal.toFixed();
}

function optionalPositiveDecimal(value: unknown): string | undefined {
  const normalized = optionalDecimal(value);
  if (normalized === undefined || new Decimal(normalized).lte(0)) return undefined;
  return normalized;
}

function compactOptional<T extends Record<string, unknown>>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined),
  ) as T;
}
