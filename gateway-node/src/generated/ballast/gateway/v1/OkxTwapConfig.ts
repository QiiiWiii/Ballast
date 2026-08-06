// Original file: ../proto/private_gateway.proto

import type { Long } from '@grpc/proto-loader';

export interface OkxTwapConfig {
  'tradeMode'?: (string);
  'positionSide'?: (string);
  'sizeLimit'?: (string);
  'priceLimit'?: (string);
  'timeIntervalSeconds'?: (number | string | Long);
  'priceVarianceRatio'?: (string);
  'priceSpread'?: (string);
  'reduceOnly'?: (boolean);
  'priceVariance'?: "priceVarianceRatio"|"priceSpread";
}

export interface OkxTwapConfig__Output {
  'tradeMode'?: (string);
  'positionSide'?: (string);
  'sizeLimit'?: (string);
  'priceLimit'?: (string);
  'timeIntervalSeconds'?: (Long);
  'priceVarianceRatio'?: (string);
  'priceSpread'?: (string);
  'reduceOnly'?: (boolean);
}
