export type DecimalText = string;
export type ExchangeId = "binance" | "okx" | "bybit" | "gate_io" | "bitget";
export type MarketKind = "spot" | "perpetual";
export type ContractKind = "linear" | "inverse";
export type TradeSide = "buy" | "sell" | "unknown";

export interface InstrumentKey {
  readonly exchange: ExchangeId;
  readonly marketKind: MarketKind;
  readonly symbol: string;
}

export interface Instrument {
  readonly key: InstrumentKey;
  readonly exchangeSymbol: string;
  readonly baseAsset: string;
  readonly quoteAsset: string;
  readonly settleAsset?: string;
  readonly contractKind?: ContractKind;
  readonly contractSize?: DecimalText;
  readonly priceTick: DecimalText;
  readonly quantityStep: DecimalText;
  readonly minimumQuantity?: DecimalText;
  readonly minimumNotional?: DecimalText;
  readonly makerFeeRate?: DecimalText;
  readonly takerFeeRate?: DecimalText;
  readonly active: boolean;
}

export interface ExchangeCapabilities {
  readonly exchange: ExchangeId;
  readonly spot: boolean;
  readonly perpetualLinear: boolean;
  readonly perpetualInverse: boolean;
  readonly fetchOrderBook: boolean;
  readonly watchOrderBook: boolean;
  readonly watchTrades: boolean;
  readonly fetchOhlcv: boolean;
  readonly fetchTrades: boolean;
}

export interface BookLevel {
  readonly price: DecimalText;
  readonly quantity: DecimalText;
}

export interface OrderBook {
  readonly instrument: InstrumentKey;
  readonly bids: readonly BookLevel[];
  readonly asks: readonly BookLevel[];
  readonly exchangeTimeMs: number;
  readonly gatewayReceivedAtMs: number;
  readonly sequence?: string;
}

export interface Trade {
  readonly eventId: string;
  readonly instrument: InstrumentKey;
  readonly exchangeTradeId: string;
  readonly price: DecimalText;
  readonly quantity: DecimalText;
  readonly takerSide: TradeSide;
  readonly exchangeTimeMs: number;
  readonly gatewayReceivedAtMs: number;
}

export type HistoricalDataType = "ohlcv" | "trades";

export interface HistoricalCandle {
  readonly openTimeMs: number;
  readonly open: string;
  readonly high: string;
  readonly low: string;
  readonly close: string;
  readonly volume: string;
}

export interface HistoricalTrade {
  readonly exchangeTradeId: string;
  readonly tradeTimeMs: number;
  readonly price: string;
  readonly quantity: string;
  readonly takerSide: TradeSide;
}

export interface HistoricalBatch {
  readonly candles: readonly HistoricalCandle[];
  readonly trades: readonly HistoricalTrade[];
  readonly nextCursorMs: number;
  readonly exhausted: boolean;
}

export interface MarketDataAdapter {
  readonly exchange: ExchangeId;

  listInstruments(reload: boolean): Promise<readonly Instrument[]>;
  capabilities(): Promise<ExchangeCapabilities>;
  getOrderBook(instrument: InstrumentKey, depth: number): Promise<OrderBook>;
  fetchHistoricalBatch(
    instrument: InstrumentKey,
    dataType: HistoricalDataType,
    timeframe: string | undefined,
    cursorMs: number,
    endMs: number,
    limit: number,
  ): Promise<HistoricalBatch>;
  watchOrderBook(instrument: InstrumentKey, depth: number): Promise<OrderBook>;
  watchTrades(instrument: InstrumentKey): Promise<readonly Trade[]>;
  close(): Promise<void>;
}
