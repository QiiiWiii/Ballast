// Original file: ../proto/private_gateway.proto

import type { MarketKind as _ballast_gateway_v1_MarketKind, MarketKind__Output as _ballast_gateway_v1_MarketKind__Output } from '../../../ballast/gateway/v1/MarketKind';

export interface AlgoCapability {
  'algorithm'?: (string);
  'marketKind'?: (_ballast_gateway_v1_MarketKind);
  'submit'?: (boolean);
  'query'?: (boolean);
  'cancel'?: (boolean);
  'listSubOrders'?: (boolean);
  'fillReconciliation'?: (boolean);
  'protectedPrice'?: (boolean);
  'validationStatus'?: (string);
  'reasonCode'?: (string);
  '_reasonCode'?: "reasonCode";
}

export interface AlgoCapability__Output {
  'algorithm'?: (string);
  'marketKind'?: (_ballast_gateway_v1_MarketKind__Output);
  'submit'?: (boolean);
  'query'?: (boolean);
  'cancel'?: (boolean);
  'listSubOrders'?: (boolean);
  'fillReconciliation'?: (boolean);
  'protectedPrice'?: (boolean);
  'validationStatus'?: (string);
  'reasonCode'?: (string);
}
