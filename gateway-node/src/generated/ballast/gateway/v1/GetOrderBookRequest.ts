// Original file: ../proto/exchange_gateway.proto

import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';

export interface GetOrderBookRequest {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'depth'?: (number);
}

export interface GetOrderBookRequest__Output {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'depth'?: (number);
}
