// Original file: ../proto/exchange_gateway.proto

import type { HistoricalCandle as _ballast_gateway_v1_HistoricalCandle, HistoricalCandle__Output as _ballast_gateway_v1_HistoricalCandle__Output } from '../../../ballast/gateway/v1/HistoricalCandle';
import type { HistoricalTrade as _ballast_gateway_v1_HistoricalTrade, HistoricalTrade__Output as _ballast_gateway_v1_HistoricalTrade__Output } from '../../../ballast/gateway/v1/HistoricalTrade';
import type { Long } from '@grpc/proto-loader';

export interface FetchHistoricalBatchResponse {
  'candles'?: (_ballast_gateway_v1_HistoricalCandle)[];
  'trades'?: (_ballast_gateway_v1_HistoricalTrade)[];
  'nextCursorMs'?: (number | string | Long);
  'exhausted'?: (boolean);
  'observedAtMs'?: (number | string | Long);
}

export interface FetchHistoricalBatchResponse__Output {
  'candles'?: (_ballast_gateway_v1_HistoricalCandle__Output)[];
  'trades'?: (_ballast_gateway_v1_HistoricalTrade__Output)[];
  'nextCursorMs'?: (Long);
  'exhausted'?: (boolean);
  'observedAtMs'?: (Long);
}
