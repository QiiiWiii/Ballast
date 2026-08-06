// Original file: ../proto/exchange_gateway.proto

import type { Exchange as _ballast_gateway_v1_Exchange, Exchange__Output as _ballast_gateway_v1_Exchange__Output } from '../../../ballast/gateway/v1/Exchange';
import type { MarketKind as _ballast_gateway_v1_MarketKind, MarketKind__Output as _ballast_gateway_v1_MarketKind__Output } from '../../../ballast/gateway/v1/MarketKind';
import type { ContractKind as _ballast_gateway_v1_ContractKind, ContractKind__Output as _ballast_gateway_v1_ContractKind__Output } from '../../../ballast/gateway/v1/ContractKind';

export interface ListInstrumentsRequest {
  'exchange'?: (_ballast_gateway_v1_Exchange);
  'marketKind'?: (_ballast_gateway_v1_MarketKind);
  'contractKind'?: (_ballast_gateway_v1_ContractKind);
  'activeOnly'?: (boolean);
  'reload'?: (boolean);
}

export interface ListInstrumentsRequest__Output {
  'exchange'?: (_ballast_gateway_v1_Exchange__Output);
  'marketKind'?: (_ballast_gateway_v1_MarketKind__Output);
  'contractKind'?: (_ballast_gateway_v1_ContractKind__Output);
  'activeOnly'?: (boolean);
  'reload'?: (boolean);
}
