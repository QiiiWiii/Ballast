// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';
import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { OrderState as _ballast_gateway_v1_OrderState, OrderState__Output as _ballast_gateway_v1_OrderState__Output } from '../../../ballast/gateway/v1/OrderState';
import type { Side as _ballast_gateway_v1_Side, Side__Output as _ballast_gateway_v1_Side__Output } from '../../../ballast/gateway/v1/Side';
import type { Long } from '@grpc/proto-loader';

export interface OrderSnapshot {
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'clientOrderId'?: (string);
  'exchangeOrderId'?: (string);
  'state'?: (_ballast_gateway_v1_OrderState);
  'side'?: (_ballast_gateway_v1_Side);
  'quantity'?: (string);
  'filledQuantity'?: (string);
  'averagePrice'?: (string);
  'reasonCode'?: (string);
  'exchangeTimeMs'?: (number | string | Long);
  'gatewayReceivedAtMs'?: (number | string | Long);
  '_exchangeOrderId'?: "exchangeOrderId";
  '_averagePrice'?: "averagePrice";
  '_reasonCode'?: "reasonCode";
}

export interface OrderSnapshot__Output {
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'clientOrderId'?: (string);
  'exchangeOrderId'?: (string);
  'state'?: (_ballast_gateway_v1_OrderState__Output);
  'side'?: (_ballast_gateway_v1_Side__Output);
  'quantity'?: (string);
  'filledQuantity'?: (string);
  'averagePrice'?: (string);
  'reasonCode'?: (string);
  'exchangeTimeMs'?: (Long);
  'gatewayReceivedAtMs'?: (Long);
}
