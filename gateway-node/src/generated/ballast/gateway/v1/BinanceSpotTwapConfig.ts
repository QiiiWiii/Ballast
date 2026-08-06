// Original file: ../proto/private_gateway.proto

import type { Long } from '@grpc/proto-loader';

export interface BinanceSpotTwapConfig {
  'durationSeconds'?: (number | string | Long);
  'limitPrice'?: (string);
}

export interface BinanceSpotTwapConfig__Output {
  'durationSeconds'?: (Long);
  'limitPrice'?: (string);
}
