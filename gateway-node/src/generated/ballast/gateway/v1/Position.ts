// Original file: ../proto/private_gateway.proto

import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';

export interface Position {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'quantity'?: (string);
  'entryPrice'?: (string);
  'markPrice'?: (string);
  'unrealizedPnl'?: (string);
  '_entryPrice'?: "entryPrice";
  '_markPrice'?: "markPrice";
  '_unrealizedPnl'?: "unrealizedPnl";
}

export interface Position__Output {
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'quantity'?: (string);
  'entryPrice'?: (string);
  'markPrice'?: (string);
  'unrealizedPnl'?: (string);
}
