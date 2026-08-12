import * as grpc from "@grpc/grpc-js";
import gatewayPackage from "../../package.json" with { type: "json" };
import type { Logger } from "pino";

import type { GatewayConfig } from "../config.js";
import type {
  ContractKind,
  ExchangeCapabilities as DomainCapabilities,
  ExchangeId,
  HistoricalDataType,
  Instrument as DomainInstrument,
  InstrumentKey,
  MarketKind,
  OrderBook as DomainOrderBook,
  Trade as DomainTrade,
} from "../exchanges/adapter.js";
import { AdapterRegistry } from "../exchanges/registry.js";
import { ContractKind as ProtoContractKind } from "../generated/ballast/gateway/v1/ContractKind.js";
import type { ContractKind as ProtoContractKindValue } from "../generated/ballast/gateway/v1/ContractKind.js";
import { Exchange as ProtoExchange } from "../generated/ballast/gateway/v1/Exchange.js";
import type { Exchange as ProtoExchangeValue } from "../generated/ballast/gateway/v1/Exchange.js";
import type { AdapterHealth } from "../generated/ballast/gateway/v1/AdapterHealth.js";
import type { ExchangeCapabilities } from "../generated/ballast/gateway/v1/ExchangeCapabilities.js";
import type { GetCapabilitiesRequest__Output } from "../generated/ballast/gateway/v1/GetCapabilitiesRequest.js";
import type { GetOrderBookRequest__Output } from "../generated/ballast/gateway/v1/GetOrderBookRequest.js";
import type { FetchHistoricalBatchRequest__Output } from "../generated/ballast/gateway/v1/FetchHistoricalBatchRequest.js";
import type { FetchHistoricalBatchResponse } from "../generated/ballast/gateway/v1/FetchHistoricalBatchResponse.js";
import { HistoricalDataType as ProtoHistoricalDataType } from "../generated/ballast/gateway/v1/HistoricalDataType.js";
import type { HealthResponse } from "../generated/ballast/gateway/v1/HealthResponse.js";
import type { Instrument } from "../generated/ballast/gateway/v1/Instrument.js";
import type { InstrumentKey as ProtoInstrumentKey } from "../generated/ballast/gateway/v1/InstrumentKey.js";
import type { ListInstrumentsRequest__Output } from "../generated/ballast/gateway/v1/ListInstrumentsRequest.js";
import type { ListInstrumentsResponse } from "../generated/ballast/gateway/v1/ListInstrumentsResponse.js";
import type { MarketDataServiceHandlers } from "../generated/ballast/gateway/v1/MarketDataService.js";
import { MarketKind as ProtoMarketKind } from "../generated/ballast/gateway/v1/MarketKind.js";
import type { MarketKind as ProtoMarketKindValue } from "../generated/ballast/gateway/v1/MarketKind.js";
import type { OrderBook } from "../generated/ballast/gateway/v1/OrderBook.js";
import type { OrderBookStreamEvent } from "../generated/ballast/gateway/v1/OrderBookStreamEvent.js";
import { Side as ProtoSide } from "../generated/ballast/gateway/v1/Side.js";
import { StreamState } from "../generated/ballast/gateway/v1/StreamState.js";
import type { Trade } from "../generated/ballast/gateway/v1/Trade.js";
import type { TradeStreamEvent } from "../generated/ballast/gateway/v1/TradeStreamEvent.js";
import type { WatchMarketRequest__Output } from "../generated/ballast/gateway/v1/WatchMarketRequest.js";
import type { AccountServiceHandlers } from "../generated/ballast/gateway/v1/AccountService.js";
import type { AlgorithmicTradingServiceHandlers } from "../generated/ballast/gateway/v1/AlgorithmicTradingService.js";
import type { AlgoCapabilities } from "../generated/ballast/gateway/v1/AlgoCapabilities.js";
import type { GetAlgoCapabilitiesRequest__Output } from "../generated/ballast/gateway/v1/GetAlgoCapabilitiesRequest.js";
import type { GetTradingCapabilitiesRequest__Output } from "../generated/ballast/gateway/v1/GetTradingCapabilitiesRequest.js";
import type { TradingCapabilities } from "../generated/ballast/gateway/v1/TradingCapabilities.js";
import type { TradingServiceHandlers } from "../generated/ballast/gateway/v1/TradingService.js";
import {
  accountService,
  algorithmicTradingService,
  marketDataService,
  tradingService,
} from "./proto.js";
import { PrivateAccountRegistry } from "../private/accounts.js";
import { PrivateAccountService } from "../private/service.js";

