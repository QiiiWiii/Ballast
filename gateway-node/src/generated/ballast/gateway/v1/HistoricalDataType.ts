// Original file: ../proto/exchange_gateway.proto

export const HistoricalDataType = {
  HISTORICAL_DATA_TYPE_UNSPECIFIED: 0,
  HISTORICAL_DATA_TYPE_OHLCV: 1,
  HISTORICAL_DATA_TYPE_TRADES: 2,
} as const;

export type HistoricalDataType =
  | 'HISTORICAL_DATA_TYPE_UNSPECIFIED'
  | 0
  | 'HISTORICAL_DATA_TYPE_OHLCV'
  | 1
  | 'HISTORICAL_DATA_TYPE_TRADES'
  | 2

export type HistoricalDataType__Output = typeof HistoricalDataType[keyof typeof HistoricalDataType]
