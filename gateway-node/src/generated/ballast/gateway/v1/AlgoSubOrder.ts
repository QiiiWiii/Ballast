// Original file: ../proto/private_gateway.proto

import type { OrderState as _ballast_gateway_v1_OrderState, OrderState__Output as _ballast_gateway_v1_OrderState__Output } from '../../../ballast/gateway/v1/OrderState';
import type { Long } from '@grpc/proto-loader';

export interface AlgoSubOrder {
  'exchangeSubOrderId'?: (string);
  'state'?: (_ballast_gateway_v1_OrderState);
  'quantity'?: (string);
  'filledQuantity'?: (string);
  'averagePrice'?: (string);
  'exchangeTimeMs'?: (number | string | Long);
  '_averagePrice'?: "averagePrice";
}

export interface AlgoSubOrder__Output {
  'exchangeSubOrderId'?: (string);
  'state'?: (_ballast_gateway_v1_OrderState__Output);
  'quantity'?: (string);
  'filledQuantity'?: (string);
  'averagePrice'?: (string);
  'exchangeTimeMs'?: (Long);
}