export interface GatewayRuntime {
  readonly server: grpc.Server;
  closeAdapters(): Promise<void>;
}

export async function startGrpcServer(
  config: GatewayConfig,
  logger: Logger,
): Promise<GatewayRuntime> {
  const registry = new AdapterRegistry(config.exchangeTimeoutMs, logger);
  const privateAccounts = PrivateAccountRegistry.fromFile(config.okxAccountsFile);
  const privateAccountService = new PrivateAccountService(privateAccounts, config.exchangeTimeoutMs, logger);
  const server = new grpc.Server();
  const handlers: MarketDataServiceHandlers = {
    Health: (_call, callback) => callback(null, healthResponse(registry)),
    ListInstruments: asyncUnary(async (request) => {
      const exchanges = request.exchange === ProtoExchange.EXCHANGE_UNSPECIFIED
        ? allExchangeIds()
        : [exchangeFromProto(request.exchange)];
      const groups = await Promise.all(
        exchanges.map(async (exchange) => {
          const adapter = registry.get(exchange);
          try {
            const instruments = await adapter.listInstruments(request.reload ?? false);
            registry.markSuccess(exchange);
            return instruments;
          } catch (error) {
            registry.markFailure(exchange, error);
            throw error;
          }
        }),
      );
      const instruments = groups
        .flat()
        .filter((instrument) => matchesFilters(instrument, request))
        .map(toProtoInstrument);
      return { instruments, observedAtMs: Date.now().toString() } satisfies ListInstrumentsResponse;
    }, logger),
    GetCapabilities: asyncUnary(async (request) => {
      const exchange = exchangeFromProto(request.exchange);
      const adapter = registry.get(exchange);
      try {
        const capabilities = await adapter.capabilities();
        registry.markSuccess(exchange);
        return toProtoCapabilities(capabilities);
      } catch (error) {
        registry.markFailure(exchange, error);
        throw error;
      }
    }, logger),
    GetOrderBook: asyncUnary(async (request) => {
      const instrument = instrumentFromRequest(request);
      const depth = normalizeDepth(request.depth);
      const adapter = registry.get(instrument.exchange);
      try {
        const book = await adapter.getOrderBook(instrument, depth);
        registry.markSuccess(instrument.exchange);
        return toProtoOrderBook(book);
      } catch (error) {
        registry.markFailure(instrument.exchange, error);
        throw error;
      }
    }, logger),
    FetchHistoricalBatch: asyncUnary(async (request) => {
      const instrument = instrumentFromHistoricalRequest(request);
      const adapter = registry.get(instrument.exchange);
      try {
        const batch = await adapter.fetchHistoricalBatch(
          instrument,
          historicalDataTypeFromProto(request.dataType ?? ProtoHistoricalDataType.HISTORICAL_DATA_TYPE_UNSPECIFIED),
          request.timeframe,
          Number(request.cursorMs),
          Number(request.endMs),
          normalizeHistoricalLimit(request.limit ?? 0),
        );
        registry.markSuccess(instrument.exchange);
        return {
          candles: batch.candles.map((row) => ({
            openTimeMs: row.openTimeMs.toString(), open: row.open, high: row.high,
            low: row.low, close: row.close, volume: row.volume,
          })),
          trades: batch.trades.map((row) => ({
            exchangeTradeId: row.exchangeTradeId, tradeTimeMs: row.tradeTimeMs.toString(),
            price: row.price, quantity: row.quantity, takerSide: row.takerSide,
          })),
          nextCursorMs: batch.nextCursorMs.toString(),
          exhausted: batch.exhausted,
          observedAtMs: Date.now().toString(),
        } satisfies FetchHistoricalBatchResponse;
      } catch (error) {
        registry.markFailure(instrument.exchange, error);
        throw error;
      }
    }, logger),
    WatchOrderBook: (call) => {
      void streamOrderBooks(call, registry, config, logger);
    },
    WatchTrades: (call) => {
      void streamTrades(call, registry, config, logger);
    },
  };

  const accountHandlers: AccountServiceHandlers = {
    GetAccountSnapshot: asyncUnary((request) => privateAccountService.getSnapshot(request), logger),
    WatchAccountEvents: disabledStream,
  };
  const tradingHandlers = createTradingHandlers(privateAccountService, logger);
  const algorithmicTradingHandlers: AlgorithmicTradingServiceHandlers = {
    GetAlgoCapabilities: asyncUnary(async (request) => algoCapabilities(request), logger),
    SubmitAlgoOrder: disabledUnary,
    GetAlgoOrder: disabledUnary,
    CancelAlgoOrder: disabledUnary,
    ListAlgoSubOrders: disabledUnary,
    WatchAlgoEvents: disabledStream,
  };

  server.addService(marketDataService.service, handlers);
  server.addService(accountService.service, accountHandlers);
  server.addService(tradingService.service, tradingHandlers);
  server.addService(algorithmicTradingService.service, algorithmicTradingHandlers);
  await bind(server, config.bind);
  return {
    server,
    closeAdapters: async () => {
      await Promise.all([registry.close(), privateAccountService.close()]);
    },
  };
}

