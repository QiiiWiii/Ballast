// Original file: ../proto/exchange_gateway.proto

export const Exchange = {
  EXCHANGE_UNSPECIFIED: 0,
  EXCHANGE_BINANCE: 1,
  EXCHANGE_OKX: 2,
  EXCHANGE_BYBIT: 3,
  EXCHANGE_GATE_IO: 4,
  EXCHANGE_BITGET: 5,
} as const;

export type Exchange =
  | 'EXCHANGE_UNSPECIFIED'
  | 0
  | 'EXCHANGE_BINANCE'
  | 1
  | 'EXCHANGE_OKX'
  | 2
  | 'EXCHANGE_BYBIT'
  | 3
  | 'EXCHANGE_GATE_IO'
  | 4
  | 'EXCHANGE_BITGET'
  | 5

export type Exchange__Output = typeof Exchange[keyof typeof Exchange]
