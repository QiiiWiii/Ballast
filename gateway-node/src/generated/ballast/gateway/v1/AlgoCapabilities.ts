// Original file: ../proto/private_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';
import type { AlgoCapability as _ballast_gateway_v1_AlgoCapability, AlgoCapability__Output as _ballast_gateway_v1_AlgoCapability__Output } from '../../../ballast/gateway/v1/AlgoCapability';

export interface AlgoCapabilities {
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'algorithms'?: (_ballast_gateway_v1_AlgoCapability)[];
}

export interface AlgoCapabilities__Output {
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'algorithms'?: (_ballast_gateway_v1_AlgoCapability__Output)[];
}