export function createTradingHandlers(
  privateAccountService: Pick<PrivateAccountService, "getOrderByClientId">,
  logger: Logger,
): TradingServiceHandlers {
  return {
    GetTradingCapabilities: asyncUnary(async (request) => tradingCapabilities(request), logger),
    PlaceIocOrder: disabledUnary,
    GetOrderByClientId: asyncUnary((request) => privateAccountService.getOrderByClientId(request), logger),
    CancelOrder: disabledUnary,
    WatchOrderEvents: disabledStream,
  };
}

function instrumentFromHistoricalRequest(request: FetchHistoricalBatchRequest__Output): InstrumentKey {
  if (Number(request.cursorMs) < 0 || Number(request.endMs) <= Number(request.cursorMs)) {
    throw new Error("invalid historical time range");
  }
  if (request.instrument === null || request.instrument === undefined) {
    throw new Error("instrument is required");
  }
  return {
    exchange: exchangeFromProto(request.instrument.exchange),
    marketKind: marketKindFromProto(request.instrument.marketKind),
    symbol: requiredRequestText(request.instrument.symbol, "instrument.symbol"),
  };
}

function requiredRequestText(value: string | null | undefined, field: string): string {
  if (value === null || value === undefined || value.trim().length === 0) {
    throw new Error(`${field} is required`);
  }
  return value;
}

function historicalDataTypeFromProto(value: number): HistoricalDataType {
  if (value === ProtoHistoricalDataType.HISTORICAL_DATA_TYPE_OHLCV) return "ohlcv";
  if (value === ProtoHistoricalDataType.HISTORICAL_DATA_TYPE_TRADES) return "trades";
  throw new Error("historical data type is required");
}

function normalizeHistoricalLimit(value: number): number {
  if (!Number.isInteger(value) || value < 1 || value > 1_000) {
    throw new Error("historical limit must be between 1 and 1000");
  }
  return value;
}

export function tradingCapabilities(
  request: GetTradingCapabilitiesRequest__Output,
): TradingCapabilities {
  const exchange = exchangeFromProto(request.exchange);
  return {
    exchange: exchangeToProto(exchange),
    placeIoc: false,
    queryByClientOrderId: exchange === "okx",
    cancelOrder: false,
    privateOrderStream: false,
    privateFillStream: false,
  };
}

