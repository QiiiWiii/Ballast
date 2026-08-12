import ccxt from "ccxt";
import type { Logger } from "pino";
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
  HistoricalBatch,
  HistoricalCandle,
  HistoricalDataType,
  HistoricalTrade,
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
  readonly #logger: Logger;

  constructor(exchange: ExchangeId, timeoutMs: number, logger: Logger) {
    this.exchange = exchange;
    this.#logger = logger;
    const ccxtId = CCXT_IDS[exchange];
    const Rest = ccxt[ccxtId as keyof typeof ccxt] as unknown as CcxtConstructor;
    const Pro = ccxt.pro[ccxtId as keyof typeof ccxt.pro] as unknown as CcxtConstructor;
    const config = { enableRateLimit: true, timeout: timeoutMs };
    this.#rest = new Rest(config);
    this.#stream = new Pro(config);
  }

  async listInstruments(reload: boolean): Promise<readonly Instrument[]> {
    const markets = await this.#rest.loadMarkets(reload);
    const candidates = Object.values(markets)
      .filter((market): market is CcxtMarket => market !== undefined)
      .filter((market) => market.spot || market.swap);
    const { instruments, rejected } = normalizeInstruments(this.exchange, this.#rest, candidates);
    if (rejected.length > 0) {
      this.#logger.warn({
        exchange: this.exchange,
        rejectedCount: rejected.length,
        candidateCount: candidates.length,
        rejected: rejected.slice(0, 10),
      }, "exchange markets rejected by the instrument contract");
    }
    if (instruments.length === 0) {
      throw new Error(`no ${this.exchange} markets satisfy the instrument contract`);
    }
    return instruments;
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
      fetchOhlcv: this.#rest.has.fetchOHLCV === true,
      fetchTrades: this.#rest.has.fetchTrades === true,
    };
  }

  async getOrderBook(instrument: InstrumentKey, depth: number): Promise<OrderBook> {
    await this.#rest.loadMarkets();
    const book = await this.#rest.fetchOrderBook(instrument.symbol, depth);
    return normalizeOrderBook(instrument, book, depth);
  }

  async fetchHistoricalBatch(
    instrument: InstrumentKey,
    dataType: HistoricalDataType,
    timeframe: string | undefined,
    cursorMs: number,
    endMs: number,
    limit: number,
  ): Promise<HistoricalBatch> {
    await this.#rest.loadMarkets();
    if (dataType === "ohlcv") {
      if (this.#rest.has.fetchOHLCV !== true) throw new Error("fetch_ohlcv_not_supported");
      if (!timeframe) throw new Error("timeframe_required");
      const rows = await withReadRetry(() =>
        this.#rest.fetchOHLCV(instrument.symbol, timeframe, cursorMs, limit),
      );
      const candles = rows
        .map(normalizeHistoricalCandle)
        .filter((row) => row.openTimeMs >= cursorMs && row.openTimeMs < endMs)
        .sort((left, right) => left.openTimeMs - right.openTimeMs);
      const stepMs = this.#rest.parseTimeframe(timeframe) * 1_000;
      const nextCursorMs = nextHistoricalCursor(
        candles.map((row) => row.openTimeMs), cursorMs, endMs, stepMs,
      );
      return {
        candles,
        trades: [],
        nextCursorMs,
        exhausted: rows.length < limit || nextCursorMs >= endMs,
      };
    }
    if (this.#rest.has.fetchTrades !== true) throw new Error("fetch_trades_not_supported");
    const rows = await withReadRetry(() =>
      this.#rest.fetchTrades(instrument.symbol, cursorMs, limit),
    );
    const trades = rows
      .map(normalizeHistoricalTrade)
      .filter((row) => row.tradeTimeMs >= cursorMs && row.tradeTimeMs < endMs)
      .sort((left, right) => left.tradeTimeMs - right.tradeTimeMs || left.exchangeTradeId.localeCompare(right.exchangeTradeId));
    const nextCursorMs = nextHistoricalCursor(
      trades.map((row) => row.tradeTimeMs), cursorMs, endMs, 0,
    );
    return {
      candles: [],
      trades,
      nextCursorMs,
      exhausted: rows.length < limit || nextCursorMs >= endMs,
    };
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

export function normalizeInstruments(
  exchange: ExchangeId,
  client: CcxtExchange,
  markets: readonly CcxtMarket[],
): {
  readonly instruments: readonly Instrument[];
  readonly rejected: readonly { symbol: string; reason: string }[];
} {
  const instruments: Instrument[] = [];
  const rejected: Array<{ symbol: string; reason: string }> = [];
  for (const market of markets) {
    try {
      instruments.push(normalizeInstrument(exchange, client, market));
    } catch (error) {
      if (!isMarketContractError(error)) throw error;
      rejected.push({
        symbol: market.symbol,
        reason: error.message,
      });
    }
  }
  return { instruments, rejected };
}

function isMarketContractError(error: unknown): error is Error {
  return error instanceof Error && /^market(?:\.| )/.test(error.message);
}

export function normalizeHistoricalCandle(
  row: readonly (number | undefined)[],
): HistoricalCandle {
  if (row.length < 6) throw new Error("historical candle is incomplete");
  const openTimeMs = row[0];
  if (openTimeMs === undefined || !Number.isFinite(openTimeMs)) {
    throw new Error("candle.timestamp is unavailable");
  }
  return {
    openTimeMs,
    open: requiredDecimal(row[1], "candle.open"),
    high: requiredDecimal(row[2], "candle.high"),
    low: requiredDecimal(row[3], "candle.low"),
    close: requiredDecimal(row[4], "candle.close"),
    volume: nonNegativeDecimal(row[5], "candle.volume"),
  };
}

export function normalizeHistoricalTrade(trade: CcxtTrade): HistoricalTrade {
  return {
    exchangeTradeId: requiredText(trade.id, "trade.id"),
    tradeTimeMs: requiredTimestamp(trade.timestamp, "trade.timestamp"),
    price: requiredDecimal(trade.price, "trade.price"),
    quantity: requiredDecimal(trade.amount, "trade.amount"),
    takerSide: normalizeSide(trade.side),
  };
}

export function nextHistoricalCursor(
  timestamps: readonly number[],
  cursorMs: number,
  endMs: number,
  stepMs: number,
): number {
  if (timestamps.length === 0) return endMs;
  const next = timestamps[timestamps.length - 1]! + stepMs;
  return Math.min(endMs, Math.max(cursorMs, next));
}

export async function withReadRetry<T>(read: () => Promise<T>): Promise<T> {
  let lastError: unknown;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      return await read();
    } catch (error) {
      lastError = error;
      if (attempt < 2) await new Promise((resolve) => setTimeout(resolve, 250 * 2 ** attempt));
    }
  }
  throw lastError;
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

function nonNegativeDecimal(value: unknown, field: string): string {
  const normalized = requiredDecimal(value, field);
  if (new Decimal(normalized).lt(0)) throw new Error(`${field} is negative`);
  return normalized;
}

function requiredTimestamp(value: unknown, field: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`${field} is unavailable`);
  return value;
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
