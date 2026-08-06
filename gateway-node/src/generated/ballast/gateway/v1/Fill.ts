// Original file: ../proto/private_gateway.proto

import type { Long } from '@grpc/proto-loader';

export interface Fill {
  'eventId'?: (string);
  'exchangeTradeId'?: (string);
  'clientOrderId'?: (string);
  'price'?: (string);
  'quantity'?: (string);
  'fee'?: (string);
  'feeAsset'?: (string);
  'exchangeTimeMs'?: (number | string | Long);
  '_fee'?: "fee";
  '_feeAsset'?: "feeAsset";
}

export interface Fill__Output {
  'eventId'?: (string);
  'exchangeTradeId'?: (string);
  'clientOrderId'?: (string);
  'price'?: (string);
  'quantity'?: (string);
  'fee'?: (string);
  'feeAsset'?: (string);
  'exchangeTimeMs'?: (Long);
}
