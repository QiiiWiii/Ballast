// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';
import type { InstrumentKey as _ballast_gateway_v1_InstrumentKey, InstrumentKey__Output as _ballast_gateway_v1_InstrumentKey__Output } from '../../../ballast/gateway/v1/InstrumentKey';

export interface CancelOrderRequest {
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey | null);
  'requestId'?: (string);
  'clientOrderId'?: (string);
}

export interface CancelOrderRequest__Output {
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'instrument'?: (_ballast_gateway_v1_InstrumentKey__Output);
  'requestId'?: (string);
  'clientOrderId'?: (string);
}
