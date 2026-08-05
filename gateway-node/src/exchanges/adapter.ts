export type DecimalText = string;

export type ExchangeId = "binance" | "okx" | "bybit" | "gate_io" | "bitget";
export type MarketKind = "spot" | "perpetual" | "future";

export interface InstrumentKey {
  readonly exchange: ExchangeId;
  readonly marketKind: MarketKind;
  readonly symbol: string;
}

export interface Instrument {
  readonly key: InstrumentKey;
  readonly baseAsset: string;
  readonly quoteAsset: string;
  readonly priceTick: DecimalText;
  readonly quantityStep: DecimalText;
  readonly minimumQuantity: DecimalText;
  readonly minimumNotional: DecimalText;
  readonly active: boolean;
}

export interface ExchangeCapabilities {
  readonly marketOrder: boolean;
  readonly limitOrder: boolean;
  readonly postOnly: boolean;
  readonly reduceOnly: boolean;
  readonly clientOrderId: boolean;
  readonly privateOrderStream: boolean;
  readonly amendOrder: boolean;
}

export interface ExchangeAdapter {
  readonly exchange: ExchangeId;

  loadInstruments(marketKind: MarketKind): Promise<readonly Instrument[]>;
  capabilities(marketKind: MarketKind): ExchangeCapabilities;
}

// Implementations are intentionally added one exchange at a time. The gateway
// must not expose a generic `callCcxt(method, params)` escape hatch.