export function algoCapabilities(request: GetAlgoCapabilitiesRequest__Output): AlgoCapabilities {
  const exchange = exchangeFromProto(request.exchange);
  const researched = {
    binance: [
      capability("binance_spot_twap", ProtoMarketKind.MARKET_KIND_SPOT, "documented_not_validated"),
      capability("binance_usdm_twap", ProtoMarketKind.MARKET_KIND_PERPETUAL, "documented_not_validated"),
      capability("binance_usdm_vp", ProtoMarketKind.MARKET_KIND_PERPETUAL, "documented_not_validated"),
    ],
    okx: [
      capability("okx_twap", ProtoMarketKind.MARKET_KIND_SPOT, "documented_not_validated"),
      capability("okx_twap", ProtoMarketKind.MARKET_KIND_PERPETUAL, "documented_not_validated"),
    ],
    bybit: [capability("native_algo", ProtoMarketKind.MARKET_KIND_UNSPECIFIED, "research_required", "capability_not_validated")],
    gate_io: [capability("native_algo", ProtoMarketKind.MARKET_KIND_UNSPECIFIED, "research_required", "capability_not_validated")],
    bitget: [capability("native_algo", ProtoMarketKind.MARKET_KIND_UNSPECIFIED, "research_required", "capability_not_validated")],
  } satisfies Record<ExchangeId, AlgoCapabilities["algorithms"]>;
  return { exchange: exchangeToProto(exchange), algorithms: researched[exchange] };
}

function capability(
  algorithm: string,
  marketKind: ProtoMarketKindValue,
  validationStatus: string,
  reasonCode?: string,
): NonNullable<AlgoCapabilities["algorithms"]>[number] {
  return {
    algorithm,
    marketKind,
    submit: false,
    query: false,
    cancel: false,
    listSubOrders: false,
    fillReconciliation: false,
    protectedPrice: false,
    validationStatus,
    ...(reasonCode === undefined ? {} : { reasonCode }),
  };
}

const disabledUnary: grpc.handleUnaryCall<unknown, never> = (_call, callback) => {
  callback(privateServicesDisabled());
};

const disabledStream: grpc.handleServerStreamingCall<unknown, never> = (call) => {
  call.destroy(privateServicesDisabled());
};

function privateServicesDisabled(): grpc.ServiceError {
  const details = "private_services_disabled";
  return Object.assign(new Error(details), {
    name: "FailedPrecondition",
    code: grpc.status.FAILED_PRECONDITION,
    details,
    metadata: new grpc.Metadata(),
  });
}

async function streamOrderBooks(
  call: grpc.ServerWritableStream<WatchMarketRequest__Output, OrderBookStreamEvent>,
  registry: AdapterRegistry,
  config: GatewayConfig,
  logger: Logger,
): Promise<void> {
  let instrument: InstrumentKey;
  try {
    instrument = instrumentFromWatchRequest(call.request);
  } catch (error) {
    call.destroy(toServiceError(error));
    return;
  }
  const adapter = registry.get(instrument.exchange);
  const depth = normalizeDepth(call.request.depth);
  await runStream(
    call,
    instrument.exchange,
    registry,
    config,
    logger,
    () => adapter.watchOrderBook(instrument, depth),
    (book) => ({ orderBook: toProtoOrderBook(book), payload: "orderBook" as const }),
  );
}

async function streamTrades(
  call: grpc.ServerWritableStream<WatchMarketRequest__Output, TradeStreamEvent>,
  registry: AdapterRegistry,
  config: GatewayConfig,
  logger: Logger,
): Promise<void> {
  let instrument: InstrumentKey;
  try {
    instrument = instrumentFromWatchRequest(call.request);
  } catch (error) {
    call.destroy(toServiceError(error));
    return;
  }
  const adapter = registry.get(instrument.exchange);
  await runStream(
    call,
    instrument.exchange,
    registry,
    config,
    logger,
    () => adapter.watchTrades(instrument),
    (trades) => trades.map((trade) => ({
      trade: toProtoTrade(trade),
      payload: "trade" as const,
    })),
  );
}

