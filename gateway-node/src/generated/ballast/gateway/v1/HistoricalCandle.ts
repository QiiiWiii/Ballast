// Original file: ../proto/exchange_gateway.proto

import type { Long } from '@grpc/proto-loader';

export interface HistoricalCandle {
  'openTimeMs'?: (number | string | Long);
  'open'?: (string);
  'high'?: (string);
  'low'?: (string);
  'close'?: (string);
  'volume'?: (string);
}

export interface HistoricalCandle__Output {
  'openTimeMs'?: (Long);
  'open'?: (string);
  'high'?: (string);
  'low'?: (string);
  'close'?: (string);
  'volume'?: (string);
}
