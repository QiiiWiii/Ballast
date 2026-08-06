// Original file: ../proto/exchange_gateway.proto

import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';
import type { BookLevel as _ballast_gateway_v1_BookLevel, BookLevel__Output as _ballast_gateway_v1_BookLevel__Output } from '../../../ballast/gateway/v1/BookLevel';
import type { Long } from '@grpc/proto-loader';

export interface OrderBook {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'bids'?: (_ballast_gateway_v1_BookLevel)[];
  'asks'?: (_ballast_gateway_v1_BookLevel)[];
  'exchangeTimeMs'?: (number | string | Long);
  'gatewayReceivedAtMs'?: (number | string | Long);
  'sequence'?: (string);
  '_sequence'?: "sequence";
}

export interface OrderBook__Output {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'bids'?: (_ballast_gateway_v1_BookLevel__Output)[];
  'asks'?: (_ballast_gateway_v1_BookLevel__Output)[];
  'exchangeTimeMs'?: (Long);
  'gatewayReceivedAtMs'?: (Long);
  'sequence'?: (string);
}