async function runStream<TDomain, TEvent>(
  call: grpc.ServerWritableStream<WatchMarketRequest__Output, TEvent>,
  exchange: ExchangeId,
  registry: AdapterRegistry,
  config: GatewayConfig,
  logger: Logger,
  read: () => Promise<TDomain>,
  events: (value: TDomain) => TEvent | readonly TEvent[],
): Promise<void> {
  let cancelled = false;
  let reconnectAttempt = 0;
  let lastUpdateAt = Date.now();
  let staleEmitted = false;
  call.on("cancelled", () => { cancelled = true; });
  call.on("close", () => { cancelled = true; });
  writeStatus(call, StreamState.STREAM_STATE_CONNECTING, reconnectAttempt);

  const staleTimer = setInterval(() => {
    if (!cancelled && !staleEmitted && Date.now() - lastUpdateAt >= config.streamStaleAfterMs) {
      staleEmitted = true;
      writeStatus(call, StreamState.STREAM_STATE_STALE, reconnectAttempt);
    }
  }, Math.min(config.streamStaleAfterMs, 5_000));

  try {
    while (!cancelled) {
      try {
        const value = await read();
        if (cancelled) break;
        registry.markSuccess(exchange);
        lastUpdateAt = Date.now();
        staleEmitted = false;
        if (reconnectAttempt > 0) writeStatus(call, StreamState.STREAM_STATE_CONNECTED, 0);
        reconnectAttempt = 0;
        const normalized = events(value);
        for (const event of Array.isArray(normalized) ? normalized : [normalized]) {
          call.write(event);
        }
      } catch (error) {
        reconnectAttempt += 1;
        const code = registry.markFailure(exchange, error);
        writeStatus(call, StreamState.STREAM_STATE_RECONNECTING, reconnectAttempt, code);
        await delay(Math.min(10_000, 500 * 2 ** Math.min(reconnectAttempt - 1, 5)));
      }
    }
  } catch (error) {
    logger.error({ exchange, error }, "market stream stopped unexpectedly");
    if (!cancelled) call.destroy(toServiceError(error));
  } finally {
    clearInterval(staleTimer);
    if (!cancelled) {
      writeStatus(call, StreamState.STREAM_STATE_CLOSED, reconnectAttempt);
      call.end();
    }
  }
}

export function healthResponse(registry: AdapterRegistry): HealthResponse {
  const adapters = registry.statuses().map((status) => compactOptional({
    exchange: exchangeToProto(status.exchange),
    status: status.status,
    lastErrorCode: status.lastErrorCode,
    lastSuccessAtMs: status.lastSuccessAtMs?.toString(),
  }) as unknown as AdapterHealth);
  const status = adapters.some((adapter) => adapter.status === "degraded") ? "degraded" : "ok";
  return {
    status,
    serviceVersion: gatewayPackage.version,
    gatewayTimeMs: Date.now().toString(),
    adapters,
  };
}

function matchesFilters(
  instrument: DomainInstrument,
  request: ListInstrumentsRequest__Output,
): boolean {
  if (request.activeOnly && !instrument.active) return false;
  if (
    request.marketKind !== ProtoMarketKind.MARKET_KIND_UNSPECIFIED
    && instrument.key.marketKind !== marketKindFromProto(request.marketKind)
  ) return false;
  if (
    request.contractKind !== ProtoContractKind.CONTRACT_KIND_UNSPECIFIED
    && instrument.contractKind !== contractKindFromProto(request.contractKind)
  ) return false;
  return true;
}

function toProtoInstrument(instrument: DomainInstrument): Instrument {
  return compactOptional({
    key: toProtoInstrumentKey(instrument.key),
    exchangeSymbol: instrument.exchangeSymbol,
    baseAsset: instrument.baseAsset,
    quoteAsset: instrument.quoteAsset,
    settleAsset: instrument.settleAsset,
    contractKind: contractKindToProto(instrument.contractKind),
    contractSize: instrument.contractSize,
    priceTick: instrument.priceTick,
    quantityStep: instrument.quantityStep,
    minimumQuantity: instrument.minimumQuantity,
    minimumNotional: instrument.minimumNotional,
    makerFeeRate: instrument.makerFeeRate,
    takerFeeRate: instrument.takerFeeRate,
    active: instrument.active,
  }) as unknown as Instrument;
}

