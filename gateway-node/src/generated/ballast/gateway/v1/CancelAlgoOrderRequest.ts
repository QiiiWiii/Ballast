// Original file: ../proto/private_gateway.proto

import type { AccountRef as _ballast_gateway_v1_AccountRef, AccountRef__Output as _ballast_gateway_v1_AccountRef__Output } from '../../../ballast/gateway/v1/AccountRef';

export interface CancelAlgoOrderRequest {
  'account'?: (_ballast_gateway_v1_AccountRef | null);
  'requestId'?: (string);
  'clientAlgoId'?: (string);
}

export interface CancelAlgoOrderRequest__Output {
  'account'?: (_ballast_gateway_v1_AccountRef__Output);
  'requestId'?: (string);
  'clientAlgoId'?: (string);
}
