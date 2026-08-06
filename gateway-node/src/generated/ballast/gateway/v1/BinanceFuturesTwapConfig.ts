// Original file: ../proto/private_gateway.proto

import type { Long } from '@grpc/proto-loader';

export interface BinanceFuturesTwapConfig {
  'durationSeconds'?: (number | string | Long);
  'limitPrice'?: (string);
  'positionSide'?: (string);
  'reduceOnly'?: (boolean);
}

export interface BinanceFuturesTwapConfig__Output {
  'durationSeconds'?: (Long);
  'limitPrice'?: (string);
  'positionSide'?: (string);
  'reduceOnly'?: (boolean);
}
