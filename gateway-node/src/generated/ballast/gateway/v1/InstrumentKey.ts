// Original file: ../proto/exchange_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';
import type { MarketKind as _ballast_gateway_v1_MarketKind, MarketKind__Output as _ballast_gateway_v1_MarketKind__Output } from '../../../ballast/gateway/v1/MarketKind';

export interface InstrumentKey {
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'marketKind'?: (_ballast_gateway_v1_MarketKind);
  'symbol'?: (string);
}

export interface InstrumentKey__Output {
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'marketKind'?: (_ballast_gateway_v1_MarketKind__Output);
  'symbol'?: (string);
}
