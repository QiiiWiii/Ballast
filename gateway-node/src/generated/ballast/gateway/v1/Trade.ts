// Original file: ../proto/exchange_gateway.proto

import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { Side as _ballast_gateway_v1_Side, Side__Output as _ballast_gateway_v1_Side__Output } from '../../../ballast/gateway/v1/Side';
import type { Long } from '@grpc/proto-loader';

export interface Trade {
  'eventId'?: (string);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'exchangeTradeId'?: (string);
  'price'?: (string);
  'quantity'?: (string);
  'takerSide'?: (_ballast_gateway_v1_Side);
  'exchangeTimeMs'?: (number | string | Long);
  'gatewayReceivedAtMs'?: (number | string | Long);
}

export interface Trade__Output {
  'eventId'?: (string);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'exchangeTradeId'?: (string);
  'price'?: (string);
  'quantity'?: (string);
  'takerSide'?: (_ballast_gateway_v1_Side__Output);
  'exchangeTimeMs'?: (Long);
  'gatewayReceivedAtMs'?: (Long);
}
