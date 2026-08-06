// Original file: ../proto/exchange_gateway.proto

export const MarketKind = {
  MARKET_KIND_UNSPECIFIED: 0,
  MARKET_KIND_SPOT: 1,
  MARKET_KIND_PERPETUAL: 2,
} as const;

export type MarketKind =
  | 'MARKET_KIND_UNSPECIFIED'
  | 0
  | 'MARKET_KIND_SPOT'
  | 1
  | 'MARKET_KIND_PERPETUAL'
  | 2

export type MarketKind__Output = typeof MarketKind[keyof typeof MarketKind]