function toProtoCapabilities(value: DomainCapabilities): ExchangeCapabilities {
  return {
    exchange: exchangeToProto(value.exchange),
    spot: value.spot,
    perpetualLinear: value.perpetualLinear,
    perpetualInverse: value.perpetualInverse,
    fetchOrderBook: value.fetchOrderBook,
    watchOrderBook: value.watchOrderBook,
    watchTrades: value.watchTrades,
    fetchOhlcv: value.fetchOhlcv,
    fetchTrades: value.fetchTrades,
  };
}

function toProtoOrderBook(book: DomainOrderBook): OrderBook {
  return compactOptional({
    instrument: toProtoInstrumentKey(book.instrument),
    bids: book.bids.map((level) => ({ price: level.price, quantity: level.quantity })),
    asks: book.asks.map((level) => ({ price: level.price, quantity: level.quantity })),
    exchangeTimeMs: book.exchangeTimeMs.toString(),
    gatewayReceivedAtMs: book.gatewayReceivedAtMs.toString(),
    sequence: book.sequence,
  }) as unknown as OrderBook;
}

function toProtoTrade(trade: DomainTrade): Trade {
  return {
    eventId: trade.eventId,
    instrument: toProtoInstrumentKey(trade.instrument),
    exchangeTradeId: trade.exchangeTradeId,
    price: trade.price,
    quantity: trade.quantity,
    takerSide: trade.takerSide === "buy"
      ? ProtoSide.SIDE_BUY
      : trade.takerSide === "sell"
        ? ProtoSide.SIDE_SELL
        : ProtoSide.SIDE_UNSPECIFIED,
    exchangeTimeMs: trade.exchangeTimeMs.toString(),
    gatewayReceivedAtMs: trade.gatewayReceivedAtMs.toString(),
  };
}

function toProtoInstrumentKey(key: InstrumentKey): ProtoInstrumentKey {
  return {
    exchange: exchangeToProto(key.exchange),
    marketKind: marketKindToProto(key.marketKind),
    symbol: key.symbol,
  };
}

function instrumentFromRequest(request: GetOrderBookRequest__Output): InstrumentKey {
  if (!request.instrument) throw invalidArgument("instrument is required");
  return instrumentFromProto(request.instrument);
}

function instrumentFromWatchRequest(request: WatchMarketRequest__Output): InstrumentKey {
  if (!request.instrument) throw invalidArgument("instrument is required");
  return instrumentFromProto(request.instrument);
}

function instrumentFromProto(value: { exchange?: number; marketKind?: number; symbol?: string }): InstrumentKey {
  if (!value.symbol) throw invalidArgument("instrument.symbol is required");
  return {
    exchange: exchangeFromProto(value.exchange),
    marketKind: marketKindFromProto(value.marketKind),
    symbol: value.symbol,
  };
}

function exchangeFromProto(value: number | undefined): ExchangeId {
  switch (value) {
    case ProtoExchange.EXCHANGE_BINANCE: return "binance";
    case ProtoExchange.EXCHANGE_OKX: return "okx";
    case ProtoExchange.EXCHANGE_BYBIT: return "bybit";
    case ProtoExchange.EXCHANGE_GATE_IO: return "gate_io";
    case ProtoExchange.EXCHANGE_BITGET: return "bitget";
    default: throw invalidArgument("a supported exchange is required");
  }
}

function exchangeToProto(value: ExchangeId): ProtoExchangeValue {
  switch (value) {
    case "binance": return ProtoExchange.EXCHANGE_BINANCE;
    case "okx": return ProtoExchange.EXCHANGE_OKX;
    case "bybit": return ProtoExchange.EXCHANGE_BYBIT;
    case "gate_io": return ProtoExchange.EXCHANGE_GATE_IO;
    case "bitget": return ProtoExchange.EXCHANGE_BITGET;
  }
}

function allExchangeIds(): readonly ExchangeId[] {
  return ["binance", "okx", "bybit", "gate_io", "bitget"];
}

