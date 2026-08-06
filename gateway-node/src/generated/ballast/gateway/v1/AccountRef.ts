// Original file: ../proto/private_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';
import type { AccountEnvironment as _ballast_gateway_v1_AccountEnvironment, AccountEnvironment__Output as _ballast_gateway_v1_AccountEnvironment__Output } from '../../../ballast/gateway/v1/AccountEnvironment';

export interface AccountRef {
  'accountId'?: (string);
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'environment'?: (_ballast_gateway_v1_AccountEnvironment);
}

export interface AccountRef__Output {
  'accountId'?: (string);
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'environment'?: (_ballast_gateway_v1_AccountEnvironment__Output);
}