function marketKindFromProto(value: number | undefined): MarketKind {
  switch (value) {
    case ProtoMarketKind.MARKET_KIND_SPOT: return "spot";
    case ProtoMarketKind.MARKET_KIND_PERPETUAL: return "perpetual";
    default: throw invalidArgument("a supported market kind is required");
  }
}

function marketKindToProto(value: MarketKind): ProtoMarketKindValue {
  return value === "spot"
    ? ProtoMarketKind.MARKET_KIND_SPOT
    : ProtoMarketKind.MARKET_KIND_PERPETUAL;
}

function contractKindFromProto(value: number | undefined): ContractKind {
  switch (value) {
    case ProtoContractKind.CONTRACT_KIND_LINEAR: return "linear";
    case ProtoContractKind.CONTRACT_KIND_INVERSE: return "inverse";
    default: throw invalidArgument("a supported contract kind is required");
  }
}

function contractKindToProto(value: ContractKind | undefined): ProtoContractKindValue {
  if (value === "linear") return ProtoContractKind.CONTRACT_KIND_LINEAR;
  if (value === "inverse") return ProtoContractKind.CONTRACT_KIND_INVERSE;
  return ProtoContractKind.CONTRACT_KIND_UNSPECIFIED;
}

function normalizeDepth(value: number | undefined): number {
  const depth = value && value > 0 ? value : 20;
  if (!Number.isInteger(depth) || depth > 500) throw invalidArgument("depth must be between 1 and 500");
  return depth;
}

function asyncUnary<TRequest, TResponse>(
  operation: (request: TRequest) => Promise<TResponse>,
  logger: Logger,
): (call: grpc.ServerUnaryCall<TRequest, TResponse>, callback: grpc.sendUnaryData<TResponse>) => void {
  return (call, send) => {
    void operation(call.request).then(
      (value) => send(null, value),
      (error) => {
        const serviceError = toServiceError(error);
        logger.warn(
          { code: serviceError.code, details: serviceError.details },
          "gRPC request failed",
        );
        send(serviceError);
      },
    );
  };
}

function writeStatus<T>(
  call: grpc.ServerWritableStream<WatchMarketRequest__Output, T>,
  state: number,
  reconnectAttempt: number,
  errorCode?: string,
): void {
  call.write(compactOptional({
    status: compactOptional({
      state,
      gatewayTimeMs: Date.now().toString(),
      reconnectAttempt,
      errorCode,
    }),
    payload: "status",
  }) as T);
}

function invalidArgument(message: string): grpc.ServiceError {
  return Object.assign(new Error(message), {
    name: "InvalidArgument",
    code: grpc.status.INVALID_ARGUMENT,
    details: message,
    metadata: new grpc.Metadata(),
  });
}

function toServiceError(error: unknown): grpc.ServiceError {
  if (isServiceError(error)) return error;
  const name = error instanceof Error ? error.name : "UnknownError";
  const details = error instanceof Error ? error.message : "unknown gateway error";
  const code = name.includes("RateLimit")
    ? grpc.status.RESOURCE_EXHAUSTED
    : name.includes("NotSupported")
      ? grpc.status.UNIMPLEMENTED
      : name.includes("BadSymbol")
        ? grpc.status.NOT_FOUND
        : name.includes("Network") || name.includes("Timeout")
          ? grpc.status.UNAVAILABLE
          : grpc.status.INTERNAL;
  return Object.assign(new Error(details), {
    name,
    code,
    details,
    metadata: new grpc.Metadata(),
  });
}

function isServiceError(error: unknown): error is grpc.ServiceError {
  return error instanceof Error && "code" in error && typeof error.code === "number";
}

function compactOptional<T extends Record<string, unknown>>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== undefined),
  ) as T;
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function bind(server: grpc.Server, address: string): Promise<void> {
  return new Promise((resolve, reject) => {
    server.bindAsync(address, grpc.ServerCredentials.createInsecure(), (error) => {
      if (error) reject(error);
      else resolve();
    });
  });
}
